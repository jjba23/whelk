// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Rust-owned shell surfaces.
//!
//! One bar is built for every monitor selected by its declarative rule.  The
//! display's monitor model and the settings file are both watched: a hotplug or
//! a save tears the old windows down and evaluates the rules again.  The old
//! windows own their modules, so destroying a window stops its timers and
//! cancels its in-flight commands through the shared module host.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge, Layer as BarLayer, ModuleSpec};
use crate::barfactory::{self, ModuleContext};
use crate::barmonitors::{BarPlacement, DEFAULT_MONITOR, plan_bars};
use crate::clipboard::Clipboard;
use crate::config::{ConfigLoader, LoadedConfig};
use crate::idle::Idle;
use crate::launcherpanel::Launcher;
use crate::locker;
use crate::networkpanel::NetworkPanel;
use crate::notificationcenter::NotificationCenter;
use crate::notifications::NotificationService;
use crate::osd::Osd;
use crate::session::Session;
use crate::settings::WallpaperSettings;
use crate::startmenu::MenuEntry;
use crate::style;

const CONFIG_RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

thread_local! {
    /// One manager owns the idle timer, its logind inhibitor and DND, for as
    /// long as the shell runs.
    static IDLE: std::cell::RefCell<Option<Idle>> = const { std::cell::RefCell::new(None) };
}

struct Bootstrap {
    loader: ConfigLoader,
    loaded: LoadedConfig,
    config_label: String,
    launcher: Option<String>,
    hold: gtk::gio::ApplicationHoldGuard,
}

type ShellRef = Rc<RefCell<Shell>>;
type ShellSlot = Rc<RefCell<Option<ShellRef>>>;

pub fn run(
    loader: ConfigLoader,
    loaded: LoadedConfig,
    config_label: String,
    launcher: Option<String>,
) -> gtk::glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("org.jointhefreeworld.whelk.rust")
        .build();

    // A configuration can intentionally have no bar on the current outputs
    // (a named external connector, or `#:when 'multi`).  Hold the application
    // so it can wait for the monitor that makes the rule true.
    let hold = application.hold();

    let bootstrap = Rc::new(RefCell::new(Some(Bootstrap {
        loader,
        loaded,
        config_label,
        launcher,
        hold,
    })));
    let slot: ShellSlot = Rc::new(RefCell::new(None));

    let activate_slot = slot.clone();
    let activate_bootstrap = bootstrap.clone();
    application.connect_activate(move |application| {
        if activate_slot.borrow().is_some() {
            return;
        }

        let Some(bootstrap) = activate_bootstrap.borrow_mut().take() else {
            return;
        };
        let shell = Shell::present(
            application,
            bootstrap.loader,
            bootstrap.loaded,
            bootstrap.config_label,
            bootstrap.launcher,
            bootstrap.hold,
        );

        *activate_slot.borrow_mut() = Some(shell);
    });

    let shutdown_slot = slot.clone();
    application.connect_shutdown(move |_application| {
        if let Some(shell) = shutdown_slot.borrow_mut().take() {
            shell.borrow_mut().teardown();
        }
    });

    let quit = gtk::gio::SimpleAction::new("quit", None);
    let application_weak = application.downgrade();

    quit.connect_activate(move |_, _| {
        if let Some(application) = application_weak.upgrade() {
            application.quit();
        }
    });

    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);

    // The shell menu's reload row, and the default menu's `whelk --reload`,
    // both ask the running shell to re-read its settings.  A second process
    // would be a second shell, so the request stays in this application.
    let reload = gtk::gio::SimpleAction::new("reload", None);
    let reload_slot = slot.clone();

    reload.connect_activate(move |_, _| {
        if let Some(shell) = reload_slot.borrow().as_ref() {
            shell.borrow_mut().reload();
        }
    });
    application.add_action(&reload);

    let launcher = gtk::gio::SimpleAction::new("launcher", Some(gtk::glib::VariantTy::STRING));

    launcher.connect_activate(|_, parameter| {
        let mode = parameter
            .and_then(|parameter| parameter.get::<String>())
            .unwrap_or_default();

        Launcher::show_current((!mode.is_empty()).then_some(mode.as_str()));
    });
    application.add_action(&launcher);

    // main() has already consumed whelk's options; give GTK only a program
    // name, or it will reject --config as one of its own.
    application.run_with_args(&["whelk-rs"])
}

struct Shell {
    application: gtk::glib::WeakRef<gtk::Application>,
    loader: ConfigLoader,
    loaded: LoadedConfig,
    config_label: String,
    session: Session,
    bars: Vec<BarWindow>,
    config_watch: Option<(gtk::gio::FileMonitor, gtk::glib::SignalHandlerId)>,
    monitor_watch: Option<(gtk::gio::ListModel, gtk::glib::SignalHandlerId)>,
    reload_source: Option<gtk::glib::SourceId>,
    monitor_rebuild_pending: bool,
    notifications: Option<NotificationService>,
    clipboard: Option<Clipboard>,
    center: Option<NotificationCenter>,
    network: Option<NetworkPanel>,
    launcher: Option<Launcher>,
    osd: Option<Osd>,
    _hold: gtk::gio::ApplicationHoldGuard,
}

impl Shell {
    fn present(
        application: &gtk::Application,
        loader: ConfigLoader,
        loaded: LoadedConfig,
        config_label: String,
        launcher_mode: Option<String>,
        hold: gtk::gio::ApplicationHoldGuard,
    ) -> ShellRef {
        // The C shell prepares the locker at startup, before a `--lock` or a
        // logind signal can arrive; the Rust shell watches logind at the same
        // point and starts `whelk-rs --lock` when a trigger fires.
        locker::watch_logind();
        start_idle(loaded.idle_lock_after);

        // The notification service shares the idle manager's DND flag.  It is
        // installed before the first bar is built so the notifications and
        // dnd modules see their reading immediately.
        let notifications = NotificationService::new(idle());

        notifications.start();
        NotificationService::install(Some(notifications.clone()));

        // The clipboard starts before the first bar so its module and the
        // launcher see the same live history from the beginning.
        let clipboard = Clipboard::new(loaded.clipboard.clone());

        clipboard.start();
        Clipboard::install(Some(clipboard.clone()));

        let osd_style = loaded.bars.first().cloned().unwrap_or_default();
        let center = NotificationCenter::new(application, &notifications, &osd_style);
        let network = NetworkPanel::new(application, &osd_style);
        let launcher = Launcher::new(application, &osd_style, Some(clipboard.clone()));
        let osd = Osd::new(application, &osd_style);

        NotificationCenter::install(Some(center.clone()));
        NetworkPanel::install(Some(network.clone()));
        Launcher::install(Some(launcher.clone()));
        Osd::install(Some(osd.clone()));

        let shell = Rc::new(RefCell::new(Self {
            application: application.downgrade(),
            loader,
            loaded,
            config_label,
            session: Session::detect(),
            bars: Vec::new(),
            config_watch: None,
            monitor_watch: None,
            reload_source: None,
            monitor_rebuild_pending: false,
            notifications: Some(notifications),
            clipboard: Some(clipboard),
            center: Some(center),
            network: Some(network),
            launcher: Some(launcher),
            osd: Some(osd),
            _hold: hold,
        }));

        {
            let mut state = shell.borrow_mut();

            state.rebuild();
        }

        watch_config(&shell);
        watch_monitors(&shell);

        let wallpaper = shell.borrow().loaded.wallpaper.clone();

        crate::wallpaper_surface::present(application, &wallpaper);

        if let Some(mode) = launcher_mode {
            gtk::glib::idle_add_local_once(move || {
                Launcher::show_current((!mode.is_empty()).then_some(mode.as_str()));
            });
        }

        shell
    }

    /// Destroy every current bar and build the set the current monitor model
    /// and loaded configuration select.  This is the only rebuild path: a
    /// monitor change and a settings save both land here.
    fn rebuild(&mut self) {
        let Some(application) = self.application.upgrade() else {
            self.bars.clear();

            return;
        };

        self.bars.clear();

        let (monitors, connectors) = monitor_snapshot();
        let placements = plan_bars(&self.loaded.bars, &connectors);
        let mut built = 0;

        for placement in placements {
            let Some(bar_settings) = self.loaded.bars.get(placement.bar_index) else {
                continue;
            };
            let monitor = placement_monitor(&placement, &monitors);

            self.bars.push(build_bar_window(
                &application,
                bar_settings,
                monitor,
                &self.config_label,
                &self.loaded.wallpaper,
                &self.loaded.menu,
                self.session,
            ));
            built += 1;
        }

        if built == 0 {
            eprintln!("whelk-rs: no bar matches the current monitor model; waiting for an output");
        }
    }

    fn reload(&mut self) {
        match self.loader.load() {
            Ok(loaded) => {
                self.loaded = loaded;
                self.config_label = self.loader.label();
                start_idle(self.loaded.idle_lock_after);

                let style = self.loaded.bars.first().cloned().unwrap_or_default();

                if let Some(center) = self.center.as_ref() {
                    center.set_style(&style);
                }

                if let Some(network) = self.network.as_ref() {
                    network.set_style(&style);
                }

                if let Some(clipboard) = self.clipboard.as_ref() {
                    clipboard.apply_settings(&self.loaded.clipboard);
                }

                if let Some(launcher) = self.launcher.as_ref() {
                    launcher.set_style(&style);
                }

                if let Some(osd) = self.osd.as_ref() {
                    osd.set_style(&style);
                }

                self.rebuild();
            }
            Err(error) => {
                eprintln!(
                    "whelk-rs: configuration reload failed: {error}; keeping the previous bars"
                );
            }
        }
    }

    fn teardown(&mut self) {
        Osd::install(None);
        Launcher::install(None);
        NetworkPanel::install(None);
        NotificationCenter::install(None);
        Clipboard::install(None);
        NotificationService::install(None);

        self.launcher.take();
        self.network.take();
        self.center.take();
        self.osd.take();

        if let Some(service) = self.notifications.take() {
            service.shutdown();
        }

        if let Some(source) = self.reload_source.take() {
            source.remove();
        }

        if let Some((monitor, handler)) = self.config_watch.take() {
            monitor.disconnect(handler);
        }

        if let Some((model, handler)) = self.monitor_watch.take() {
            model.disconnect(handler);
        }

        // BarWindow's Drop removes its fallback clock source and CSS provider,
        // then destroys the window.  Destroying the window shuts down every
        // ModuleHost beneath it, cancelling timers and commands.
        self.bars.clear();

        // The history flushes synchronously when the service's last reference
        // goes away, after the bar and launcher listeners are gone.
        if let Some(clipboard) = self.clipboard.take() {
            clipboard.shutdown();
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.teardown();
    }
}

struct BarWindow {
    window: gtk::ApplicationWindow,
    clock_timer: Option<ClockTimer>,
    css: Option<gtk::CssProvider>,
}

struct ClockTimer {
    source: Rc<RefCell<Option<gtk::glib::SourceId>>>,
    alive: Rc<Cell<bool>>,
}

impl Drop for ClockTimer {
    fn drop(&mut self) {
        self.alive.set(false);

        if let Some(source) = self.source.borrow_mut().take() {
            source.remove();
        }
    }
}

impl Drop for BarWindow {
    fn drop(&mut self) {
        drop(self.clock_timer.take());

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        // Detach the CenterBox explicitly so the module roots receive their
        // destroy signal before the window itself goes away.
        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

fn build_bar_window(
    application: &gtk::Application,
    bar_settings: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
    config_label: &str,
    wallpaper_settings: &WallpaperSettings,
    shell_menu: &[MenuEntry],
    session: Session,
) -> BarWindow {
    let widget_id = style::widget_id(&bar_settings.name);
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk-rs")
        .build();

    window.set_widget_name(&widget_id);
    window.add_css_class("whelk-surface");
    window.set_decorated(false);
    window.set_resizable(false);

    let bar = gtk::CenterBox::new();
    bar.add_css_class("whelk-bar");
    bar.set_size_request(-1, bar_settings.height);

    let name = gtk::Label::new(Some("whelk"));
    name.add_css_class("whelk-rs-name");
    name.set_tooltip_text(Some(config_label));

    let wallpaper = gtk::Label::new(Some(&wallpaper_settings.summary()));
    wallpaper.add_css_class("whelk-rs-status");
    wallpaper.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let start = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));
    let center = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));
    let end = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));

    start.add_css_class("whelk-rs-start");
    center.add_css_class("whelk-rs-center");
    end.add_css_class("whelk-rs-end");
    start.append(&name);

    let mut context = ModuleContext::from_bar(bar_settings, session);

    context.shell_menu = Rc::new(shell_menu.to_vec());

    append_modules(&start, &bar_settings.left, &context);
    append_modules(&center, &bar_settings.center, &context);
    append_modules(&end, &bar_settings.right, &context);

    if center.first_child().is_none() {
        center.append(&wallpaper);
    }

    // A clock module is itself one of the readings now.  Only a configuration
    // without one needs the bootstrap clock, so a configured clock is not
    // duplicated in the right half.
    let configured_clock = bar_settings.module("clock").is_some()
        || bar_settings
            .overflow
            .iter()
            .any(|module| module.kind == "clock");
    let clock_timer = if !configured_clock {
        let clock = gtk::Label::new(None);
        let clock_format = bar_settings
            .clock_format()
            .unwrap_or_else(|| "%H:%M".to_owned());
        let weak_clock = clock.downgrade();
        let source = Rc::new(RefCell::new(None));
        let alive = Rc::new(Cell::new(true));
        let callback_source = source.clone();
        let callback_alive = alive.clone();
        let timer_format = clock_format.clone();

        let timer = gtk::glib::timeout_add_seconds_local(1, move || {
            if !callback_alive.get() {
                return gtk::glib::ControlFlow::Break;
            }

            let Some(clock) = weak_clock.upgrade() else {
                *callback_source.borrow_mut() = None;

                return gtk::glib::ControlFlow::Break;
            };

            update_clock(&clock, &timer_format);

            gtk::glib::ControlFlow::Continue
        });

        *source.borrow_mut() = Some(timer);

        clock.add_css_class("whelk-rs-clock");
        update_clock(&clock, &clock_format);
        end.append(&clock);

        Some(ClockTimer { source, alive })
    } else {
        None
    };

    append_overflow(&end, bar_settings, &context);

    bar.set_start_widget(Some(&start));
    bar.set_center_widget(Some(&center));
    bar.set_end_widget(Some(&end));
    window.set_child(Some(&bar));

    let css = install_css(bar_settings);

    if layer_shell_available() {
        configure_layer_shell(&window, bar_settings, monitor);
    } else {
        // Broadway and a plain X11 test display have no layer shell.  A normal
        // window is still a useful smoke test for the widget tree.
        window.set_default_size(720, bar_settings.height.max(32));
    }

    window.present();

    BarWindow {
        window,
        clock_timer,
        css,
    }
}

fn placement_monitor<'a>(
    placement: &BarPlacement,
    monitors: &'a [gtk::gdk::Monitor],
) -> Option<&'a gtk::gdk::Monitor> {
    if placement.monitor == DEFAULT_MONITOR {
        return None;
    }

    monitors.get(placement.monitor as usize)
}

fn monitor_snapshot() -> (Vec<gtk::gdk::Monitor>, Vec<String>) {
    let Some(display) = gtk::gdk::Display::default() else {
        return (Vec::new(), Vec::new());
    };
    let model = display.monitors();
    let mut monitors = Vec::new();
    let mut connectors = Vec::new();

    for index in 0..model.n_items() {
        let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let connector = monitor.connector().unwrap_or_default().to_string();

        monitors.push(monitor);
        connectors.push(connector);
    }

    (monitors, connectors)
}

fn watch_config(shell: &ShellRef) {
    let path = shell.borrow().loader.path().to_owned();
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if let Err(error) = std::fs::create_dir_all(directory) {
        eprintln!(
            "whelk-rs: cannot create the configuration directory '{}': {error}",
            directory.display()
        );

        return;
    }

    let directory = gtk::gio::File::for_path(directory);
    let monitor = match directory.monitor_directory(
        gtk::gio::FileMonitorFlags::NONE,
        None::<&gtk::gio::Cancellable>,
    ) {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("whelk-rs: cannot watch the configuration directory: {error}");

            return;
        }
    };
    let weak = Rc::downgrade(shell);
    let handler = monitor.connect_changed(move |_, _, _, _| {
        if let Some(shell) = weak.upgrade() {
            schedule_reload(&shell);
        }
    });

    shell.borrow_mut().config_watch = Some((monitor, handler));
}

fn schedule_reload(shell: &ShellRef) {
    let mut state = shell.borrow_mut();

    if let Some(source) = state.reload_source.take() {
        source.remove();
    }

    let weak = Rc::downgrade(shell);

    state.reload_source = Some(gtk::glib::timeout_add_local_once(
        CONFIG_RELOAD_DEBOUNCE,
        move || {
            if let Some(shell) = weak.upgrade() {
                let mut state = shell.borrow_mut();

                state.reload_source = None;
                state.reload();
            }
        },
    ));
}

fn watch_monitors(shell: &ShellRef) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let model = display.monitors();
    let weak = Rc::downgrade(shell);
    let handler = model.connect_items_changed(move |_, _, _, _| {
        if let Some(shell) = weak.upgrade() {
            schedule_monitor_rebuild(&shell);
        }
    });

    shell.borrow_mut().monitor_watch = Some((model, handler));
}

fn schedule_monitor_rebuild(shell: &ShellRef) {
    let mut state = shell.borrow_mut();

    if state.monitor_rebuild_pending {
        return;
    }

    state.monitor_rebuild_pending = true;
    let weak = Rc::downgrade(shell);

    // Rebuilding while the monitor model is still emitting is reentrancy-prone:
    // a bar being destroyed queries the same model.  The idle runs after the
    // model has settled, just like the C shell's monitor idle.
    gtk::glib::idle_add_local_once(move || {
        if let Some(shell) = weak.upgrade() {
            let mut state = shell.borrow_mut();

            state.monitor_rebuild_pending = false;
            state.rebuild();
        }
    });
}

/// Starts the idle manager once; a later reload only updates the policy, so a
/// second activation does not start a second logind session lookup.
fn start_idle(lock_after: i32) {
    IDLE.with(|slot| {
        let mut slot = slot.borrow_mut();

        if let Some(idle) = slot.as_ref() {
            idle.set_lock_after(lock_after);

            return;
        }

        let idle = Idle::new();

        idle.set_lock_action(locker::start_lock_screen);
        idle.set_lock_after(lock_after);
        idle.start();

        *slot = Some(idle);
    });
}

fn idle() -> Option<Idle> {
    IDLE.with(|slot| slot.borrow().clone())
}

fn configure_layer_shell(
    window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
) {
    window.init_layer_shell();
    window.set_layer(layer_for(bar.layer));
    window.set_namespace(Some("whelk-rust"));

    let edge = edge_for(bar.edge);

    window.set_anchor(edge, true);

    match bar.edge {
        BarEdge::Top | BarEdge::Bottom => {
            window.set_anchor(Edge::Left, true);
            window.set_anchor(Edge::Right, true);
        }
        BarEdge::Left | BarEdge::Right => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Bottom, true);
        }
    }

    window.set_margin(Edge::Top, bar.margin_top);
    window.set_margin(Edge::Right, bar.margin_right);
    window.set_margin(Edge::Bottom, bar.margin_bottom);
    window.set_margin(Edge::Left, bar.margin_left);
    window.set_exclusive_zone(if bar.exclusive { bar.height } else { -1 });
    window.set_keyboard_mode(KeyboardMode::None);

    if let Some(monitor) = monitor {
        window.set_monitor(Some(monitor));
    }
}

fn edge_for(edge: BarEdge) -> Edge {
    match edge {
        BarEdge::Top => Edge::Top,
        BarEdge::Bottom => Edge::Bottom,
        BarEdge::Left => Edge::Left,
        BarEdge::Right => Edge::Right,
    }
}

fn layer_for(layer: BarLayer) -> Layer {
    match layer {
        BarLayer::Background => Layer::Background,
        BarLayer::Bottom => Layer::Bottom,
        BarLayer::Top => Layer::Top,
        BarLayer::Overlay => Layer::Overlay,
    }
}

pub(crate) fn layer_shell_available() -> bool {
    // gtk4-layer-shell is about the Wayland protocol, not merely the library.
    // On Broadway or X11, init_layer_shell() asserts; keep those backends on
    // the ordinary-window path.
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };

    // Check the backend first: gtk_layer_is_supported() itself asserts on a
    // non-Wayland display instead of quietly answering false.
    display.backend().is_wayland() && gtk4_layer_shell::is_supported()
}

fn append_modules(section: &gtk::Box, modules: &[ModuleSpec], context: &ModuleContext) {
    for module in modules {
        if let Some(widget) = barfactory::build(module, context) {
            section.append(&widget);
        }
    }
}

/// The configured overflow row: built with the bar so its modules have already
/// started, but hidden behind the chevron until it is opened.  This is the
/// Rust equivalent of C's inline `#:overflow` area; the width-driven folded
/// menu is still a later slice.
fn append_overflow(end: &gtk::Box, bar_settings: &BarSettings, context: &ModuleContext) {
    if bar_settings.overflow.is_empty() {
        return;
    }

    let expanded = bar_settings.overflow_expanded;
    let overflow = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));
    overflow.add_css_class("whelk-overflow");
    overflow.set_valign(gtk::Align::Center);
    append_modules(&overflow, &bar_settings.overflow, context);

    let toggle = gtk::ToggleButton::new();
    let icon = gtk::Image::from_icon_name("pan-start-symbolic");

    icon.set_pixel_size(16);
    toggle.set_child(Some(&icon));
    toggle.add_css_class("flat");
    toggle.add_css_class("whelk-overflow-toggle");
    toggle.set_valign(gtk::Align::Center);
    toggle.set_active(expanded);
    overflow.set_visible(expanded);
    icon.set_icon_name(Some(if expanded {
        "pan-end-symbolic"
    } else {
        "pan-start-symbolic"
    }));
    toggle.set_tooltip_text(Some(if expanded {
        "Extra items · click to hide"
    } else {
        "Extra items · click to show"
    }));

    let overflow_for_toggle = overflow.clone();
    let icon_for_toggle = icon.clone();

    toggle.connect_toggled(move |button| {
        let expanded = button.is_active();

        overflow_for_toggle.set_visible(expanded);
        icon_for_toggle.set_icon_name(Some(if expanded {
            "pan-end-symbolic"
        } else {
            "pan-start-symbolic"
        }));
        button.set_tooltip_text(Some(if expanded {
            "Extra items · click to hide"
        } else {
            "Extra items · click to show"
        }));
    });

    end.append(&overflow);
    end.append(&toggle);
}

fn update_clock(label: &gtk::Label, format: &str) {
    let text = gtk::glib::DateTime::now_local()
        .and_then(|now| now.format(format))
        .map(|formatted| formatted.to_string())
        .unwrap_or_else(|_| "--:--".to_owned());

    label.set_text(&text);
}

fn install_css(bar: &BarSettings) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let widget_id = style::widget_id(&bar.name);
    let base = "
        .whelk-surface {
            background-color: transparent;
        }
        .whelk-rs-bar {
            padding: 0;
        }
        .whelk-rs-name {
            font-weight: 800;
        }
        .whelk-rs-status {
            opacity: 0.72;
        }
        .whelk-rs-clock {
            font-weight: 700;
        }
        .whelk-module {
            padding: 0 6px;
            border-radius: 6px;
            transition: background-color 120ms ease-in-out;
        }
        .whelk-module:hover {
            background-color: alpha(currentColor, 0.10);
        }
        .whelk-label {
            font-feature-settings: \"tnum\";
        }
        .whelk-battery.warning {
            color: #ffb340;
        }
        .whelk-battery.critical {
            color: #ff5f56;
            font-weight: bold;
        }
        .whelk-wifi.off, .whelk-bluetooth.off {
            opacity: 0.55;
        }
        .whelk-window {
            font-weight: 600;
        }
        .whelk-workspace {
            padding: 0 3px;
        }
        .whelk-workspace.inactive {
            opacity: 0.62;
        }
        .whelk-workspace.empty {
            opacity: 0.38;
        }
        ";
    let generated = style::css(&widget_id, bar, "whelk-bar", bar.css.as_deref());
    let css = gtk::CssProvider::new();

    css.load_from_string(&format!("{base}\n{generated}"));

    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(css)
}

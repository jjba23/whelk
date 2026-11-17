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

use gtk::gio::prelude::ApplicationCommandLineExt;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::about;
use crate::bar::{BarSettings, Edge as BarEdge, Layer as BarLayer, ModuleSpec};
use crate::barfactory::{self, ModuleContext};
use crate::barmonitors::{BarPlacement, DEFAULT_MONITOR, plan_bars};
use crate::cli::{FORWARDED_OPTIONS, Request, Requests};
use crate::clipboard::Clipboard;
use crate::config::{ConfigLoader, LoadedConfig};
use crate::desktopwidgets::Widgets;
use crate::frost;
use crate::idle::Idle;
use crate::launcherpanel::Launcher;
use crate::locker;
use crate::menuapps::MenuSources;
use crate::networkpanel::NetworkPanel;
use crate::notificationcenter::NotificationCenter;
use crate::notifications::NotificationService;
use crate::osd::Osd;
use crate::preferences::{self, PreferenceValue, Preferences, ReadFn, ReloadFn, SaveFn, Snapshot};
use crate::session::Session;
use crate::sources;
use crate::startmenu::MenuEntry;
use crate::style;
use crate::wallpapermenu::WallpaperMenu;
use crate::wallpaperpicker::Picker;

const CONFIG_RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

thread_local! {
    /// One manager owns the idle timer, its logind inhibitor and DND, for as
    /// long as the shell runs.
    static IDLE: std::cell::RefCell<Option<Idle>> = const { std::cell::RefCell::new(None) };
    /// How a bar writes a shelf state back to the GUI preferences, installed
    /// once by the shell: a chevron that the reader opened must stay open
    /// across the reload its own click might cause.
    static SHELF_SAVE: std::cell::RefCell<Option<ShelfSaver>> =
        const { std::cell::RefCell::new(None) };
}

/// How a bar writes the shelf's state back to the GUI preferences.
type ShelfSaver = Rc<dyn Fn(bool)>;

/// Remember whether the overflow shelf is open.
fn save_shelf_state(expanded: bool) {
    let saver = SHELF_SAVE.with(|slot| slot.borrow().clone());

    if let Some(saver) = saver {
        saver(expanded);
    }
}

struct Bootstrap {
    loader: ConfigLoader,
    loaded: LoadedConfig,
    config_label: String,
    hold: gtk::gio::ApplicationHoldGuard,
}

type ShellRef = Rc<RefCell<Shell>>;
type ShellSlot = Rc<RefCell<Option<ShellRef>>>;

pub fn run(mut loader: ConfigLoader, config_label: String) -> gtk::glib::ExitCode {
    let mut flags = gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE;

    // Integration tests run a second shell beside the real one.  The C shell
    // has the same escape hatch.
    if std::env::var_os("WHELK_TEST_NON_UNIQUE").is_some() {
        flags |= gtk::gio::ApplicationFlags::NON_UNIQUE;
    }

    let application = gtk::Application::builder()
        .application_id("org.jointhefreeworld.whelk")
        .flags(flags)
        .build();

    for option in FORWARDED_OPTIONS {
        let short = option
            .short
            .map(|character| gtk::glib::Char::from(character as u8))
            .unwrap_or(gtk::glib::Char::from(0));
        let argument = if option.value.is_some() {
            gtk::glib::OptionArg::String
        } else {
            gtk::glib::OptionArg::None
        };

        application.add_main_option(
            option.long,
            short,
            gtk::glib::OptionFlags::NONE,
            argument,
            option.description,
            option.value,
        );
    }

    // Install the handlers before registering.  The supervisor forwards a
    // termination signal as soon as the child exists, and a signal that
    // arrives during registration must still take the ordinary shutdown path
    // instead of killing the UI before it can flush the clipboard.
    if let Err(error) = crate::signals::install() {
        eprintln!("whelk: could not install termination handlers: {error}");
    }

    let _termination = crate::signals::quit_source(&application);

    // Register before reading the configuration.  A second invocation only
    // forwards its command line to the first shell, so a broken settings file
    // must not stop `whelk --quit` or `--reload` from reaching the shell
    // that is already running.  The C shell has the same ordering.
    if let Err(error) = application.register(None::<&gtk::gio::Cancellable>) {
        eprintln!("whelk: could not register the application: {error}");

        return gtk::glib::ExitCode::FAILURE;
    }

    let bootstrap = Rc::new(RefCell::new(None));

    if !application.is_remote() {
        let loaded = match loader.load() {
            Ok(loaded) => loaded,
            Err(error) => {
                eprintln!("whelk: {error}");

                return gtk::glib::ExitCode::FAILURE;
            }
        };

        // A configuration can intentionally have no bar on the current outputs
        // (a named external connector, or `#:when 'multi`).  Hold the
        // application so it can wait for the monitor that makes the rule true.
        let hold = application.hold();

        *bootstrap.borrow_mut() = Some(Bootstrap {
            loader,
            loaded,
            config_label,
            hold,
        });
    }

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

    // A second invocation is forwarded here by GApplication.  When this is
    // the first one, `activate` is ours to call: a handled command line does
    // not activate the application by itself.
    let command_line_slot = slot.clone();
    application.connect_command_line(move |application, command_line| {
        if command_line_slot.borrow().is_none() {
            application.activate();
        }

        let requests = requests_from(command_line);

        if let Some(shell) = command_line_slot.borrow().clone() {
            for request in requests.commands() {
                if !perform(application, &shell, request, Some(command_line)) {
                    break;
                }
            }
        }

        gtk::glib::ExitCode::SUCCESS
    });

    add_quit_action(&application);
    add_simple_action(&application, &slot, "reload", Request::Reload);
    add_simple_action(&application, &slot, "settings", Request::Settings);
    add_simple_action(&application, &slot, "notifications", Request::Notifications);
    add_simple_action(&application, &slot, "dnd", Request::Dnd);
    add_simple_action(&application, &slot, "night-light", Request::NightLight);
    add_simple_action(&application, &slot, "inhibit", Request::Inhibit(None));
    add_string_action(&application, &slot, "launcher", |value| {
        Request::Launcher((!value.is_empty()).then_some(value))
    });
    add_string_action(&application, &slot, "menu", Request::Menu);
    add_string_action(&application, &slot, "network", |value| {
        Request::Network(Some(value))
    });

    // The real command line is handed to GApplication, not replaced with a
    // synthetic program name: a second `whelk --launcher` must be parsed
    // and forwarded instead of starting a second shell.
    application.run()
}

/// Read the options GApplication parsed for this invocation.  `--config` is
/// read by `main` before the application exists and intentionally ignored
/// here: a second invocation cannot change the first one's configuration.
fn requests_from(command_line: &gtk::gio::ApplicationCommandLine) -> Requests {
    let options = command_line.options_dict();
    let flag = |name: &str| options.lookup::<bool>(name).ok().flatten().unwrap_or(false);
    let text = |name: &str| options.lookup::<String>(name).ok().flatten();

    Requests {
        launcher: flag("launcher"),
        launcher_mode: text("launcher-mode"),
        clipboard: flag("clipboard"),
        clipboard_status: flag("clipboard-status"),
        clipboard_pause: flag("clipboard-pause"),
        clipboard_clear: flag("clipboard-clear"),
        clipboard_clear_unpinned: flag("clipboard-clear-unpinned"),
        clipboard_restart: flag("clipboard-restart"),
        menu: text("menu"),
        session_menu: flag("session-menu"),
        settings: flag("settings"),
        about: flag("about"),
        wallpaper_picker: flag("wallpaper-picker"),
        notifications: flag("notifications"),
        dnd: flag("dnd"),
        night_light: flag("night-light"),
        preset: text("preset"),
        inhibit: flag("inhibit"),
        inhibit_reason: text("inhibit-reason"),
        idle_status: flag("idle-status"),
        network: text("network"),
        lock: flag("lock"),
        reload: flag("reload"),
        quit: flag("quit"),
    }
}

fn add_quit_action(application: &gtk::Application) {
    let quit = gtk::gio::SimpleAction::new("quit", None);
    let weak = application.downgrade();

    quit.connect_activate(move |_, _| {
        if let Some(application) = weak.upgrade() {
            application.quit();
        }
    });

    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
}

fn add_simple_action(
    application: &gtk::Application,
    slot: &ShellSlot,
    name: &str,
    request: Request,
) {
    let action = gtk::gio::SimpleAction::new(name, None);
    let slot = slot.clone();
    let weak = application.downgrade();

    action.connect_activate(move |_, _| {
        if let (Some(application), Some(shell)) = (weak.upgrade(), slot.borrow().clone()) {
            perform(&application, &shell, request.clone(), None);
        }
    });

    application.add_action(&action);
}

fn add_string_action(
    application: &gtk::Application,
    slot: &ShellSlot,
    name: &str,
    request: fn(String) -> Request,
) {
    let action = gtk::gio::SimpleAction::new(name, Some(gtk::glib::VariantTy::STRING));
    let slot = slot.clone();
    let weak = application.downgrade();

    action.connect_activate(move |_, parameter| {
        let value = parameter
            .and_then(|parameter| parameter.get::<String>())
            .unwrap_or_default();

        if let (Some(application), Some(shell)) = (weak.upgrade(), slot.borrow().clone()) {
            perform(&application, &shell, request(value), None);
        }
    });

    application.add_action(&action);
}

/// Perform one forwarded request.  `false` means the application is quitting
/// and no later request should run.
fn perform(
    application: &gtk::Application,
    shell: &ShellRef,
    request: Request,
    command_line: Option<&gtk::gio::ApplicationCommandLine>,
) -> bool {
    match request {
        Request::Quit => {
            application.quit();

            return false;
        }
        Request::Reload => shell.borrow_mut().reload(),
        Request::Settings => present_preferences(shell),
        Request::About => {
            let style = shell
                .borrow()
                .loaded
                .bars
                .first()
                .cloned()
                .unwrap_or_default();
            let label = shell.borrow().config_label.clone();

            about::show_program(&style, &label);
        }
        Request::Lock => locker::start_lock_screen(),
        Request::Menu(command) => {
            if let Some(launcher) = Launcher::current() {
                launcher.show_menu(&command);
            }
        }
        Request::SessionMenu => {
            if let Some(launcher) = Launcher::current() {
                launcher.show_session_menu();
            }
        }
        Request::Launcher(mode) => {
            Launcher::show_current(mode.as_deref().filter(|mode| !mode.is_empty()));
        }
        Request::Clipboard => Launcher::show_current(Some("clipboard")),
        Request::ClipboardStatus => {
            if let Some(clipboard) = Clipboard::current() {
                say(command_line, &format!("{}\n", clipboard.status_line()));
            }
        }
        Request::ClipboardPause => {
            if let Some(clipboard) = Clipboard::current() {
                clipboard.set_paused(!clipboard.is_paused());
            }
        }
        Request::ClipboardClear => {
            if let Some(clipboard) = Clipboard::current() {
                clipboard.clear();
            }
        }
        Request::ClipboardClearUnpinned => {
            if let Some(clipboard) = Clipboard::current() {
                clipboard.clear_unpinned();
            }
        }
        Request::ClipboardRestart => {
            if let Some(clipboard) = Clipboard::current() {
                clipboard.restart();
            }
        }
        Request::WallpaperPicker => shell.borrow().show_picker(),
        Request::Notifications => NotificationCenter::toggle_current(),
        Request::Dnd => toggle_dnd(),
        Request::NightLight => {
            let enabled = shell.borrow_mut().toggle_night_light();

            match enabled {
                Some(enabled) => say(
                    command_line,
                    &format!("night light: {}\n", if enabled { "on" } else { "off" }),
                ),
                None => say(
                    command_line,
                    "night light: unchanged (the override could not be saved)\n",
                ),
            }
        }
        Request::Preset(name) => {
            // "forget" is the one word that means "no preset": it clears the
            // remembered choice and lets the settings file decide again, which
            // is the only way back out of a bundle without editing a file.
            let forget = name
                .as_deref()
                .is_none_or(|name| name.eq_ignore_ascii_case("forget"));

            if shell
                .borrow_mut()
                .switch_preset(if forget { None } else { name.as_deref() })
            {
                say(
                    command_line,
                    &match (forget, name.as_deref()) {
                        (true, _) => "preset: following the settings file again\n".to_owned(),
                        (false, Some(name)) => format!("preset: {name}\n"),
                        (false, None) => "preset: unchanged\n".to_owned(),
                    },
                );
            } else {
                say(
                    command_line,
                    "preset: unchanged (unknown preset, or the choice could not be saved)\n",
                );
            }
        }
        Request::Inhibit(reason) => toggle_inhibit(reason.as_deref()),
        Request::IdleStatus => say(command_line, &idle_status()),
        Request::Network(page) => {
            NetworkPanel::show_current(page.as_deref().unwrap_or(""));
        }
    }

    true
}

/// Write a reply for the invoking process.  GApplication sends it back over
/// the command-line connection, so `whelk --clipboard-status` in a script
/// reads its answer from stdout even though the shell owns the service.
fn say(command_line: Option<&gtk::gio::ApplicationCommandLine>, text: &str) {
    if let Some(command_line) = command_line {
        command_line.print_literal(text);
    }
}

struct Shell {
    application: gtk::glib::WeakRef<gtk::Application>,
    loader: ConfigLoader,
    loaded: LoadedConfig,
    config_label: String,
    session: Session,
    bars: Vec<BarWindow>,
    widgets: Widgets,
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
    picker: Option<Picker>,
    wallpaper_menu: Option<WallpaperMenu>,
    desktop: Vec<gtk::Window>,
    preferences: Option<Preferences>,
    _hold: gtk::gio::ApplicationHoldGuard,
}

impl Shell {
    #[allow(clippy::too_many_arguments)]
    fn present(
        application: &gtk::Application,
        loader: ConfigLoader,
        loaded: LoadedConfig,
        config_label: String,
        hold: gtk::gio::ApplicationHoldGuard,
    ) -> ShellRef {
        // Every surface below paints its skin with the finish in force, so it
        // is installed before the first pane exists.
        frost::install(loaded.frost.clone());

        // The C shell prepares the locker at startup, before a `--lock` or a
        // logind signal can arrive; the Rust shell watches logind at the same
        // point and starts `whelk --lock` when a trigger fires.
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
            widgets: Widgets::new(),
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
            picker: None,
            wallpaper_menu: None,
            desktop: Vec::new(),
            preferences: None,
            _hold: hold,
        }));

        {
            let mut state = shell.borrow_mut();

            state.rebuild();
        }

        // The night light opens its own display connection and follows the
        // configuration from here; a session whose server has no gamma table
        // logs why and keeps the setting for one that does.
        {
            let config = shell.borrow().loaded.night_light.clone();

            crate::nightlight::start(config);
        }

        // The picker is created hidden: a `--wallpaper-picker` run, a menu row
        // or the shell's own action presents it, and a chosen picture is
        // saved as the GUI preference the loader already knows how to read.
        {
            let weak = Rc::downgrade(&shell);
            let on_choose: Rc<dyn Fn(&Path)> = Rc::new(move |path| {
                if let Some(shell) = weak.upgrade() {
                    shell.borrow_mut().choose_wallpaper(path);
                }
            });
            let (style, wallpaper) = {
                let state = shell.borrow();

                (
                    state.loaded.bars.first().cloned().unwrap_or_default(),
                    state.loaded.wallpaper.clone(),
                )
            };
            let picker = Picker::new(application, &style, &wallpaper, Rc::clone(&on_choose));
            let browse_weak = Rc::downgrade(&shell);
            let on_browse: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(shell) = browse_weak.upgrade() {
                    shell.borrow().show_picker();
                }
            });
            let menu = WallpaperMenu::new(application, &style, &wallpaper, on_choose, on_browse);

            crate::wallpapermenu::install(Some(menu.clone()));

            let mut state = shell.borrow_mut();

            state.picker = Some(picker);
            state.wallpaper_menu = Some(menu);
        }

        {
            let weak = Rc::downgrade(&shell);

            SHELF_SAVE.with(|slot| {
                let saver: ShelfSaver = Rc::new(move |expanded: bool| {
                    let Some(shell) = weak.upgrade() else {
                        return;
                    };
                    let mut state = shell.borrow_mut();

                    if let Err(error) = state
                        .loader
                        .save_preference("bar-overflow-expanded?", &PreferenceValue::Bool(expanded))
                    {
                        eprintln!("whelk: could not remember the shelf: {error}");
                    }
                });

                *slot.borrow_mut() = Some(saver);
            });
        }

        watch_config(&shell);
        watch_monitors(&shell);

        let wallpaper = shell.borrow().loaded.wallpaper.clone();

        crate::wallpaper_surface::present(application, &wallpaper);

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
                &self.loaded.menu,
                &self.loaded.menu_sources,
                self.session,
            ));
            built += 1;
        }

        if built == 0 {
            eprintln!("whelk: no bar matches the current monitor model; waiting for an output");
        }

        // The background's input half: one transparent surface per output, so a
        // right click on an empty desktop can offer the wallpapers.
        self.desktop = crate::desktop::present(&application, &monitors);

        // The cards are placed around the bars, so they are rebuilt with them:
        // a settings save and a hotplug both land here, and the style they
        // read is the first bar's, exactly as the C shell passes it.
        let style = self.loaded.bars.first().cloned().unwrap_or_default();

        self.widgets.build(
            &application,
            &self.loaded.widgets,
            &style,
            self.loader.path(),
        );
    }

    /// The values the settings window draws: the first bar's material and
    /// modules, the night light Scheme reads, and the themes it knows.
    fn preferences_snapshot(&mut self) -> Snapshot {
        Snapshot {
            bar: self.loaded.bars.first().cloned().unwrap_or_default(),
            night_light: self.loader.night_light().unwrap_or_default(),
            themes: self.loader.theme_names(),
        }
    }

    fn reload(&mut self) {
        match self.loader.load() {
            Ok(loaded) => {
                // A bar-only save (the settings window writes one on every
                // control) must not restart the background; a wallpaper change
                // in the file is a repaint, as the C engine does on reload.
                let wallpaper_changed = loaded.wallpaper != self.loaded.wallpaper;

                self.loaded = loaded;
                self.config_label = self.loader.label();
                // A new finish repaints every pane below; a reload that did not
                // touch it installs the same value and costs one clone.
                frost::install(self.loaded.frost.clone());
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

                if let Some(picker) = self.picker.as_ref() {
                    picker.set_style(&style);
                }

                if let Some(menu) = self.wallpaper_menu.as_ref() {
                    menu.set_style(&style);
                    menu.set_settings(&self.loaded.wallpaper);
                }

                self.rebuild();

                crate::nightlight::update(self.loaded.night_light.clone());

                if wallpaper_changed && let Some(application) = self.application.upgrade() {
                    crate::wallpaper_surface::present(&application, &self.loaded.wallpaper);
                }
            }
            Err(error) => {
                eprintln!("whelk: configuration reload failed: {error}; keeping the previous bars");
            }
        }
    }

    /// Present the wallpaper picker from a forwarded request.
    fn show_picker(&self) {
        if let Some(picker) = self.picker.as_ref() {
            picker.show(&self.loaded.wallpaper);
        }
    }

    /// Flip the night-light GUI preference and reload the shell, as the C
    /// `--night-light` handler does.  `None` means the override was not saved.
    fn toggle_night_light(&mut self) -> Option<bool> {
        let enabled = !self.loaded.night_light.enabled;
        let value = PreferenceValue::Bool(enabled);

        if let Err(error) = self.loader.save_preference("night-light-enabled?", &value) {
            eprintln!("whelk: could not save the night-light switch: {error}");

            return None;
        }

        self.reload();

        Some(enabled)
    }

    /// Switch the shell to a preset by name, or forget the remembered choice
    /// when NAME is `None`.
    ///
    /// The preset is applied by the loader, so the switch is a reload: the bars,
    /// the widgets and the menu it brings arrive through the same path a
    /// settings edit takes, and nothing has to know two ways to build a shell.
    fn switch_preset(&mut self, name: Option<&str>) -> bool {
        match self.loader.choose_preset(name) {
            Ok(true) => {
                self.reload();

                true
            }
            Ok(false) => false,
            Err(error) => {
                eprintln!("whelk: could not switch preset: {error}");

                false
            }
        }
    }

    /// Save the picture chosen in the picker as the configuration's GUI
    /// preference and repaint the background now, so the choice is both what
    /// this session shows and what the next one starts on.
    fn choose_wallpaper(&mut self, path: &Path) {
        let mut settings = self.loaded.wallpaper.clone();

        settings.current = Some(path.to_path_buf());

        if let Err(error) = self.loader.remember_wallpaper(path) {
            eprintln!("whelk: could not save the wallpaper choice: {error}");
        }

        if let Some(application) = self.application.upgrade() {
            crate::wallpaper_surface::present(&application, &settings);
        }

        self.loaded.wallpaper = settings;
    }

    fn teardown(&mut self) {
        crate::nightlight::stop();

        Osd::install(None);
        Launcher::install(None);
        crate::wallpapermenu::install(None);
        NetworkPanel::install(None);
        NotificationCenter::install(None);
        Clipboard::install(None);
        NotificationService::install(None);

        SHELF_SAVE.with(|slot| *slot.borrow_mut() = None);

        self.preferences.take();
        self.picker.take();
        self.wallpaper_menu.take();

        for window in self.desktop.drain(..) {
            window.set_visible(false);
            window.destroy();
        }
        self.launcher.take();
        self.network.take();
        self.center.take();
        self.osd.take();

        if let Some(service) = self.notifications.take() {
            service.shutdown();
        }

        if let Some(source) = self.reload_source.take() {
            sources::remove(source);
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
        self.widgets.clear();

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
            sources::remove(source);
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

#[allow(clippy::too_many_arguments)]
fn build_bar_window(
    application: &gtk::Application,
    bar_settings: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
    config_label: &str,
    shell_menu: &[MenuEntry],
    menu_sources: &Rc<MenuSources>,
    session: Session,
) -> BarWindow {
    let widget_id = style::widget_id(&bar_settings.name);
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk")
        .build();

    window.set_widget_name(&widget_id);
    window.add_css_class("whelk-surface");
    window.set_decorated(false);
    // A layer surface is sized by the compositor -- an exclusive zone, a scale
    // change or a margin all shrink or grow it -- and a non-resizable window
    // ignores those configure events, laying out at its first size instead.
    window.set_resizable(true);

    let bar = gtk::CenterBox::new();
    bar.add_css_class("whelk-bar");
    bar.set_size_request(-1, bar_settings.height);
    // The name label that used to open the left row is gone; the settings file
    // it named is still a hover away, on the bar's own gaps.
    bar.set_tooltip_text(Some(config_label));

    let start = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));
    let center = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));
    let end = gtk::Box::new(gtk::Orientation::Horizontal, bar_settings.spacing.max(0));

    start.add_css_class("whelk-start");
    center.add_css_class("whelk-center");
    end.add_css_class("whelk-end");

    let mut context = ModuleContext::from_bar(bar_settings, session);

    context.shell_menu = Rc::new(shell_menu.to_vec());
    context.menu_sources = Rc::clone(menu_sources);

    append_modules(&start, &bar_settings.left, &context);
    append_modules(&center, &bar_settings.center, &context);

    let mut readings = append_modules(&end, &bar_settings.right, &context);

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

        clock.add_css_class("whelk-clock");
        update_clock(&clock, &clock_format);
        end.append(&clock);
        // The only clock a configuration without a clock module gets: it must
        // not be the reading that folds.
        readings.push((
            clock.clone().upcast::<gtk::Widget>(),
            crate::bar::KEEP_PRIORITY,
        ));

        Some(ClockTimer { source, alive })
    } else {
        None
    };

    let overflow = append_overflow(&end, bar_settings, &context);
    let more = build_folded_shelf(&end);

    bar.set_start_widget(Some(&start));
    bar.set_center_widget(Some(&center));
    bar.set_end_widget(Some(&end));
    window.set_child(Some(&bar));
    build_bar_context_menu(application, &bar, &start, &center, &end);

    let css = install_css(bar_settings);

    if layer_shell_available() {
        configure_layer_shell(&window, bar_settings, monitor);
    } else {
        // Broadway and a plain X11 test display have no layer shell.  A normal
        // window is still a useful smoke test for the widget tree.
        window.set_default_size(720, bar_settings.height.max(32));
    }

    fold_after_map(
        FoldState {
            bar: bar.clone(),
            start: start.clone(),
            center: center.clone(),
            end: end.clone(),
            overflow,
            more,
            readings,
            spacing: bar_settings.spacing,
            padding: bar_settings.padding,
        },
        &window,
    );
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

/// Create the settings window on first use and bring it forward after that.
///
/// Building it at startup would read every launcher prefix and theme name
/// through Guile for a window most runs never open.  The C shell builds its
/// singleton on the first `whelk_preferences_show` too; a later open still
/// redraws from a fresh snapshot, so no field is stale.
fn present_preferences(shell: &ShellRef) {
    let Some(application) = shell.borrow().application.upgrade() else {
        return;
    };

    if shell.borrow().preferences.is_some() {
        let snapshot = shell.borrow_mut().preferences_snapshot();

        if let Some(preferences) = shell.borrow().preferences.as_ref() {
            preferences.show(snapshot);
        }

        return;
    }

    let save_weak = Rc::downgrade(shell);
    let save: SaveFn = Rc::new(move |key, value| {
        let Some(shell) = save_weak.upgrade() else {
            return false;
        };
        let mut state = shell.borrow_mut();

        match state.loader.save_preference(key, value) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("whelk: could not save '{key}': {error}");

                false
            }
        }
    });
    let reset_weak = Rc::downgrade(shell);
    let reset: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let Some(shell) = reset_weak.upgrade() else {
            return false;
        };
        let mut state = shell.borrow_mut();

        match state.loader.reset_preferences() {
            Ok(answer) => answer,
            Err(error) => {
                eprintln!("whelk: could not reset the GUI preferences: {error}");

                false
            }
        }
    });
    let read_weak = Rc::downgrade(shell);
    let read: ReadFn = Rc::new(move |key| {
        let shell = read_weak.upgrade()?;
        let mut state = shell.borrow_mut();

        state.loader.preference_text(key)
    });
    let reload_weak = Rc::downgrade(shell);
    let reload: ReloadFn = Rc::new(move || {
        let Some(shell) = reload_weak.upgrade() else {
            return Snapshot::fallback();
        };
        let mut state = shell.borrow_mut();

        state.reload();
        state.preferences_snapshot()
    });
    let callbacks = Rc::new(preferences::Callbacks {
        save,
        reset,
        read,
        reload,
    });
    let (config_path, snapshot) = {
        let mut state = shell.borrow_mut();

        (state.loader.path().to_owned(), state.preferences_snapshot())
    };
    let preferences = Preferences::new(&application, config_path, snapshot, callbacks);

    shell.borrow_mut().preferences = Some(preferences);

    // `Preferences::new` already drew this snapshot; only present it.
    if let Some(preferences) = shell.borrow().preferences.as_ref() {
        preferences.present();
    }
}

fn watch_config(shell: &ShellRef) {
    let path = shell.borrow().loader.path().to_owned();
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if let Err(error) = std::fs::create_dir_all(directory) {
        eprintln!(
            "whelk: cannot create the configuration directory '{}': {error}",
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
            eprintln!("whelk: cannot watch the configuration directory: {error}");

            return;
        }
    };
    let weak = Rc::downgrade(shell);
    let watched = path.clone();
    let handler = monitor.connect_changed(move |_, file, _, _| {
        // The watch is on the directory, so an unrelated write -- a rotated log,
        // an editor's lock file -- would otherwise re-evaluate the settings and
        // rebuild every bar.  A `.scm` in this directory is configuration, and a
        // render that renames a temporary file over the settings file is seen as
        // the settings file itself.
        if let Some(changed) = file.path()
            && changed != watched
            && !changed
                .extension()
                .is_some_and(|extension| extension == "scm")
        {
            return;
        }

        if let Some(shell) = weak.upgrade() {
            schedule_reload(&shell);
        }
    });

    shell.borrow_mut().config_watch = Some((monitor, handler));
}

fn schedule_reload(shell: &ShellRef) {
    let mut state = shell.borrow_mut();

    if let Some(source) = state.reload_source.take() {
        sources::remove(source);
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

/// The idle manager's DND flag, flipped from the command line or the
/// `dnd` application action.
fn toggle_dnd() {
    if let Some(idle) = idle() {
        idle.set_dnd(!idle.has_dnd());
    }
}

/// Take or release the logind presentation inhibitor, with the C default
/// reason when the command line did not supply one.
fn toggle_inhibit(reason: Option<&str>) {
    let Some(idle) = idle() else {
        return;
    };
    let reason = reason
        .filter(|reason| !reason.is_empty())
        .unwrap_or("presentation mode");

    if !idle.set_inhibited(!idle.is_inhibited(), reason) {
        eprintln!("whelk: could not change the presentation inhibitor");
    }
}

/// The idle manager's own account of its timer, inhibitor and DND state.
fn idle_status() -> String {
    idle()
        .map(|idle| idle.describe())
        .unwrap_or_else(|| "idle manager unavailable\n".to_owned())
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

    // A GTK window does not take its size from the compositor's configure: a
    // layer client asks for one.  Without this, an anchored surface is only as
    // large as its content -- a bar as wide as its modules -- while the
    // compositor happily anchors that small window.
    apply_surface_size(window, bar, monitor);
}

/// Ask for the size the anchors imply: a horizontal bar spans its output's
/// width, a vertical one its height, and the other axis keeps its natural size.
fn apply_surface_size(
    window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
) {
    let fallback = gtk::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gtk::gdk::Monitor>();
    let Some(monitor) = monitor.or(fallback.as_ref()) else {
        return;
    };
    let geometry = monitor.geometry();

    let (width, height) = match bar.edge {
        BarEdge::Top | BarEdge::Bottom => {
            (geometry.width() - bar.margin_left - bar.margin_right, -1)
        }
        BarEdge::Left | BarEdge::Right => {
            (-1, geometry.height() - bar.margin_top - bar.margin_bottom)
        }
    };

    window.set_default_size(width.max(1), height);
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

/// Build one row, and return what it holds: the widgets in row order with the
/// priority each one declared, which is what the fold reads.
fn append_modules(
    section: &gtk::Box,
    modules: &[ModuleSpec],
    context: &ModuleContext,
) -> Vec<(gtk::Widget, i32)> {
    let mut readings = Vec::new();

    for module in modules {
        if let Some(widget) = barfactory::build(module, context) {
            section.append(&widget);
            readings.push((
                widget.upcast::<gtk::Widget>(),
                crate::bar::module_priority(module),
            ));
        }
    }

    readings
}

/// The configured overflow row: built with the bar so its modules have already
/// started, but hidden behind the chevron until it is opened.  This is the
/// Rust equivalent of C's inline `#:overflow` area; the width-driven folded
/// row lives in `build_folded_shelf()` beside it.
fn append_overflow(
    end: &gtk::Box,
    bar_settings: &BarSettings,
    context: &ModuleContext,
) -> Option<OverflowRow> {
    if bar_settings.overflow.is_empty() {
        return None;
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
        save_shelf_state(expanded);
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

    Some(OverflowRow {
        shelf: overflow,
        toggle,
    })
}

struct OverflowRow {
    shelf: gtk::Box,
    toggle: gtk::ToggleButton,
}

struct FoldedShelf {
    button: gtk::MenuButton,
    count: gtk::Label,
    shelf: gtk::Box,
}

/// The `+N` shelf at the end of the right row.  Its popover is built now and
/// filled only when the first allocation proves that readings do not fit.
fn build_folded_shelf(end: &gtk::Box) -> FoldedShelf {
    let shelf = gtk::Box::new(gtk::Orientation::Vertical, 4);

    shelf.add_css_class("whelk-overflow-box");

    let popover = gtk::Popover::new();

    popover.set_child(Some(&shelf));

    let count = gtk::Label::new(Some("+1"));

    count.add_css_class("whelk-folded-count");

    let button = gtk::MenuButton::new();

    button.set_child(Some(&count));
    button.add_css_class("flat");
    button.add_css_class("whelk-overflow-more");
    button.set_tooltip_text(Some("Readings that did not fit"));
    button.set_popover(Some(&popover));
    button.set_visible(false);
    crate::barmodule::attach_exclusive_popover(&button, &popover);
    end.append(&button);

    FoldedShelf {
        button,
        count,
        shelf,
    }
}

struct FoldState {
    bar: gtk::CenterBox,
    start: gtk::Box,
    center: gtk::Box,
    end: gtk::Box,
    overflow: Option<OverflowRow>,
    more: FoldedShelf,
    /// The right row as it was built: in row order, with each reading's claim.
    readings: Vec<(gtk::Widget, i32)>,
    spacing: i32,
    padding: i32,
}

/// Fold the right row, and fold it again whenever the bar is handed a new size.
///
/// The C bar folded once, from an idle after the surface was presented, which
/// left a row folded to a width it no longer had: a scale change, a rotation or
/// a font that grows all re-lay the bar out, and the fold has to re-answer.  A
/// headless build has no surface and keeps the row whole, exactly as the
/// `root_width <= 1` guard does.
fn fold_after_map(state: FoldState, window: &gtk::ApplicationWindow) {
    let state = Rc::new(state);
    let armed = Rc::new(Cell::new(false));

    let map_state = Rc::clone(&state);

    window.connect_map(move |_| {
        if armed.replace(true) {
            return;
        }

        let state = Rc::clone(&map_state);

        // A short timeout rather than an idle: the row's real allocation is
        // what the first fold needs, and map alone does not have it yet.
        gtk::glib::timeout_add_local_once(Duration::from_millis(100), move || {
            collapse_end(&state);
        });
    });

    // `GdkSurface:width` is the allocation the compositor gave the surface,
    // which is what a fold has to fit into.
    if let Some(surface) = window.surface() {
        let state = Rc::clone(&state);

        surface.connect_width_notify(move |_| {
            collapse_end(&state);
        });
    }
}

fn widget_width(widget: &gtk::Widget) -> i32 {
    if !widget.is_visible() {
        return 0;
    }

    let width = widget.width();

    if width > 0 {
        return width;
    }

    let (_, natural, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);

    natural.max(0)
}

fn reading_width(widget: &gtk::Widget) -> i32 {
    let width = widget_width(widget);

    if width > 0 { width } else { 24 }
}

fn collapse_end(state: &FoldState) {
    let root_width = state.bar.width();

    // No allocation yet: leave the row alone rather than hiding everything.
    if root_width <= 1 || state.readings.is_empty() {
        return;
    }

    let mut used_by_others = widget_width(state.start.upcast_ref::<gtk::Widget>())
        + widget_width(state.center.upcast_ref::<gtk::Widget>())
        + 2 * state.padding.max(0);

    if let Some(overflow) = &state.overflow {
        used_by_others += widget_width(overflow.shelf.upcast_ref::<gtk::Widget>())
            + widget_width(overflow.toggle.upcast_ref::<gtk::Widget>());
    }

    let available = (root_width - used_by_others - 16).max(0);
    let readings: Vec<crate::bar::Reading> = state
        .readings
        .iter()
        .map(|(widget, priority)| crate::bar::Reading {
            width: reading_width(widget),
            priority: *priority,
        })
        .collect();
    let kept = crate::bar::fold_visible(&readings, available, state.spacing);

    // Re-answer from the whole row every time: a widget the last fold moved to
    // the shelf has to be a candidate again once the bar is wider.
    while let Some(child) = state.more.shelf.first_child() {
        state.more.shelf.remove(&child);
    }

    let row = state.end.clone().upcast::<gtk::Widget>();

    for (widget, _) in &state.readings {
        if widget.parent().as_ref() == Some(&row) {
            state.end.remove(widget);
        }
    }

    // Prepending in reverse puts the kept readings back at the head of the row,
    // in their configured order and ahead of the overflow and `+N` controls.
    for index in kept.iter().rev() {
        if let Some((widget, _)) = state.readings.get(*index) {
            state.end.prepend(widget);
        }
    }

    for (index, (widget, _)) in state.readings.iter().enumerate() {
        if !kept.contains(&index) {
            state.more.shelf.append(widget);
        }
    }

    let folded = state.readings.len() - kept.len();

    if folded > 0 {
        state.more.count.set_text(&format!("+{folded}"));
        state.more.button.set_tooltip_text(Some(&format!(
            "{folded} reading{} did not fit · click to see {}",
            if folded == 1 { "" } else { "s" },
            if folded == 1 { "it" } else { "them" }
        )));
    }

    state.more.button.set_visible(folded > 0);
}

/// The C bar's own right-click menu: a way into the settings window and a
/// reload without remembering a socket command.  Modules keep their own right
/// clicks; only the bar's gaps, padding and three groups open this menu.
fn build_bar_context_menu(
    application: &gtk::Application,
    bar: &gtk::CenterBox,
    start: &gtk::Box,
    center: &gtk::Box,
    end: &gtk::Box,
) {
    let popover = gtk::Popover::new();

    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_parent(bar);
    crate::barmodule::watch_popover(&popover);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-bar-menu");

    let heading = gtk::Label::new(Some("whelk"));

    heading.add_css_class("whelk-bar-menu-heading");
    heading.set_xalign(0.0);
    heading.set_margin_start(8);
    heading.set_margin_end(8);
    heading.set_margin_top(4);
    heading.set_margin_bottom(4);

    let edit = gtk::Button::with_label("Edit settings…");

    edit.add_css_class("flat");
    edit.set_halign(gtk::Align::Start);

    let reload = gtk::Button::with_label("Reload settings");

    reload.add_css_class("flat");
    reload.set_halign(gtk::Align::Start);

    menu.append(&heading);
    menu.append(&edit);
    menu.append(&reload);
    popover.set_child(Some(&menu));

    let application_for_edit = application.clone();
    let popover_for_edit = popover.clone();

    edit.connect_clicked(move |_| {
        application_for_edit.activate_action("settings", None);
        popover_for_edit.popdown();
    });

    let application_for_reload = application.clone();
    let popover_for_reload = popover.clone();

    reload.connect_clicked(move |_| {
        application_for_reload.activate_action("reload", None);
        popover_for_reload.popdown();
    });

    let gesture = gtk::GestureClick::new();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);

    let bar_for_pick = bar.clone();
    let start = start.clone().upcast::<gtk::Widget>();
    let center = center.clone().upcast::<gtk::Widget>();
    let end = end.clone().upcast::<gtk::Widget>();

    gesture.connect_pressed(move |_, _, x, y| {
        let root = bar_for_pick.clone().upcast::<gtk::Widget>();
        let target = bar_for_pick.pick(x, y, gtk::PickFlags::DEFAULT);
        let structural = target.as_ref().is_some_and(|target| {
            target == &root || target == &start || target == &center || target == &end
        });

        if !structural {
            return;
        }

        // The bar's own menu is one more grabbing popup: the module menus and
        // the start menu go first, or GDK refuses to map this one.
        crate::barmodule::close_popovers(Some(&popover));
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    bar.add_controller(gesture);
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
        .whelk-bar {
            padding: 0;
        }
        .whelk-clock {
            font-weight: 700;
        }
        .whelk-overflow-more {
            padding: 0 4px;
        }
        .whelk-bar-menu {
            padding: 4px;
        }
        .whelk-bar-menu-heading {
            font-weight: 700;
            opacity: 0.7;
        }
        .whelk-overflow-box {
            padding: 6px;
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

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

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::gio::prelude::ApplicationCommandLineExt;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::about;
use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::{self, ModuleContext};
use crate::barmonitors::{BarPlacement, DEFAULT_MONITOR, plan_bars};
use crate::cli::{FORWARDED_OPTIONS, Request, Requests};
use crate::clipboard::Clipboard;
use crate::command;
use crate::config::{ConfigLoader, LoadedConfig};
use crate::desktopmenu::{self, DesktopCommand, DesktopMenu, DesktopMenuState};
use crate::frost;
use crate::idle::Idle;
use crate::launcherhost::{WhelkLauncherHost, launcher_style};
use crate::launcherpanel::Launcher;
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
use bibelot::Widgets;

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
    add_simple_action(&application, &slot, "reveal-bar", Request::Reveal);
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
        reveal: flag("reveal-bar"),
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
        Request::Lock => crate::hasp::lock(),
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
        Request::Reveal => crate::autohide::reveal_all(),
        Request::Inhibit(reason) => toggle_inhibit(reason.as_deref()),
        Request::IdleStatus => say(command_line, &idle_status()),
        Request::Network(page) => {
            NetworkPanel::show_current(page.as_deref().unwrap_or(""));
        }
    }

    true
}

/// Perform one row of the desktop context menu.  Every arm borrows the shell
/// for no longer than the call it makes, so a command that reloads (and
/// therefore rebuilds) can never run under an outstanding borrow.
fn perform_desktop_command(
    shell: &ShellRef,
    application: &gtk::Application,
    command: DesktopCommand,
) {
    match command {
        DesktopCommand::Launcher(mode) => Launcher::show_current(Some(mode)),
        DesktopCommand::Clipboard => Launcher::show_current(Some("clipboard")),
        DesktopCommand::Notifications => NotificationCenter::toggle_current(),
        DesktopCommand::Settings => present_preferences(shell),
        DesktopCommand::About => {
            let (style, label) = {
                let state = shell.borrow();

                (
                    state.loaded.bars.first().cloned().unwrap_or_default(),
                    state.config_label.clone(),
                )
            };

            about::show_program(&style, &label);
        }
        DesktopCommand::SessionMenu => {
            if let Some(launcher) = Launcher::current() {
                launcher.show_session_menu();
            }
        }
        DesktopCommand::Lock => crate::hasp::lock(),
        DesktopCommand::Power(action) => command::spawn(action.command()),
        DesktopCommand::NightLight => {
            shell.borrow_mut().toggle_night_light();
        }
        DesktopCommand::Dnd => toggle_dnd(),
        DesktopCommand::Inhibit => toggle_inhibit(None),
        DesktopCommand::Reload => shell.borrow_mut().reload(),
        DesktopCommand::Quit => application.quit(),
        DesktopCommand::WallpaperNext => {
            shell.borrow_mut().step_wallpaper(1, false);
        }
        DesktopCommand::WallpaperPrevious => {
            shell.borrow_mut().step_wallpaper(-1, false);
        }
        DesktopCommand::WallpaperShuffle => {
            shell.borrow_mut().step_wallpaper(1, true);
        }
        DesktopCommand::WallpaperPicker => shell.borrow().show_picker(),
        // The menu opens the thumbnail collection itself, at the place it was
        // opened, and never sends this row to the shell.
        DesktopCommand::WallpaperCollection => {}
        DesktopCommand::ToggleWidgets => {
            shell.borrow_mut().toggle_widgets();
        }
    }
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
    bars: Vec<whelk_bar::surface::BarWindow>,
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
    desktop_menu: Option<DesktopMenu>,
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
        crate::hasp::watch_logind();
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
        let launcher_skin = launcher_style(&osd_style, loaded.launcher);
        let launcher_host = Rc::new(WhelkLauncherHost::new(Some(clipboard.clone())));
        let launcher = Launcher::new(application, &launcher_skin, launcher_host);
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
            desktop_menu: None,
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

        // The desktop's own menu: the same surface a right click on the empty
        // desktop opens.  Its state is read when it opens, so a switch flipped
        // from a bar or the command line is never a stale row.
        {
            let state_shell = Rc::downgrade(&shell);
            let state: Rc<dyn Fn() -> DesktopMenuState> = Rc::new(move || {
                let Some(shell) = state_shell.upgrade() else {
                    return DesktopMenuState::default();
                };
                let shell = shell.borrow();
                let idle = idle();

                DesktopMenuState {
                    night_light: shell.loaded.night_light.enabled,
                    dnd: idle.as_ref().map(|idle| idle.has_dnd()).unwrap_or(false),
                    inhibited: idle
                        .as_ref()
                        .map(|idle| idle.is_inhibited())
                        .unwrap_or(false),
                    widgets_visible: shell.widgets.visible(),
                    widget_count: shell.widgets.count(),
                    wallpaper: crate::wallpaper_surface::selected()
                        .or_else(|| shell.loaded.wallpaper.current.clone())
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned()),
                }
            });
            let command_shell = Rc::downgrade(&shell);
            let command_application = application.clone();
            let on_command: Rc<dyn Fn(DesktopCommand)> = Rc::new(move |command| {
                if let Some(shell) = command_shell.upgrade() {
                    perform_desktop_command(&shell, &command_application, command);
                }
            });
            let style = shell
                .borrow()
                .loaded
                .bars
                .first()
                .cloned()
                .unwrap_or_default();
            let menu = DesktopMenu::new(application, &style, state, on_command);

            desktopmenu::install(Some(menu.clone()));
            shell.borrow_mut().desktop_menu = Some(menu);
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
        let widget_ids = style::unique_widget_ids(&self.loaded.bars);
        let mut built = 0;

        for placement in placements {
            let Some(bar_settings) = self.loaded.bars.get(placement.bar_index) else {
                continue;
            };
            let monitor = placement_monitor(&placement, &monitors);
            let widget_id = widget_ids
                .get(placement.bar_index)
                .map(String::as_str)
                .unwrap_or("whelk-main");

            let host = Rc::new(WhelkBarHost {
                bar: bar_settings.clone(),
                session: self.session,
                shell_menu: Rc::new(self.loaded.menu.clone()),
                menu_sources: Rc::clone(&self.loaded.menu_sources),
            });

            self.bars.push(whelk_bar::surface::build(
                &application,
                bar_settings,
                monitor,
                widget_id,
                &self.config_label,
                host,
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
            &card_skin(&style),
            &self.loaded.widget_theme,
            self.loader.path(),
        );
    }

    /// The values the settings window draws: the first bar's material and
    /// modules, the night light Scheme reads, and the themes it knows.
    fn preferences_snapshot(&mut self) -> Snapshot {
        Snapshot {
            bar: self.loaded.bars.first().cloned().unwrap_or_default(),
            launcher: self.loaded.launcher,
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
                    launcher.set_style(&launcher_style(&style, self.loaded.launcher));
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

                if let Some(menu) = self.desktop_menu.as_ref() {
                    menu.set_style(&style);
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

    /// Step the wallpaper collection by one picture without a picker.
    ///
    /// The collection is re-read from the configuration, so a file added since
    /// the shell started is already in the walk.  A shuffle picks a picture
    /// that is not the one on screen; a signed step wraps at both ends.
    fn step_wallpaper(&mut self, by: i32, shuffle: bool) -> bool {
        let settings = self.loaded.wallpaper.clone();
        let items = crate::collection::collect(
            &settings.paths,
            settings.current.as_deref(),
            settings.video,
        );
        // The live wallpaper engine may have rotated since the settings were
        // read; the menu must step from the picture actually on screen.
        let current = crate::wallpaper_surface::selected()
            .or_else(|| settings.current.clone())
            .or_else(|| items.first().map(|item| item.path.clone()));
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let next = crate::rotation::step_path(&items, current.as_deref(), by, shuffle, seed);

        match next {
            Some(path) => {
                self.choose_wallpaper(&path);

                true
            }
            None => false,
        }
    }

    /// Show or hide every desktop card without rebuilding the set.
    fn toggle_widgets(&mut self) {
        let visible = !self.widgets.visible();

        self.widgets.set_visible(visible);
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

        desktopmenu::install(None);
        self.desktop_menu.take();

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

struct WhelkBarHost {
    bar: BarSettings,
    session: Session,
    shell_menu: Rc<Vec<MenuEntry>>,
    menu_sources: Rc<MenuSources>,
}

impl whelk_bar::surface::BarHost for WhelkBarHost {
    fn module(&self, spec: &ModuleSpec) -> Option<gtk::Box> {
        let mut context = ModuleContext::from_bar(&self.bar, self.session);

        context.shell_menu = Rc::clone(&self.shell_menu);
        context.menu_sources = Rc::clone(&self.menu_sources);
        barfactory::build(spec, &context)
    }

    fn css(&self, bar: &BarSettings, widget_id: &str) -> String {
        style::css(widget_id, bar, "whelk-bar", bar.css.as_deref())
    }

    fn save_overflow(&self, expanded: bool) {
        save_shelf_state(expanded);
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

        idle.set_lock_action(crate::hasp::lock);
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

pub(crate) fn layer_shell_available() -> bool {
    whelk_bar::surface::layer_shell_available()
}

fn card_skin(style: &BarSettings) -> bibelot::Skin {
    let frost = crate::frost::current();

    bibelot::Skin {
        foreground: style.foreground.clone(),
        background: style.background.clone(),
        accent: style.accent.clone(),
        border_color: style.border_color.clone(),
        radius: style.radius,
        opacity: style.opacity,
        blur: style.blur,
        shine: style.shine,
        reflection: style.reflection,
        flare: style.flare,
        font: style.font.clone(),
        font_size: style.font_size.clone(),
        frost: bibelot::frost::Frost {
            enabled: frost.enabled,
            tint: frost.tint.clone(),
            opacity: frost.opacity,
            warmth: frost.warmth,
            grain: frost.grain,
            highlight: frost.highlight,
            shadow: frost.shadow,
        },
    }
}

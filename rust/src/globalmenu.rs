// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The global `menu` bar module, ported from `modules/menu.c`.
//!
//! The bar shows the focused application's menus where a traditional desktop
//! keeps a menu bar.  The data comes from the first producer that answers:
//!
//! 1. `#:menu-command`, a configuration's own `Menu<TAB>Item<TAB>…` program;
//! 2. `org.gtk.Menus` on the application's bus name;
//! 3. `com.canonical.dbusmenu`, including the X11 registrar's exact object path;
//! 4. the application's accessibility tree, read through AT-SPI;
//! 5. the curated application dictionary, Orbit corpus, desktop-entry actions
//!    and the shared shortcut/window fallback.
//!
//! Every row is owned Rust data ([`crate::menubar`]); the GTK layer only turns a
//! row into a button and switches between the top-level menus.  The D-Bus calls
//! are asynchronous and carry a generation token, so an answer for the previous
//! application can never replace the current application's menu.  AT-SPI runs
//! on its own worker thread and follows accessibility object events.
//!
//! The live `org.gtk.Menus.Changed` and DBusMenu `LayoutUpdated` watches remain
//! a later slice; those producers are re-read when focus or the X11 object graph
//! changes.  AT-SPI changes are followed live.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::atspi;
use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::chord::ChordStyle;
use crate::command;
use crate::menuapps::{self, MenuSources};
use crate::menubar::{self, Menu, MenuAction, MenuEntry};
use crate::menuprobe;
use crate::session::{self, FocusedPaths, Session, WindowActions};
use crate::style;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
const START_TIMEOUT_MS: i32 = 1500;
const REGISTRAR_TIMEOUT_MS: i32 = 1200;
const LAYOUT_TIMEOUT_MS: i32 = 1500;

const MENU_CSS: &str = "\
.whelk-menu-popover { padding: 4px; }\n\
.whelk-menu-bar .whelk-menu-button,\n\
.whelk-menu-bar .whelk-menu-app-button {\n\
  padding: 2px 7px;\n\
  min-height: 18px;\n\
}\n\
.whelk-menu-item { padding: 4px 8px; }\n\
.whelk-menu-accel { opacity: 0.62; }\n";

struct MenuState {
    row: gtk::Box,
    session: Session,
    show_shortcuts: bool,
    show_app_name: bool,
    style: ChordStyle,
    shortcut_tool: Option<String>,
    menu_command: Option<String>,
    sources: Rc<MenuSources>,
    command_timeout: Duration,
    bus: Option<gio::DBusConnection>,
    app_id: String,
    paths: FocusedPaths,
    pid: i32,
    app_name: String,
    actions: WindowActions,
    menus: Vec<Menu>,
    buttons: Vec<gtk::MenuButton>,
    /// The `#:menu-command` child.  Keeping the handle is what makes the
    /// deadline real: dropping a `CommandHandle` cancels its child.
    script: RefCell<Option<command::CommandHandle>>,
    /// The accessibility producer, when the protocol producers found nothing.
    /// Dropping the watch stops its worker.
    atspi: Option<atspi::Watch>,
    generation: u64,
    initialized: bool,
}

impl MenuState {
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn clear_row(&mut self) {
        while let Some(child) = self.row.first_child() {
            self.row.remove(&child);
        }

        self.buttons.clear();
    }

    fn close_menus(&self) {
        for button in &self.buttons {
            button.popdown();
        }
    }

    fn current_generation(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let session = context.session;
    let show_shortcuts = module
        .boolean("shortcuts?")
        .or_else(|| module.boolean("shortcuts"))
        .unwrap_or(true);
    let show_app_name = module
        .boolean("app-name?")
        .or_else(|| module.boolean("app-name"))
        .unwrap_or(true);
    let chord_style = ChordStyle::from_name(module.text("accelerator-style").as_deref());
    let shortcut_tool = match module.text("shortcut-tool") {
        Some(requested) if !requested.is_empty() => session::program_in_path(&requested),
        _ => session.shortcut_tool(),
    };
    let menu_command = module
        .text("menu-command")
        .filter(|value| !value.is_empty());
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let command_timeout = host.settings().command_timeout;
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok();

    row.add_css_class("whelk-menu-bar");
    row.set_visible(false);
    root.append(&row);

    let state = Rc::new(RefCell::new(MenuState {
        row: row.clone(),
        session,
        show_shortcuts,
        show_app_name,
        style: chord_style,
        shortcut_tool,
        menu_command,
        sources: Rc::clone(&context.menu_sources),
        command_timeout,
        bus,
        app_id: String::new(),
        paths: FocusedPaths::default(),
        pid: 0,
        app_name: String::new(),
        actions: WindowActions::default(),
        menus: Vec::new(),
        buttons: Vec::new(),
        script: RefCell::new(None),
        atspi: None,
        generation: 0,
        initialized: false,
    }));

    if let Some(provider) = install_css(&context.bar) {
        root.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    let update_state = Rc::clone(&state);

    host.start(move |host| update(host, &update_state, session));

    Some(root)
}

fn update(host: &ModuleHost, state: &Rc<RefCell<MenuState>>, session: Session) {
    if !host.is_alive() {
        return;
    }

    let Some(command_line) = session.focused_window_command() else {
        // No query verb (River, plain Wayland): there is still a shell-owned
        // application menu and the user's own `#:menu-command`.
        build_empty_if_needed(state);

        return;
    };
    let state = Rc::clone(state);

    host.run_command(&command_line, move |text| {
        let Some(text) = text else {
            build_empty_if_needed(&state);

            return;
        };

        on_focused_window(&state, &text);
    });
}

fn on_focused_window(state: &Rc<RefCell<MenuState>>, text: &str) {
    let session = state.borrow().session;
    let Some(window) = session.parse_focused(text) else {
        build_empty_if_needed(state);

        return;
    };
    let paths = session.parse_focused_paths(text);
    let pid = session.focused_pid(text);

    if state.borrow().initialized
        && state.borrow().app_id == window.app_id
        && state.borrow().paths == paths
        && state.borrow().pid == pid
    {
        return;
    }

    rebuild(state, &window.app_id, paths, pid);
}

fn build_empty_if_needed(state: &Rc<RefCell<MenuState>>) {
    let needs_rebuild = {
        let state = state.borrow();

        !state.initialized || !state.app_id.is_empty()
    };

    if needs_rebuild {
        rebuild(state, "", FocusedPaths::default(), 0);
    }
}

fn rebuild(state: &Rc<RefCell<MenuState>>, app_id: &str, paths: FocusedPaths, pid: i32) {
    let (generation, menu_command) = {
        let mut state = state.borrow_mut();

        state.invalidate();
        state.initialized = true;
        state.script.take();
        state.atspi.take();
        state.app_id = app_id.to_owned();
        state.paths = paths;
        state.pid = pid;
        state.actions = state.session.window_actions();

        let desktop = menuapps::desktop_entry_for_app_id(app_id);
        let built = menuapps::build_menus(
            &state.sources,
            app_id,
            desktop.as_ref(),
            &state.actions,
            state.shortcut_tool.as_deref(),
            state.show_shortcuts,
            state.style,
        );

        state.app_name = built.app_name;
        state.menus = built.menus;
        (state.generation, state.menu_command.clone())
    };

    render(state);

    if let Some(command_line) = menu_command {
        start_script(state, &command_line, generation);

        return;
    }

    probe_gtk(state, generation);
}

fn render(state: &Rc<RefCell<MenuState>>) {
    let (menus, app_name, actions, show_app_name) = {
        let mut menu_state = state.borrow_mut();

        menu_state.close_menus();
        menu_state.clear_row();

        (
            menu_state.menus.clone(),
            menu_state.app_name.clone(),
            menu_state.actions.clone(),
            menu_state.show_app_name,
        )
    };

    if show_app_name && !app_name.is_empty() {
        let application = menubar::application_menu(&actions);

        if MenuEntry::has_items(&application.entries) {
            add_top_button(state, &application, "whelk-menu-app-button", &app_name);
        }
    }

    for menu in menus.iter().filter(|menu| !menu.entries.is_empty()) {
        add_top_button(state, menu, "whelk-menu-button", &menu.label);
    }

    let visible = !state.borrow().buttons.is_empty();

    state.borrow().row.set_visible(visible);
}

fn add_top_button(state: &Rc<RefCell<MenuState>>, menu: &Menu, css_class: &str, label_text: &str) {
    let button = gtk::MenuButton::new();
    let label = gtk::Label::new(Some(label_text));
    let popover = gtk::Popover::new();

    button.set_has_frame(false);
    button.set_child(Some(&label));
    button.add_css_class("flat");
    button.add_css_class(css_class);

    if css_class == "whelk-menu-app-button" {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(22);
    }

    popover.add_css_class("whelk-menu-popover");
    popover.set_has_arrow(false);
    fill_popover(state, &popover, &menu.entries);
    attach_keys(&popover, state);
    button.set_popover(Some(&popover));

    let map_button = Rc::downgrade(state);

    popover.connect_map(|popover| {
        // A popover's children exist but are not laid out at map time; wait one
        // main-loop turn before moving keyboard focus into the first row.
        let popover = popover.clone();

        glib::idle_add_local_once(move || focus_first(&popover));
    });
    button.connect_map(move |button| {
        if let Some(state) = map_button.upgrade() {
            let any_open = state
                .borrow()
                .buttons
                .iter()
                .any(|button| button.is_active());

            if any_open && !button.is_active() {
                button.popup();
            }
        }
    });

    let weak = Rc::downgrade(state);
    let motion = gtk::EventControllerMotion::new();
    let motion_button = button.clone();

    motion.connect_enter(move |_, _, _| {
        let Some(state) = weak.upgrade() else {
            return;
        };
        let any_open = state
            .borrow()
            .buttons
            .iter()
            .any(|button| button.is_active());

        if any_open && !motion_button.is_active() {
            motion_button.popup();
        }
    });
    button.add_controller(motion);

    state.borrow_mut().buttons.push(button.clone());
    state.borrow().row.append(&button);
}

fn fill_popover(state: &Rc<RefCell<MenuState>>, popover: &gtk::Popover, entries: &[MenuEntry]) {
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-menu");
    popover.set_child(Some(&menu));

    for entry in entries {
        add_entry(state, &menu, entry, popover);
    }
}

fn add_entry(
    state: &Rc<RefCell<MenuState>>,
    menu: &gtk::Box,
    entry: &MenuEntry,
    parent: &gtk::Popover,
) {
    match entry {
        MenuEntry::Separator => {
            menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        MenuEntry::Item {
            label,
            enabled,
            submenu,
            ..
        } if !submenu.is_empty() => {
            let button = gtk::MenuButton::new();
            let popover = gtk::Popover::new();

            button.set_has_frame(false);
            button.set_label(label);
            button.set_direction(gtk::ArrowType::Right);
            button.add_css_class("flat");
            button.add_css_class("whelk-menu-item");
            button.set_halign(gtk::Align::Fill);
            button.set_sensitive(*enabled);
            popover.add_css_class("whelk-menu-popover");
            popover.set_has_arrow(false);
            fill_popover(state, &popover, submenu);
            attach_keys(&popover, state);
            button.set_popover(Some(&popover));
            menu.append(&button);
        }
        MenuEntry::Item {
            label,
            enabled,
            accel,
            action,
            ..
        } => {
            let button = gtk::Button::new();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);

            row.set_hexpand(true);

            let text = gtk::Label::new(Some(label));

            text.set_xalign(0.0);
            text.set_hexpand(true);
            row.append(&text);

            if let Some(accel) = accel.as_deref().filter(|value| !value.is_empty()) {
                let shortcut = gtk::Label::new(Some(accel));

                shortcut.add_css_class("whelk-menu-accel");
                row.append(&shortcut);
            }

            button.set_child(Some(&row));
            button.add_css_class("flat");
            button.add_css_class("whelk-menu-item");
            button.set_halign(gtk::Align::Fill);
            button.set_has_frame(false);
            button.set_sensitive(*enabled && action.is_some());

            if let Some(action) = action.clone() {
                let weak = Rc::downgrade(state);
                let popover = parent.downgrade();

                button.connect_clicked(move |_| {
                    if let Some(popover) = popover.upgrade() {
                        popover.popdown();
                    }

                    if let Some(state) = weak.upgrade() {
                        activate_action(&state, &action);
                    }
                });
            }

            menu.append(&button);
        }
    }
}

fn activate_action(state: &Rc<RefCell<MenuState>>, action: &MenuAction) {
    match action {
        MenuAction::Command(command_line) => command::spawn(command_line),
        MenuAction::DBusMenu { service, path, id } => {
            let Some(bus) = state.borrow().bus.clone() else {
                return;
            };
            let parameters = (*id, "clicked", 0i32.to_variant(), 0u32).to_variant();

            bus.call(
                Some(service),
                path,
                "com.canonical.dbusmenu",
                "Event",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                None::<&gio::Cancellable>,
                |_| {},
            );
        }
        MenuAction::GtkAction {
            service,
            path,
            name,
            target,
        } => {
            activate_gtk_action(state, service, path, name, target.as_deref());
        }
        MenuAction::GtkActionName(action) => {
            let Some((service, app_path)) = focused_action_target(state) else {
                return;
            };
            let (prefix, name) = action
                .split_once('.')
                .filter(|(_, name)| !name.is_empty())
                .unwrap_or((action, action));
            let path = if prefix == "win" {
                format!("{app_path}/windows/0")
            } else {
                app_path
            };

            activate_gtk_action(state, &service, &path, name, None);
        }
        MenuAction::DesktopAction { command, .. } => command::spawn(command),
        MenuAction::Atspi {
            service,
            path,
            action,
        } => atspi::activate(service, path, *action),
    }
}

fn activate_gtk_action(
    state: &Rc<RefCell<MenuState>>,
    service: &str,
    path: &str,
    name: &str,
    target: Option<&str>,
) {
    let Some(bus) = state.borrow().bus.clone() else {
        return;
    };
    let targets = target
        .map(|target| vec![target.to_variant()])
        .unwrap_or_default();
    let parameters = (
        name,
        targets,
        std::collections::HashMap::<String, glib::Variant>::new(),
    )
        .to_variant();

    bus.call(
        Some(service),
        path,
        "org.gtk.Actions",
        "Activate",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |_| {},
    );
}

fn focused_action_target(state: &Rc<RefCell<MenuState>>) -> Option<(String, String)> {
    let state = state.borrow();
    let service = state
        .paths
        .bus_name
        .clone()
        .filter(|name| gio::dbus_is_name(name))
        .or_else(|| gio::dbus_is_name(&state.app_id).then(|| state.app_id.clone()))?;
    let path = state
        .paths
        .app_path
        .clone()
        .filter(|path| glib::Variant::is_object_path(path))
        .or_else(|| app_path_for(&state.app_id))?;

    Some((service, path))
}

// ---------------------------------------------------------------------------
// The producers
// ---------------------------------------------------------------------------

fn start_script(state: &Rc<RefCell<MenuState>>, command_line: &str, generation: u64) {
    let weak = Rc::downgrade(state);
    let style = state.borrow().style;
    let timeout = state.borrow().command_timeout;
    let handle = command::run_with_timeout(command_line, timeout, move |text| {
        let Some(state) = weak.upgrade() else {
            return;
        };

        // The child is complete; releasing the handle here cannot cancel it.
        state.borrow_mut().script.take();

        let Some(text) = text else {
            return;
        };

        if !state.borrow().current_generation(generation) {
            return;
        }

        let menus = menubar::from_text(&text, style);

        if !menus.is_empty() {
            replace_menus(&state, menus);
        }
    });

    if let Some(handle) = handle
        && state.borrow().current_generation(generation)
    {
        *state.borrow_mut().script.borrow_mut() = Some(handle);
    }
}

struct Probe {
    state: Weak<RefCell<MenuState>>,
    generation: u64,
    bus: gio::DBusConnection,
    service: String,
    app_path: Option<String>,
    paths: Vec<String>,
    index: usize,
    dbus_paths: Vec<String>,
    dbus_index: usize,
    registrar_tried: bool,
    registrar_service: Option<String>,
}

fn probe_gtk(state: &Rc<RefCell<MenuState>>, generation: u64) {
    let (bus, app_id, paths) = {
        let state = state.borrow();

        (state.bus.clone(), state.app_id.clone(), state.paths.clone())
    };
    let Some(bus) = bus else {
        return;
    };
    let service = paths
        .bus_name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| gio::dbus_is_name(&app_id).then(|| app_id.clone()));
    let app_path = paths
        .app_path
        .clone()
        .filter(|path| glib::Variant::is_object_path(path))
        .or_else(|| app_path_for(&app_id));
    let valid_bus = service.as_deref().is_some_and(gio::dbus_is_name);

    if !valid_bus && paths.window_id == 0 {
        // A Wayland application whose id is not a bus name ("Alacritty",
        // "Freebuff") has no protocol menus, but it may still publish one to
        // AT-SPI.  Start the accessibility producer instead of giving up on
        // application menus before the fallback table.
        start_atspi(state, generation);

        return;
    }

    let order = menuprobe::order_new(Some(&paths), valid_bus, app_path.as_deref());
    let probe = Rc::new(RefCell::new(Probe {
        state: Rc::downgrade(state),
        generation,
        bus,
        service: service.unwrap_or_default(),
        app_path,
        paths: order.gtk_paths,
        index: 0,
        dbus_paths: order.dbus_paths,
        dbus_index: 0,
        registrar_tried: false,
        registrar_service: None,
    }));

    probe_gtk_next(probe);
}

fn probe_gtk_next(probe: Rc<RefCell<Probe>>) {
    let next = {
        let mut probe = probe.borrow_mut();

        if probe.index >= probe.paths.len() {
            None
        } else {
            let path = probe.paths[probe.index].clone();

            probe.index += 1;
            Some((
                probe.state.clone(),
                probe.generation,
                path,
                probe.service.clone(),
                probe.app_path.clone(),
                probe.bus.clone(),
            ))
        }
    };
    let Some((state, generation, path, service, app_path, bus)) = next else {
        probe_registrar(probe);

        return;
    };
    let Some(state) = state.upgrade() else {
        return;
    };

    if !state.borrow().current_generation(generation) {
        return;
    }

    let Some(reply_type) = glib::VariantTy::new("(a(uuaa{sv}))").ok() else {
        return;
    };
    let callback_probe = Rc::clone(&probe);
    let service = service.clone();
    let callback_service = service.clone();
    let app_path = app_path.clone();

    bus.call(
        Some(&service),
        &path,
        "org.gtk.Menus",
        "Start",
        Some(&(vec![0u32],).to_variant()),
        Some(reply_type),
        gio::DBusCallFlags::NONE,
        START_TIMEOUT_MS,
        None::<&gio::Cancellable>,
        move |result| {
            let Ok(reply) = result else {
                probe_gtk_next(callback_probe);

                return;
            };
            let Some(state) = callback_probe.borrow().state.upgrade() else {
                return;
            };

            if !state.borrow().current_generation(generation) {
                return;
            }

            let body = reply.child_value(0);
            let menus = menubar::from_gtk(
                &body,
                &callback_service,
                app_path.as_deref(),
                state.borrow().style,
            );

            if menus.is_empty() {
                probe_gtk_next(callback_probe);
            } else {
                replace_menus(&state, menus);
            }
        },
    );
}

fn probe_registrar(probe: Rc<RefCell<Probe>>) {
    let (state, generation, window_id, bus, tried) = {
        let probe = probe.borrow_mut();

        (
            probe.state.clone(),
            probe.generation,
            state_window_id(&probe),
            probe.bus.clone(),
            probe.registrar_tried,
        )
    };
    let Some(state) = state.upgrade() else {
        return;
    };

    if !state.borrow().current_generation(generation) {
        return;
    }

    if tried || window_id == 0 {
        probe_dbus_next(probe);

        return;
    }

    probe.borrow_mut().registrar_tried = true;

    let Some(reply_type) = glib::VariantTy::new("(so)").ok() else {
        return;
    };
    let callback_probe = Rc::clone(&probe);

    bus.call(
        Some("com.canonical.AppMenu.Registrar"),
        "/com/canonical/AppMenu/Registrar",
        "com.canonical.AppMenu.Registrar",
        "GetMenuForWindow",
        Some(&(window_id,).to_variant()),
        Some(reply_type),
        gio::DBusCallFlags::NONE,
        REGISTRAR_TIMEOUT_MS,
        None::<&gio::Cancellable>,
        move |result| {
            let Ok(reply) = result else {
                probe_dbus_next(callback_probe);

                return;
            };
            let Some(service) = reply
                .try_child_value(0)
                .and_then(|value| value.get::<String>())
            else {
                probe_dbus_next(callback_probe);

                return;
            };
            let Some(path) = reply
                .try_child_value(1)
                .and_then(|value| value.get::<String>())
            else {
                probe_dbus_next(callback_probe);

                return;
            };

            if !gio::dbus_is_name(&service) || !glib::Variant::is_object_path(&path) {
                probe_dbus_next(callback_probe);

                return;
            }

            {
                let mut probe = callback_probe.borrow_mut();

                menuprobe::use_registrar_path(&mut probe.dbus_paths, &path);
                probe.registrar_service = Some(service);
                probe.dbus_index = 0;
            }

            probe_dbus_next(callback_probe);
        },
    );
}

fn probe_dbus_next(probe: Rc<RefCell<Probe>>) {
    let (exhausted, service, state, generation, path, bus) = {
        let mut probe = probe.borrow_mut();

        if probe.dbus_index >= probe.dbus_paths.len() {
            (
                true,
                String::new(),
                probe.state.clone(),
                probe.generation,
                String::new(),
                probe.bus.clone(),
            )
        } else {
            let service = probe
                .registrar_service
                .clone()
                .or_else(|| (!probe.service.is_empty()).then(|| probe.service.clone()))
                .unwrap_or_default();
            let path = probe.dbus_paths[probe.dbus_index].clone();

            probe.dbus_index += 1;
            (
                false,
                service,
                probe.state.clone(),
                probe.generation,
                path,
                probe.bus.clone(),
            )
        }
    };

    if exhausted {
        start_atspi_probe(&probe);

        return;
    }

    if !gio::dbus_is_name(&service) {
        start_atspi_probe(&probe);

        return;
    }
    let Some(state) = state.upgrade() else {
        return;
    };

    if !state.borrow().current_generation(generation) {
        return;
    }

    let Some(reply_type) = glib::VariantTy::new("(u(ia{sv}av))").ok() else {
        return;
    };
    let callback_probe = Rc::clone(&probe);
    let callback_path = path.clone();
    let callback_service = service.clone();

    bus.call(
        Some(&service),
        &path,
        "com.canonical.dbusmenu",
        "GetLayout",
        Some(&(0i32, -1i32).to_variant()),
        Some(reply_type),
        gio::DBusCallFlags::NONE,
        LAYOUT_TIMEOUT_MS,
        None::<&gio::Cancellable>,
        move |result| {
            let Ok(reply) = result else {
                probe_dbus_next(callback_probe);

                return;
            };
            let Some(state) = callback_probe.borrow().state.upgrade() else {
                return;
            };

            if !state.borrow().current_generation(generation) {
                return;
            }

            let menus = menubar::from_dbusmenu(
                &reply,
                &callback_service,
                &callback_path,
                state.borrow().style,
            );

            if menus.is_empty() {
                probe_dbus_next(callback_probe);
            } else {
                replace_menus(&state, menus);
            }
        },
    );
}

fn start_atspi_probe(probe: &Rc<RefCell<Probe>>) {
    let (state, generation) = {
        let probe = probe.borrow();

        (probe.state.clone(), probe.generation)
    };

    let Some(state) = state.upgrade() else {
        return;
    };

    if !state.borrow().current_generation(generation) {
        return;
    }

    start_atspi(&state, generation);
}

fn start_atspi(state: &Rc<RefCell<MenuState>>, generation: u64) {
    let (app_name, pid, style) = {
        let state = state.borrow();

        if state.atspi.is_some() {
            return;
        }

        (state.app_name.clone(), state.pid, state.style)
    };
    let weak = Rc::downgrade(state);
    let Some(watch) = atspi::Watch::start(app_name, pid, style, move |menus| {
        if menus.is_empty() {
            return;
        }

        let Some(state) = weak.upgrade() else {
            return;
        };

        if !state.borrow().current_generation(generation) {
            return;
        }

        replace_menus(&state, menus);
    }) else {
        return;
    };

    let mut state = state.borrow_mut();

    if state.current_generation(generation) && state.atspi.is_none() {
        state.atspi = Some(watch);
    }
}

fn state_window_id(probe: &Probe) -> u32 {
    let state = probe.state.upgrade();

    state.map_or(0, |state| state.borrow().paths.window_id)
}

fn replace_menus(state: &Rc<RefCell<MenuState>>, menus: Vec<Menu>) {
    state.borrow_mut().menus = menus;
    render(state);
}

fn app_path_for(app_id: &str) -> Option<String> {
    let path = if app_id.is_empty() {
        "/org/gtk/Application/anonymous".to_owned()
    } else {
        format!("/{}", app_id.replace('.', "/"))
    };

    glib::Variant::is_object_path(&path).then_some(path)
}

// ---------------------------------------------------------------------------
// Keyboard and focus
// ---------------------------------------------------------------------------

fn attach_keys(popover: &gtk::Popover, state: &Rc<RefCell<MenuState>>) {
    let keys = gtk::EventControllerKey::new();
    let weak = popover.downgrade();
    let state = Rc::downgrade(state);

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        let Some(popover) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };

        if key == gtk::gdk::Key::Escape {
            popover.popdown();

            return glib::Propagation::Stop;
        }

        let focus = popover
            .root()
            .and_then(|root| root.focus())
            .filter(|focus| focus.is_ancestor(&popover));

        if (key == gtk::gdk::Key::Left || key == gtk::gdk::Key::Right)
            && let Some(state) = state.upgrade()
        {
            if key == gtk::gdk::Key::Right
                && let Some(button) = focus
                    .as_ref()
                    .and_then(|focus| focus.downcast_ref::<gtk::MenuButton>())
                && let Some(submenu) = button.popover()
            {
                submenu.popup();

                return glib::Propagation::Stop;
            }

            switch_top_level(&state, key == gtk::gdk::Key::Right);

            return glib::Propagation::Stop;
        }

        if key == gtk::gdk::Key::Home || key == gtk::gdk::Key::End {
            let Some(menu) = popover.child() else {
                return glib::Propagation::Proceed;
            };

            let target = if key == gtk::gdk::Key::Home {
                first_item(&menu)
            } else {
                last_item(&menu)
            };

            if let Some(target) = target {
                target.grab_focus();
            }

            return glib::Propagation::Stop;
        }

        if key != gtk::gdk::Key::Down && key != gtk::gdk::Key::Up {
            return glib::Propagation::Proceed;
        }

        let Some(menu) = popover.child() else {
            return glib::Propagation::Proceed;
        };

        if let Some(target) = neighbour_item(&menu, focus.as_ref(), key == gtk::gdk::Key::Down) {
            target.grab_focus();
        }

        glib::Propagation::Stop
    });
    popover.add_controller(keys);
}

fn switch_top_level(state: &Rc<RefCell<MenuState>>, forward: bool) {
    let buttons = state.borrow().buttons.clone();

    if buttons.len() < 2 {
        return;
    }

    let Some(current) = buttons.iter().position(|button| button.is_active()) else {
        return;
    };
    let next = if forward {
        (current + 1) % buttons.len()
    } else {
        (current + buttons.len() - 1) % buttons.len()
    };

    buttons[current].popdown();
    buttons[next].popup();
}

fn focus_first(popover: &gtk::Popover) {
    let Some(menu) = popover.child() else {
        return;
    };

    if let Some(item) = first_item(&menu) {
        item.grab_focus();
    }
}

fn first_item(menu: &gtk::Widget) -> Option<gtk::Widget> {
    let mut child = menu.first_child();

    while let Some(widget) = child {
        if focusable(&widget) {
            return Some(widget);
        }

        child = widget.next_sibling();
    }

    None
}

fn last_item(menu: &gtk::Widget) -> Option<gtk::Widget> {
    let mut child = menu.first_child();
    let mut last = None;

    while let Some(widget) = child {
        if focusable(&widget) {
            last = Some(widget.clone());
        }

        child = widget.next_sibling();
    }

    last
}

fn neighbour_item(
    menu: &gtk::Widget,
    from: Option<&gtk::Widget>,
    down: bool,
) -> Option<gtk::Widget> {
    let mut items = Vec::new();
    let mut child = menu.first_child();

    while let Some(widget) = child {
        if focusable(&widget) {
            items.push(widget.clone());
        }

        child = widget.next_sibling();
    }

    if items.is_empty() {
        return None;
    }

    if let Some(from) = from
        && let Some(index) = items.iter().position(|item| item == from)
    {
        let count = items.len();

        return Some(
            items[if down {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            }]
            .clone(),
        );
    }

    Some(if down {
        items[0].clone()
    } else {
        items[items.len() - 1].clone()
    })
}

fn focusable(widget: &gtk::Widget) -> bool {
    (widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>())
        && widget.is_sensitive()
        && widget.is_visible()
}

// ---------------------------------------------------------------------------
// Appearance
// ---------------------------------------------------------------------------

fn install_css(style: &BarSettings) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let mut css = MENU_CSS.to_owned();

    if let Some(foreground) = style
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(
            ".whelk-menu-bar, .whelk-menu-popover {{ color: {foreground}; }}\n"
        ));
    }

    if let Some(background) = style::solid(style.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-menu-popover {{ background-color: {background}; }}\n"
        ));
    }

    let provider = gtk::CssProvider::new();

    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_paths_are_derived_only_when_they_are_object_paths() {
        assert_eq!(
            app_path_for("org.gnome.Nautilus").as_deref(),
            Some("/org/gnome/Nautilus")
        );
        assert_eq!(
            app_path_for("").as_deref(),
            Some("/org/gtk/Application/anonymous")
        );
        assert_eq!(app_path_for("has space").as_deref(), None);
    }
}

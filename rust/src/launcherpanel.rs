// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher overlay, ported from `src/launcher.c`.
//!
//! One full-output layer surface, one search entry and one ranked list.  The
//! databases and their parsing live in [`crate::launcherdata`]; this half owns
//! the widgets, the debounce, the selection and the action a chosen row runs.
//! The clipboard and live menu-item modes still belong to the C shell and say
//! so instead of pretending to be empty.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::command;
use crate::filesearch;
use crate::launcher as launcher_geometry;
use crate::launcherdata::{self, ItemKind, LauncherItem, RESULT_LIMIT};
use crate::launcherkeys::LauncherKeys;
use crate::style;

const DEBOUNCE: Duration = Duration::from_millis(120);
const FILE_DEBOUNCE: Duration = Duration::from_millis(260);
const WALK_MAX_AGE: Duration = Duration::from_secs(30);
const CARD_TOP: i32 = 120;
const HINT: &str = "> run   = calc   / files   ? web   @ windows   % ssh   ! close   ; clipboard   : menu items   ·   Tab completes   ·   Esc closes";

thread_local! {
    static CURRENT: RefCell<Option<Launcher>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct Launcher {
    inner: Rc<RefCell<State>>,
}

struct State {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    entry: gtk::SearchEntry,
    list: gtk::ListBox,
    scroller: gtk::ScrolledWindow,
    mode_label: gtk::Label,
    empty: gtk::Label,
    close: gtk::Button,
    icon_minus: gtk::Button,
    icon_plus: gtk::Button,
    font_minus: gtk::Button,
    font_plus: gtk::Button,
    grip: gtk::Label,
    icon_value: gtk::Label,
    font_value: gtk::Label,
    css: Option<gtk::CssProvider>,
    zoom_css: Option<gtk::CssProvider>,
    font_base: Option<String>,

    apps: Option<Vec<LauncherItem>>,
    commands: Option<Vec<LauncherItem>>,
    hosts: Option<Vec<LauncherItem>>,
    windows: Vec<LauncherItem>,
    files: Vec<LauncherItem>,
    menu: Vec<LauncherItem>,
    results: Vec<LauncherItem>,

    keys: LauncherKeys,
    mode: String,
    menu_command: Option<String>,

    width: i32,
    height: i32,
    icon_size: i32,
    font_scale: i32,
    drag_width: i32,
    drag_height: i32,

    query_source: Option<gtk::glib::SourceId>,
    files_generation: u64,
    windows_generation: u64,
    menu_generation: u64,
    last_file_query: Option<String>,
    files_walk: Option<String>,
    files_walk_at: Option<Instant>,
}

impl Launcher {
    pub fn new(application: &gtk::Application, style: &BarSettings) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk-rs launcher")
            .build();

        window.set_widget_name(&style::widget_id("launcher"));
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(true);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let entry = gtk::SearchEntry::new();
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let brand = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let brand_row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        let mark = gtk::Label::new(Some("●"));
        let title = gtk::Label::new(Some("whelk"));
        let subtitle = gtk::Label::new(Some("launcher"));
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        let mode_label = gtk::Label::new(Some("Applications"));
        let hint = gtk::Label::new(Some(HINT));
        let list = gtk::ListBox::new();
        let scroller = gtk::ScrolledWindow::new();
        let empty = gtk::Label::new(Some("Nothing matched — try a different search."));
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let icon_label = gtk::Label::new(Some("Icons"));
        let icon_minus = gtk::Button::with_label("−");
        let icon_plus = gtk::Button::with_label("+");
        let font_label = gtk::Label::new(Some("Text"));
        let font_minus = gtk::Button::with_label("A−");
        let font_plus = gtk::Button::with_label("A+");
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let grip = gtk::Label::new(Some("◢"));
        let icon_value = gtk::Label::new(None);
        let font_value = gtk::Label::new(None);

        card.add_css_class("whelk-launcher");
        entry.add_css_class("whelk-launcher-entry");
        entry.set_placeholder_text(Some("Search applications, windows, commands…"));
        entry.set_hexpand(true);
        header.add_css_class("whelk-launcher-header");
        brand.add_css_class("whelk-launcher-brand");
        mark.add_css_class("whelk-launcher-mark");
        mark.add_css_class("whelk-accent");
        title.add_css_class("whelk-launcher-title");
        subtitle.add_css_class("whelk-launcher-subtitle");
        title.set_xalign(0.0);
        subtitle.set_xalign(0.0);
        brand.set_hexpand(true);
        brand_row.append(&mark);
        brand_row.append(&title);
        brand.append(&brand_row);
        brand.append(&subtitle);
        close.add_css_class("whelk-launcher-close");
        close.add_css_class("circular");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close the launcher (Esc)"));
        close.set_halign(gtk::Align::End);
        header.append(&brand);
        header.append(&close);

        mode_label.add_css_class("whelk-launcher-mode");
        mode_label.set_xalign(0.0);
        mode_label.set_halign(gtk::Align::Start);
        hint.add_css_class("whelk-launcher-hint");
        hint.set_xalign(0.0);
        hint.set_wrap(true);

        footer.add_css_class("whelk-launcher-footer");
        icon_label.add_css_class("whelk-launcher-zoom-label");
        font_label.add_css_class("whelk-launcher-zoom-label");
        grip.add_css_class("whelk-launcher-grip");
        spacer.set_hexpand(true);
        grip.set_tooltip_text(Some("Drag to resize the launcher"));

        for (button, tooltip) in [
            (&icon_minus, "Smaller icons"),
            (&icon_plus, "Larger icons"),
            (&font_minus, "Smaller text"),
            (&font_plus, "Larger text"),
        ] {
            button.add_css_class("whelk-launcher-zoom");
            button.add_css_class("flat");
            button.set_tooltip_text(Some(tooltip));
        }

        icon_value.add_css_class("whelk-launcher-zoom-value");
        font_value.add_css_class("whelk-launcher-zoom-value");
        footer.append(&icon_label);
        footer.append(&icon_minus);
        footer.append(&icon_plus);
        footer.append(&icon_value);
        footer.append(&spacer);
        footer.append(&font_label);
        footer.append(&font_minus);
        footer.append(&font_plus);
        footer.append(&font_value);
        footer.append(&grip);

        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("whelk-launcher-list");
        list.set_activate_on_single_click(true);
        scroller.add_css_class("whelk-launcher-scroller");
        scroller.set_child(Some(&list));
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);

        empty.add_css_class("whelk-launcher-empty");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Center);
        empty.set_visible(false);

        card.append(&header);
        card.append(&entry);
        card.append(&mode_label);
        card.append(&scroller);
        card.append(&empty);
        card.append(&hint);
        card.append(&footer);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Start);
        card.set_margin_top(CARD_TOP);
        card.set_hexpand(false);

        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_hexpand(true);
        holder.set_vexpand(true);
        holder.append(&card);
        window.set_child(Some(&holder));

        let width = launcher_geometry::clamp_width(style.launcher_width);
        let height = launcher_geometry::clamp_height(style.launcher_height);
        let icon_size = launcher_geometry::clamp_icon_size(style.launcher_icon_size);
        let font_scale = launcher_geometry::clamp_font_scale(style.launcher_font_scale);

        let inner = Rc::new(RefCell::new(State {
            window,
            card,
            entry,
            list,
            scroller,
            mode_label,
            empty,
            close,
            icon_minus,
            icon_plus,
            font_minus,
            font_plus,
            grip,
            icon_value,
            font_value,
            css: None,
            zoom_css: None,
            font_base: None,
            apps: None,
            commands: None,
            hosts: None,
            windows: Vec::new(),
            files: Vec::new(),
            menu: Vec::new(),
            results: Vec::new(),
            keys: LauncherKeys::new(),
            mode: "app".to_owned(),
            menu_command: None,
            width,
            height,
            icon_size,
            font_scale,
            drag_width: width,
            drag_height: height,
            query_source: None,
            files_generation: 0,
            windows_generation: 0,
            menu_generation: 0,
            last_file_query: None,
            files_walk: None,
            files_walk_at: None,
        }));
        let launcher = Self { inner };

        launcher.install_style(style);
        launcher.configure_surface();
        launcher.apply_geometry();
        launcher.update_zoom_labels();
        launcher.connect_controls();
        launcher
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub fn install(launcher: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = launcher);
    }

    pub fn show_current(mode: Option<&str>) {
        if let Some(launcher) = Self::current() {
            launcher.show(mode);
        }
    }

    pub fn hide_current() {
        if let Some(launcher) = Self::current() {
            launcher.hide();
        }
    }

    pub fn toggle_current(mode: Option<&str>) {
        if let Some(launcher) = Self::current() {
            if launcher.is_visible() {
                launcher.hide();
            } else {
                launcher.show(mode);
            }
        }
    }

    pub fn show(&self, mode: Option<&str>) {
        let chosen = mode
            .filter(|mode| !mode.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.inner.borrow().keys.default_mode().to_owned());
        let chosen = resolve_mode(&chosen);

        {
            let mut state = self.inner.borrow_mut();

            state.mode = chosen.to_owned();

            if chosen == "session" && state.menu.is_empty() {
                state.menu = launcherdata::session_items();
            }
        }

        // Setting the text emits `changed`, whose callback takes another
        // borrow of the state; keep the entry and window borrowed only for the
        // calls themselves, never across a signal.
        let (entry, window) = {
            let state = self.inner.borrow();

            (state.entry.clone(), state.window.clone())
        };

        window.present();
        entry.set_text("");
        self.cancel_query();

        if chosen == "window" || chosen == "quit" {
            self.refresh_windows();
        }

        self.refresh_results();
        self.inner.borrow().entry.grab_focus();
    }

    pub fn hide(&self) {
        self.cancel_query();
        self.inner.borrow().window.set_visible(false);
    }

    pub fn is_visible(&self) -> bool {
        self.inner.borrow().window.is_visible()
    }

    pub fn showing(&self, mode: &str) -> bool {
        self.is_visible() && self.inner.borrow().mode == mode
    }

    /// Show the generic menu mode for a generator command.
    pub fn show_menu(&self, command_line: &str) {
        if command_line.is_empty() {
            return;
        }

        let generation = {
            let mut state = self.inner.borrow_mut();

            state.menu_command = Some(command_line.to_owned());
            state.mode = "menu".to_owned();
            state.menu_generation = state.menu_generation.wrapping_add(1);

            state.menu_generation
        };
        let weak = Rc::downgrade(&self.inner);
        let listing = command::with_retv(command_line, "0");

        command::run_bytes(&listing, move |bytes| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = Launcher { inner };

            if panel.inner.borrow().menu_generation != generation {
                return;
            }

            panel.inner.borrow_mut().menu =
                launcherdata::parse_menu_bytes(bytes.as_deref().unwrap_or_default());
            panel.show(Some("menu"));
        });
    }

    /// Show the built-in session menu: lock, log out, suspend, hibernate,
    /// restart and shut down.
    pub fn show_session_menu(&self) {
        self.inner.borrow_mut().menu = launcherdata::session_items();
        self.inner.borrow_mut().menu_command = None;
        self.show(Some("session"));
    }

    pub fn set_style(&self, style: &BarSettings) {
        {
            let mut state = self.inner.borrow_mut();

            state.width = launcher_geometry::clamp_width(style.launcher_width);
            state.height = launcher_geometry::clamp_height(style.launcher_height);
            state.icon_size = launcher_geometry::clamp_icon_size(style.launcher_icon_size);
            state.font_scale = launcher_geometry::clamp_font_scale(style.launcher_font_scale);
            state.drag_width = state.width;
            state.drag_height = state.height;
        }

        self.install_style(style);
        self.apply_geometry();
        self.apply_icon_size();
        self.update_zoom_labels();
        self.inner.borrow().card.queue_draw();
    }

    fn configure_surface(&self) {
        let state = self.inner.borrow();
        let window = &state.window;

        if crate::ui::layer_shell_available() {
            window.init_layer_shell();
            window.set_namespace(Some("whelk-rust-launcher"));
            window.set_layer(Layer::Overlay);
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Left, true);
            window.set_anchor(Edge::Right, true);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
            window.set_default_size(800, 800);
        } else {
            window.set_default_size(900, 720);
        }

        window.present();
        window.set_visible(false);
    }

    fn connect_controls(&self) {
        let state = self.inner.borrow();
        let weak = Rc::downgrade(&self.inner);

        state.close.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                Launcher { inner }.hide();
            }
        });

        let weak = Rc::downgrade(&self.inner);
        state.entry.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Launcher { inner }.schedule_query();
            }
        });

        let weak = Rc::downgrade(&self.inner);
        state.entry.connect_activate(move |_| {
            if let Some(inner) = weak.upgrade() {
                Launcher { inner }.activate_selected();
            }
        });

        let weak = Rc::downgrade(&self.inner);
        state.entry.connect_stop_search(move |_| {
            if let Some(inner) = weak.upgrade() {
                Launcher { inner }.hide();
            }
        });

        let weak = Rc::downgrade(&self.inner);
        state.list.connect_row_activated(move |_, row| {
            if let Some(inner) = weak.upgrade() {
                Launcher { inner }.activate_index(row.index() as usize);
            }
        });

        for (button, delta, font) in [
            (state.icon_minus.clone(), -4, false),
            (state.icon_plus.clone(), 4, false),
            (state.font_minus.clone(), -10, true),
            (state.font_plus.clone(), 10, true),
        ] {
            let weak = Rc::downgrade(&self.inner);

            button.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    let panel = Launcher { inner };

                    if font {
                        panel.zoom_font(delta);
                    } else {
                        panel.zoom_icons(delta);
                    }
                }
            });
        }

        let weak = Rc::downgrade(&self.inner);
        let grip = state.grip.clone();
        let resize = gtk::GestureDrag::new();

        resize.connect_drag_begin({
            let weak = weak.clone();

            move |_, _, _| {
                if let Some(inner) = weak.upgrade() {
                    let panel = Launcher { inner };
                    let mut state = panel.inner.borrow_mut();

                    state.drag_width = state.width;
                    state.drag_height = state.height;
                }
            }
        });
        resize.connect_drag_update(move |_, offset_x, offset_y| {
            if let Some(inner) = weak.upgrade() {
                let panel = Launcher { inner };
                let mut state = panel.inner.borrow_mut();

                state.width =
                    launcher_geometry::clamp_width(state.drag_width + 2 * offset_x.round() as i32);
                state.height =
                    launcher_geometry::clamp_height(state.drag_height + offset_y.round() as i32);
                state.card.set_size_request(state.width, state.height);
            }
        });
        grip.add_controller(resize);
        grip.set_cursor_from_name(Some("nwse-resize"));

        let weak = Rc::downgrade(&self.inner);
        let keys = gtk::EventControllerKey::new();

        keys.connect_key_pressed(move |_, keyval, _, modifiers| {
            let Some(inner) = weak.upgrade() else {
                return gtk::glib::Propagation::Proceed;
            };

            Launcher { inner }.key_pressed(keyval, modifiers)
        });
        state.window.add_controller(keys);

        let weak = Rc::downgrade(&self.inner);
        let outside = gtk::GestureClick::new();

        outside.set_button(gtk::gdk::BUTTON_PRIMARY);
        outside.connect_pressed(move |gesture, _, x, y| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = Launcher { inner };

            if panel.point_outside(x, y) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                panel.hide();
            }
        });
        state.window.add_controller(outside);
    }

    fn key_pressed(
        &self,
        keyval: gtk::gdk::Key,
        _modifiers: gtk::gdk::ModifierType,
    ) -> gtk::glib::Propagation {
        match keyval {
            gtk::gdk::Key::Escape => {
                self.hide();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Down => {
                self.move_selection(1);
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Up => {
                self.move_selection(-1);
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Tab => {
                self.complete_selection();
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
                self.activate_selected();
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        }
    }

    fn schedule_query(&self) {
        let mode = self.inner.borrow().mode.clone();
        let text = self.inner.borrow().entry.text().to_string();
        let delay = if text.starts_with('/') || mode == "file" {
            FILE_DEBOUNCE
        } else {
            DEBOUNCE
        };
        let weak = Rc::downgrade(&self.inner);

        self.cancel_query();
        let source = gtk::glib::timeout_add_local_once(delay, move || {
            if let Some(inner) = weak.upgrade() {
                let panel = Launcher { inner };

                panel.inner.borrow_mut().query_source = None;
                panel.refresh_results();
            }
        });

        self.inner.borrow_mut().query_source = Some(source);
    }

    fn cancel_query(&self) {
        let source = self.inner.borrow_mut().query_source.take();

        if let Some(source) = source {
            source.remove();
        }
    }

    fn effective_mode(&self, text: &str) -> (String, String) {
        let state = self.inner.borrow();

        if state.mode == "menu" || state.mode == "session" {
            return (state.mode.clone(), text.to_owned());
        }

        if let Some((mode, query)) = state.keys.mode_for_text(text) {
            return (mode.to_owned(), query.to_owned());
        }

        (resolve_mode(&state.mode).to_owned(), text.to_owned())
    }

    fn refresh_results(&self) {
        let text = self.inner.borrow().entry.text().to_string();
        let (mode, query) = self.effective_mode(&text);

        if mode == "file" {
            self.prepare_files(&query);
        }

        let results = match mode.as_str() {
            "run" => self.run_results(&query),
            "calc" => calc_results(&query),
            "web" => web_results(&query),
            "window" => self.window_results(&query, false),
            "quit" => self.window_results(&query, true),
            "ssh" => {
                self.ensure_hosts();
                let state = self.inner.borrow();

                state
                    .hosts
                    .as_deref()
                    .map(|items| ranked(items, &query))
                    .unwrap_or_default()
            }
            "file" => self.inner.borrow().files.clone(),
            "menu" | "session" => {
                let state = self.inner.borrow();

                ranked(&state.menu, &query)
            }
            "clipboard" => vec![unsupported(
                "Clipboard history",
                "The clipboard mode is still served by the C shell",
            )],
            "menuitem" => vec![unsupported(
                "Application menu items",
                "The live menu-item mode is still served by the C shell",
            )],
            _ => {
                self.ensure_apps();
                let state = self.inner.borrow();

                state
                    .apps
                    .as_deref()
                    .map(|items| ranked(items, &query))
                    .unwrap_or_default()
            }
        };

        self.paint_results(&mode, results);
    }

    fn paint_results(&self, mode: &str, results: Vec<LauncherItem>) {
        let (list, scroller, empty, mode_label, icon_size) = {
            let state = self.inner.borrow();

            (
                state.list.clone(),
                state.scroller.clone(),
                state.empty.clone(),
                state.mode_label.clone(),
                state.icon_size,
            )
        };

        clear_list(&list);

        for item in &results {
            list.append(&build_row(item, icon_size));
        }

        let count = results.len();
        let label = launcherdata::mode_label(mode);
        let suffix = if mode == "clipboard" {
            " · Ctrl+P pins · Ctrl+Delete removes · Ctrl+Z undoes"
        } else {
            ""
        };

        mode_label.set_text(&format!("{label} — {count}{suffix}"));
        scroller.set_visible(count > 0);
        empty.set_visible(count == 0);

        self.inner.borrow_mut().results = results;

        if let Some(first) = list.row_at_index(0) {
            list.select_row(Some(&first));
        }

        self.scroll_selected_into_view();
    }

    fn ensure_apps(&self) {
        if self.inner.borrow().apps.is_some() {
            return;
        }

        let apps = launcherdata::load_apps();

        self.inner.borrow_mut().apps = Some(apps);
    }

    fn ensure_commands(&self) {
        if self.inner.borrow().commands.is_some() {
            return;
        }

        let commands = launcherdata::load_commands();

        self.inner.borrow_mut().commands = Some(commands);
    }

    fn ensure_hosts(&self) {
        if self.inner.borrow().hosts.is_some() {
            return;
        }

        let hosts = launcherdata::load_ssh_hosts();

        self.inner.borrow_mut().hosts = Some(hosts);
    }

    fn run_results(&self, query: &str) -> Vec<LauncherItem> {
        self.ensure_commands();

        let mut results = Vec::new();

        if !query.is_empty() {
            results.push(
                LauncherItem::new(ItemKind::Command, query, "Run this command")
                    .with_action(query)
                    .with_icon(Some("utilities-terminal-symbolic")),
            );
        }

        let state = self.inner.borrow();

        if let Some(commands) = state.commands.as_deref() {
            results.extend(ranked(commands, query));
        }

        results
    }

    fn window_results(&self, query: &str, quit: bool) -> Vec<LauncherItem> {
        let state = self.inner.borrow();
        let mut items = state.windows.clone();

        if quit {
            for item in &mut items {
                item.kind = ItemKind::Quit;
                item.icon = Some("window-close-symbolic".to_owned());
            }
        }

        ranked(&items, query)
    }

    fn prepare_files(&self, query: &str) {
        let mut state = self.inner.borrow_mut();

        if query.is_empty() {
            state.files.clear();
            state.last_file_query = None;

            return;
        }

        if state.last_file_query.as_deref() == Some(query) {
            return;
        }

        state.last_file_query = Some(query.to_owned());
        state.files.clear();
        state.files_generation = state.files_generation.wrapping_add(1);

        let generation = state.files_generation;
        let cached = state.files_walk.clone().filter(|_| {
            state
                .files_walk_at
                .is_some_and(|at| at.elapsed() < WALK_MAX_AGE)
        });

        if let Some(walk) = cached {
            state.files = launcherdata::file_items(query, &walk, RESULT_LIMIT as u32);

            return;
        }

        drop(state);

        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
        let command_line = filesearch::command(&home, 6);
        let weak = Rc::downgrade(&self.inner);
        let requested = query.to_owned();

        command::run(&command_line, move |text| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = Launcher { inner };
            let mut state = panel.inner.borrow_mut();

            if state.files_generation != generation {
                return;
            }

            state.files = text
                .as_deref()
                .map(|walk| launcherdata::file_items(&requested, walk, RESULT_LIMIT as u32))
                .unwrap_or_default();
            state.files_walk = text;
            state.files_walk_at = Some(Instant::now());

            let refresh = state.mode == "file";

            drop(state);

            if refresh {
                panel.refresh_results();
            }
        });
    }

    fn refresh_windows(&self) {
        let generation = {
            let mut state = self.inner.borrow_mut();

            state.windows_generation = state.windows_generation.wrapping_add(1);

            state.windows_generation
        };
        let weak = Rc::downgrade(&self.inner);

        command::run("niri msg -j windows", move |text| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let panel = Launcher { inner };
            let mut state = panel.inner.borrow_mut();

            if state.windows_generation != generation {
                return;
            }

            state.windows =
                launcherdata::parse_niri_windows(text.as_deref().unwrap_or_default(), false);

            let refresh = state.mode == "window" || state.mode == "quit";

            drop(state);

            if refresh {
                panel.refresh_results();
            }
        });
    }

    fn selected_item(&self) -> Option<LauncherItem> {
        let state = self.inner.borrow();
        let row = state.list.selected_row()?;
        let index = row.index() as usize;

        state.results.get(index).cloned()
    }

    fn activate_selected(&self) {
        let state = self.inner.borrow();
        let Some(row) = state.list.selected_row() else {
            return;
        };

        let index = row.index() as usize;

        drop(state);
        self.activate_index(index);
    }

    fn activate_index(&self, index: usize) {
        let state = self.inner.borrow();
        let Some(item) = state.results.get(index).cloned() else {
            return;
        };

        if item.disabled {
            return;
        }

        let menu_command = state.menu_command.clone();

        drop(state);

        match item.kind {
            ItemKind::App => {
                command::spawn(&format!("gio launch {}", quote(&item.action)));
            }
            ItemKind::Window => {
                command::spawn(&format!(
                    "niri msg action focus-window --id {}",
                    item.action
                ));
            }
            ItemKind::Quit => {
                command::spawn(&format!(
                    "niri msg action close-window --id {}",
                    item.action
                ));
            }
            ItemKind::Ssh => {
                let terminal = std::env::var("TERMINAL")
                    .ok()
                    .filter(|terminal| !terminal.is_empty())
                    .unwrap_or_else(|| "x-terminal-emulator".to_owned());

                command::spawn(&format!("{terminal} -e ssh {}", quote(&item.action)));
            }
            ItemKind::Command | ItemKind::Menu => {
                command::spawn(&item.action);
            }
            ItemKind::Script => {
                let call = format!("{} {}", menu_command.unwrap_or_default(), quote(&item.name));

                command::spawn(&command::with_retv(&call, "1"));
            }
            ItemKind::File => {
                command::spawn(&format!("xdg-open {}", quote(&item.action)));
            }
            ItemKind::Web => {
                let base = std::env::var("WHELK_WEB_SEARCH").ok();
                let url = launcherdata::web_url(&item.action, base.as_deref());

                command::spawn(&format!("xdg-open {}", quote(&url)));
            }
            ItemKind::Calc => {
                command::spawn(&format!("printf '%s' {} | wl-copy", quote(&item.action)));
            }
            ItemKind::Session => {
                let action = if item.name == "Lock Screen" {
                    current_exe()
                        .map(|executable| format!("{} --lock", quote(&executable)))
                        .unwrap_or_else(|| "whelk-rs --lock".to_owned())
                } else {
                    item.action.clone()
                };

                command::spawn(&action);
            }
        }

        self.hide();
    }

    fn move_selection(&self, delta: i32) {
        let state = self.inner.borrow();
        let count = state.results.len() as i32;

        if count == 0 {
            return;
        }

        let current = state
            .list
            .selected_row()
            .map(|row| row.index())
            .unwrap_or(-1);
        let mut index = current + delta;

        if index < 0 {
            index = count - 1;
        } else if index >= count {
            index = 0;
        }

        if let Some(row) = state.list.row_at_index(index) {
            state.list.select_row(Some(&row));
        }

        drop(state);
        self.scroll_selected_into_view();
    }

    fn complete_selection(&self) {
        let Some(item) = self.selected_item() else {
            return;
        };
        let entry = self.inner.borrow().entry.clone();

        entry.set_text(&item.name);
        entry.set_position(-1);
    }

    fn scroll_selected_into_view(&self) {
        let state = self.inner.borrow();
        let Some(row) = state.list.selected_row() else {
            return;
        };
        let adjustment = state.scroller.vadjustment();

        if !row.is_mapped() {
            adjustment.set_value(adjustment.lower());

            return;
        }

        let Some(bounds) = row.compute_bounds(&state.list) else {
            return;
        };
        let current = adjustment.value();
        let next = launcher_geometry::scroll_value(
            current,
            adjustment.page_size(),
            adjustment.lower(),
            adjustment.upper(),
            f64::from(bounds.y()),
            f64::from(bounds.y() + bounds.height()),
        );

        if (next - current).abs() > 0.5 {
            adjustment.set_value(next);
        }
    }

    fn point_outside(&self, x: f64, y: f64) -> bool {
        let state = self.inner.borrow();
        let Some(bounds) = state.card.compute_bounds(&state.window) else {
            return false;
        };

        launcher_geometry::point_outside(
            x,
            y,
            f64::from(bounds.x()),
            f64::from(bounds.y()),
            f64::from(bounds.x() + bounds.width()),
            f64::from(bounds.y() + bounds.height()),
        )
    }

    fn zoom_icons(&self, delta: i32) {
        {
            let mut state = self.inner.borrow_mut();

            state.icon_size = launcher_geometry::clamp_icon_size(state.icon_size + delta);
        }

        self.apply_icon_size();
        self.update_zoom_labels();
    }

    fn zoom_font(&self, delta: i32) {
        {
            let mut state = self.inner.borrow_mut();

            state.font_scale = launcher_geometry::clamp_font_scale(state.font_scale + delta);
        }

        self.apply_font_scale();
        self.update_zoom_labels();
    }

    fn apply_icon_size(&self) {
        self.refresh_results();
    }

    fn apply_geometry(&self) {
        let state = self.inner.borrow();

        state.card.set_size_request(state.width, state.height);
        state.scroller.set_vexpand(true);
    }

    fn update_zoom_labels(&self) {
        let state = self.inner.borrow();

        state
            .icon_value
            .set_text(&format!("{} px", state.icon_size));
        state.font_value.set_text(&format!("{}%", state.font_scale));
    }

    fn apply_font_scale(&self) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let (widget_id, font_base, percent) = {
            let state = self.inner.borrow();

            (
                style::widget_id("launcher"),
                state.font_base.clone(),
                state.font_scale,
            )
        };
        let size = launcherdata::scaled_font_size(font_base.as_deref(), percent);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!(
            "window#{widget_id} .whelk-launcher {{ font-size: {size}; }}\n"
        ));

        let mut state = self.inner.borrow_mut();

        if let Some(previous) = state.zoom_css.take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        state.zoom_css = Some(css);
    }

    fn install_style(&self, style: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let mut styled = style.clone();

        if styled.radius < 16 {
            styled.radius = 16;
        }

        let widget_id = style::widget_id("launcher");
        let generated = style::css(&widget_id, &styled, "whelk-launcher", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{BASE_CSS}\n{generated}"));

        let mut state = self.inner.borrow_mut();

        if let Some(previous) = state.css.take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        state.css = Some(css);
        state.font_base = styled.font_size.clone();

        drop(state);
        self.apply_font_scale();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(source) = self.query_source.take() {
            source.remove();
        }

        if let Some(css) = self.zoom_css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.destroy();
    }
}

fn resolve_mode(mode: &str) -> &'static str {
    match mode {
        "window" => "window",
        "run" => "run",
        "calc" => "calc",
        "file" => "file",
        "web" => "web",
        "menu" => "menu",
        "ssh" => "ssh",
        "quit" => "quit",
        "clipboard" => "clipboard",
        "menuitem" => "menuitem",
        "session" => "session",
        _ => "app",
    }
}

fn ranked(items: &[LauncherItem], query: &str) -> Vec<LauncherItem> {
    launcherdata::rank_indices(items, query, RESULT_LIMIT)
        .into_iter()
        .map(|index| items[index].clone())
        .collect()
}

fn calc_results(query: &str) -> Vec<LauncherItem> {
    let Some(value) = crate::calc::eval(query) else {
        return Vec::new();
    };
    let formatted = crate::calc::format(value);

    vec![
        LauncherItem::new(
            ItemKind::Calc,
            format!("= {formatted}"),
            "Enter copies the result",
        )
        .with_action(formatted)
        .with_icon(Some("accessories-calculator-symbolic")),
    ]
}

fn web_results(query: &str) -> Vec<LauncherItem> {
    if query.is_empty() {
        return Vec::new();
    }

    vec![
        LauncherItem::new(
            ItemKind::Web,
            format!("Search the web for “{query}”"),
            "Opens the default browser",
        )
        .with_action(query)
        .with_icon(Some("web-browser-symbolic")),
    ]
}

fn unsupported(name: &str, detail: &str) -> LauncherItem {
    let mut item = LauncherItem::new(ItemKind::Command, name, detail)
        .with_icon(Some("dialog-information-symbolic"));

    item.disabled = true;
    item
}

fn build_row(item: &LauncherItem, icon_size: i32) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let name = gtk::Label::new(Some(&item.name));
    let detail = gtk::Label::new(Some(&item.detail));
    let icon = match item.icon.as_deref() {
        Some(icon) if icon.starts_with('/') => gtk::Image::from_file(icon),
        Some(icon) => gtk::Image::from_icon_name(icon),
        None => gtk::Image::from_icon_name("application-x-executable-symbolic"),
    };

    content.add_css_class("whelk-launcher-row");
    name.set_xalign(0.0);
    detail.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.add_css_class("whelk-launcher-name");
    detail.add_css_class("whelk-launcher-detail");
    text.append(&name);
    text.append(&detail);
    text.set_hexpand(true);
    icon.set_pixel_size(icon_size);
    icon.set_valign(gtk::Align::Center);

    let icon_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    icon_box.add_css_class("whelk-launcher-icon");
    icon_box.set_valign(gtk::Align::Center);
    icon_box.append(&icon);
    content.append(&icon_box);
    content.append(&text);
    row.set_child(Some(&content));
    row.set_sensitive(!item.disabled);
    row
}

fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn quote(value: &str) -> String {
    gtk::glib::shell_quote(value).to_string_lossy().into_owned()
}

fn current_exe() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

const BASE_CSS: &str = "
    .whelk-surface {
        background-color: transparent;
    }
    .whelk-launcher {
        padding: 16px 18px 14px 18px;
        border-radius: 16px;
        background-image: linear-gradient(to bottom, alpha(white, 0.06),
                                          alpha(white, 0) 92px);
        box-shadow: 0 24px 70px alpha(black, 0.48), 0 2px 8px alpha(black, 0.20);
    }
    .whelk-launcher-header {
        padding: 0 2px 4px 2px;
    }
    .whelk-launcher-brand {
        padding-top: 1px;
    }
    .whelk-launcher-mark {
        font-size: 0.82em;
        padding-top: 2px;
    }
    .whelk-launcher-title {
        font-size: 1.1em;
        font-weight: 800;
        letter-spacing: -0.02em;
    }
    .whelk-launcher-subtitle {
        opacity: 0.48;
        font-size: 0.78em;
        letter-spacing: 0.08em;
    }
    .whelk-launcher-close {
        min-width: 30px;
        min-height: 30px;
        padding: 4px;
        border-radius: 999px;
        opacity: 0.62;
    }
    .whelk-launcher-close:hover {
        opacity: 1;
        background-color: alpha(currentColor, 0.12);
    }
    .whelk-launcher-entry {
        font-size: 1.18em;
        min-height: 44px;
        padding: 0 12px;
        border-radius: 12px;
        border: 1px solid alpha(currentColor, 0.14);
        background-color: alpha(black, 0.20);
    }
    .whelk-launcher-entry:focus {
        border-color: alpha(currentColor, 0.42);
        box-shadow: 0 0 0 3px alpha(currentColor, 0.08);
    }
    .whelk-launcher-mode {
        opacity: 0.58;
        font-size: 0.78em;
        letter-spacing: 0.02em;
        padding: 1px 3px;
    }
    .whelk-launcher-hint {
        opacity: 0.42;
        font-size: 0.78em;
        padding: 2px 2px 0 2px;
    }
    .whelk-launcher-footer {
        padding: 2px 2px 0 2px;
    }
    .whelk-launcher-zoom-label {
        opacity: 0.48;
        font-size: 0.78em;
    }
    .whelk-launcher-zoom-value {
        opacity: 0.62;
        font-size: 0.78em;
        min-width: 42px;
    }
    .whelk-launcher-zoom {
        min-width: 26px;
        min-height: 26px;
        padding: 2px 7px;
        border-radius: 8px;
        opacity: 0.72;
    }
    .whelk-launcher-zoom:hover {
        opacity: 1;
        background-color: alpha(currentColor, 0.10);
    }
    .whelk-launcher-grip {
        opacity: 0.36;
        padding: 0 2px 0 6px;
        font-size: 0.95em;
    }
    .whelk-launcher-scroller {
        background-color: alpha(black, 0.13);
        border: 1px solid alpha(currentColor, 0.08);
        border-radius: 12px;
        padding: 4px;
    }
    .whelk-launcher-list {
        background: transparent;
        padding: 2px;
    }
    .whelk-launcher-list row {
        padding: 0;
        background: transparent;
    }
    .whelk-launcher-row {
        padding: 8px 10px;
        border-radius: 9px;
    }
    .whelk-launcher-list row:hover .whelk-launcher-row {
        background: alpha(currentColor, 0.07);
    }
    .whelk-launcher-list row:selected .whelk-launcher-row {
        background: alpha(currentColor, 0.15);
        box-shadow: inset 2px 0 alpha(currentColor, 0.85);
    }
    .whelk-launcher-icon {
        min-width: 28px;
        min-height: 28px;
        padding: 4px;
        border-radius: 8px;
        background-color: alpha(currentColor, 0.08);
    }
    .whelk-launcher-empty {
        padding: 54px 24px;
        opacity: 0.52;
        font-size: 0.95em;
    }
    .whelk-launcher-name {
        font-weight: 600;
    }
    .whelk-launcher-detail {
        opacity: 0.55;
        font-size: 0.85em;
    }
";

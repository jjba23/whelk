// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The settings window, ported from `src/preferences.c`.
//!
//! The window edits a focused set of bar options and saves each change through
//! `(whelk prefs)`.  It never writes the primary settings file: what it saves is
//! a state-directory override that wins when the shell reloads, and Reset
//! deletes that file.
//!
//! Two halves live here.  The model — how a typed value is written back to
//! Scheme, how a module's override key is named, which options a text field may
//! edit — needs no display and is tested as such.  The pages below only draw
//! that model and hand every change to the debounced queue.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{BarSettings, Edge, Layer};
use crate::command;
use crate::launcherkeys::LauncherKeys;
use crate::nightlight;
use crate::settings::SchemeValue;
use crate::sources;
use crate::style;
use whelk_launcher::style::LauncherSettings;

/// Typing in a field settles for this long before the value is written, so a
/// slider drag or a sentence does not become one preference write per keystroke.
pub const APPLY_DEBOUNCE: Duration = Duration::from_millis(180);

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// One value a control can save.  The variant is what decides the Scheme the
/// override file receives: a symbol stays a symbol and a number stays a number.
#[derive(Debug, Clone, PartialEq)]
pub enum PreferenceValue {
    Bool(bool),
    Integer(i64),
    Decimal(f64),
    Text(String),
    Symbol(String),
}

impl PreferenceValue {
    pub fn to_scheme(&self) -> String {
        match self {
            Self::Bool(value) => if *value { "#t" } else { "#f" }.to_owned(),
            Self::Integer(value) => value.to_string(),
            Self::Decimal(value) => format!("{value}"),
            Self::Text(value) => crate::guile::scheme_string_literal(value),
            Self::Symbol(value) => format!("'{}", value.trim_start_matches('\'')),
        }
    }
}

/// A module override key: its kind and occurrence, so inserting or moving
/// another kind in the bar cannot move this card's override.
pub fn module_key(kind: &str, occurrence: usize, option: &str) -> String {
    format!("module-{kind}-{occurrence}-{option}")
}

/// The switch that hides a module without removing it from settings.scm.
pub fn module_visible_key(kind: &str, occurrence: usize) -> String {
    module_key(kind, occurrence, "visible?")
}

/// What a module option is, as far as the window can edit it.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionKind {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Symbol(String),
    Text(String),
}

/// One editable option.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleOption {
    pub name: String,
    pub kind: OptionKind,
}

/// One card on the Modules page: a module instance, not a kind, so two custom
/// modules are two cards.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleCard {
    /// `"Left"`, `"Centre"`, `"Right"` or `"Overflow"`.
    pub group: &'static str,
    pub kind: String,
    pub occurrence: usize,
    pub options: Vec<ModuleOption>,
    pub hidden: bool,
    /// An option containing a line break stays in settings.scm: a single text
    /// field would silently join the lines.
    pub skipped_line_breaks: bool,
    /// A list or an exact fraction has no single text spelling that is also
    /// itself, so those options stay in settings.scm too.
    pub skipped_lists: bool,
}

/// Every module on the bar, in the order the Scheme override layer numbers
/// them: left, centre, right, overflow.
pub fn module_cards(bar: &BarSettings) -> Vec<ModuleCard> {
    let groups: [(&'static str, &[crate::bar::ModuleSpec]); 4] = [
        ("Left", &bar.left),
        ("Centre", &bar.center),
        ("Right", &bar.right),
        ("Overflow", &bar.overflow),
    ];
    let mut occurrences: BTreeMap<&str, usize> = BTreeMap::new();
    let mut cards = Vec::new();

    for (label, specs) in groups {
        for spec in specs {
            let occurrence = *occurrences.get(spec.kind.as_str()).unwrap_or(&0);
            let mut card = ModuleCard {
                group: label,
                kind: spec.kind.clone(),
                occurrence,
                options: Vec::new(),
                hidden: matches!(spec.options.get("hidden?"), Some(SchemeValue::Bool(true))),
                skipped_line_breaks: false,
                skipped_lists: false,
            };

            occurrences.insert(spec.kind.as_str(), occurrence + 1);

            for (name, value) in &spec.options {
                if name == "hidden?" || (spec.kind == "scheme" && name == "update") {
                    continue;
                }

                if matches!(value, SchemeValue::Text(text) if text.contains('\n')) {
                    card.skipped_line_breaks = true;
                    continue;
                }

                let Some(kind) = option_kind(value) else {
                    card.skipped_lists = true;
                    continue;
                };

                card.options.push(ModuleOption {
                    name: name.clone(),
                    kind,
                });
            }

            card.options
                .sort_by(|left, right| left.name.cmp(&right.name));
            cards.push(card);
        }
    }

    cards
}

fn option_kind(value: &SchemeValue) -> Option<OptionKind> {
    match value {
        SchemeValue::Bool(value) => Some(OptionKind::Bool(*value)),
        SchemeValue::Integer(value) => Some(OptionKind::Integer(*value)),
        SchemeValue::Number(value) => Some(OptionKind::Number(*value)),
        SchemeValue::Symbol(value) => Some(OptionKind::Symbol(value.clone())),
        SchemeValue::Text(value) => Some(OptionKind::Text(value.clone())),
        SchemeValue::List(_) | SchemeValue::Alist(_) | SchemeValue::Other => None,
    }
}

/// Minutes from midnight as the field shows them.
pub fn clock_text(minutes: i64) -> String {
    let minutes = minutes.rem_euclid(24 * 60);

    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// `"HH:MM"` as minutes from midnight.  A field still being typed (`"2"`,
/// `"20:"`) is not a value yet, and neither is an out-of-range time.
pub fn parse_clock_text(text: &str) -> Option<i64> {
    let (hours, minutes) = text.trim().split_once(':')?;

    if hours.is_empty() || hours.len() > 2 || minutes.len() != 2 {
        return None;
    }

    let hours: i64 = hours.parse().ok()?;
    let minutes: i64 = minutes.parse().ok()?;

    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

/// The words a launcher mode wears on the card.
pub fn launcher_mode_label(mode: &str) -> &str {
    match mode {
        "app" => "Applications",
        "window" => "Windows",
        "run" => "Commands",
        "calc" => "Calculator",
        "file" => "Files",
        "web" => "The web",
        "ssh" => "SSH hosts",
        "quit" => "Close windows",
        "clipboard" => "Clipboard",
        "menuitem" => "Menu items",
        other => other,
    }
}

/// A colour as the text the override stores, through GTK's own parser and
/// formatter so the saved value round-trips.  An unset or unparsable colour is
/// black, which is what the C control opens the dialog on.
pub fn color_text(value: Option<&str>) -> String {
    let fallback = gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0);
    let rgba = value
        .and_then(|value| gtk::gdk::RGBA::parse(value).ok())
        .unwrap_or(fallback);

    rgba.to_string()
}

fn edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Top => "top",
        Edge::Bottom => "bottom",
        Edge::Left => "left",
        Edge::Right => "right",
    }
}

fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Top => "top",
        Layer::Bottom => "bottom",
        Layer::Background => "background",
        Layer::Overlay => "overlay",
    }
}

// ---------------------------------------------------------------------------
// The window's state
// ---------------------------------------------------------------------------

/// What the window draws and what it needs to save a change.  The shell owns
/// the Guile side, so persistence and reload arrive as callbacks.
pub type SaveFn = Rc<dyn Fn(&str, &PreferenceValue) -> bool>;
pub type ReadFn = Rc<dyn Fn(&str) -> Option<String>>;
pub type ReloadFn = Rc<dyn Fn() -> Snapshot>;

pub struct Callbacks {
    pub save: SaveFn,
    pub reset: Rc<dyn Fn() -> bool>,
    /// One saved preference's text, for the fields whose value is not on the
    /// bar (the launcher's default mode and its prefixes).
    pub read: ReadFn,
    /// Reload the shell and answer with what it now reads.
    pub reload: ReloadFn,
}

/// The values a (re)build reads.  A reload replaces all of them, so Reset can
/// show the configuration as it is.
#[derive(Clone)]
pub struct Snapshot {
    pub bar: BarSettings,
    pub launcher: LauncherSettings,
    pub night_light: nightlight::Config,
    pub themes: Vec<String>,
}

impl Snapshot {
    /// What a window can still draw when the shell is already gone: the
    /// built-in bar and the filter's defaults.
    pub fn fallback() -> Self {
        Self {
            bar: BarSettings::default(),
            launcher: LauncherSettings::default(),
            night_light: nightlight::Config::default(),
            themes: Vec::new(),
        }
    }
}

struct State {
    config_path: PathBuf,
    snapshot: RefCell<Snapshot>,
    pending: RefCell<BTreeMap<String, PreferenceValue>>,
    apply: RefCell<Option<gtk::glib::SourceId>>,
    loading: Cell<bool>,
    callbacks: Rc<Callbacks>,
}

impl State {
    fn queue(self: &Rc<Self>, key: &str, value: PreferenceValue) {
        if self.loading.get() {
            return;
        }

        self.pending.borrow_mut().insert(key.to_owned(), value);

        if self.apply.borrow().is_some() {
            return;
        }

        let state = Rc::clone(self);
        let source = gtk::glib::timeout_add_local_once(APPLY_DEBOUNCE, move || {
            state.apply();
        });

        *self.apply.borrow_mut() = Some(source);
    }

    fn apply(&self) {
        *self.apply.borrow_mut() = None;

        let changes = std::mem::take(&mut *self.pending.borrow_mut());
        let mut wrote = false;

        for (key, value) in &changes {
            wrote |= (self.callbacks.save)(key, value);
        }

        if wrote {
            (self.callbacks.reload)();
        }
    }

    fn drop_pending(&self) {
        if let Some(source) = self.apply.borrow_mut().take() {
            sources::remove(source);
        }

        self.pending.borrow_mut().clear();
    }
}

// ---------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------

/// One settings window.  It is created hidden and rebuilt from a snapshot each
/// time it opens, so a settings save or a reload is never left on screen as a
/// stale number.
pub struct Preferences {
    window: gtk::ApplicationWindow,
    host: gtk::Box,
    css: RefCell<Option<gtk::CssProvider>>,
    state: Rc<State>,
}

const WINDOW_CSS: &str = "
    .settings-header {
        padding: 18px 20px 6px;
    }
    .settings-heading {
        font-size: 20px;
        font-weight: 800;
    }
    .settings-badge {
        opacity: 0.45;
        font-size: 10px;
        letter-spacing: 1px;
    }
    .settings-subtitle {
        opacity: 0.7;
        font-size: 12px;
    }
    .settings-page {
        padding: 8px 20px 20px;
    }
    .settings-card {
        padding: 14px;
        border-radius: 12px;
        background-color: alpha(currentColor, 0.05);
    }
    .settings-card-heading {
        margin-bottom: 10px;
    }
    .settings-card-title {
        font-weight: 700;
    }
    .settings-card-subtitle {
        opacity: 0.65;
        font-size: 12px;
    }
    .settings-row {
        padding: 6px 0;
    }
    .settings-row-title {
        font-size: 13px;
    }
    .settings-row-subtitle {
        opacity: 0.6;
        font-size: 11px;
    }
    .settings-note {
        opacity: 0.6;
        font-size: 11px;
    }
    .settings-footer {
        padding: 12px 20px 18px;
    }
    .settings-footer-note {
        opacity: 0.55;
        font-size: 11px;
    }
    entry.error {
        color: #ff5f56;
    }
";

impl Preferences {
    pub fn new(
        application: &gtk::Application,
        config_path: PathBuf,
        snapshot: Snapshot,
        callbacks: Rc<Callbacks>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk settings")
            .build();
        let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let state = Rc::new(State {
            config_path,
            snapshot: RefCell::new(snapshot),
            pending: RefCell::new(BTreeMap::new()),
            apply: RefCell::new(None),
            loading: Cell::new(false),
            callbacks,
        });

        window.add_css_class("whelk-settings-window");
        window.set_default_size(760, 760);
        window.set_child(Some(&host));

        {
            let target = window.clone();
            let keys = gtk::EventControllerKey::new();

            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape {
                    target.set_visible(false);

                    return gtk::glib::Propagation::Stop;
                }

                gtk::glib::Propagation::Proceed
            });
            window.add_controller(keys);
        }

        let preferences = Self {
            window,
            host,
            css: RefCell::new(None),
            state,
        };

        preferences.set_skin();
        preferences.rebuild();
        preferences.window.set_visible(false);

        preferences
    }

    /// The bar's palette, so the window reads as part of the same shell.
    fn set_skin(&self) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let mut bar = self.state.snapshot.borrow().bar.clone();

        if bar.radius < 12 {
            bar.radius = 12;
        }

        let generated = style::css("whelk-settings", &bar, "settings-page", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!(
            "{WINDOW_CSS}\nwindow.whelk-settings-window {{ background-color: rgba(24, 24, 28, 0.96); }}\n{generated}"
        ));
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        if let Some(previous) = self.css.borrow_mut().replace(css) {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Present the pages as they were last built.
    pub fn present(&self) {
        self.window.set_visible(true);
        self.window.present();
    }

    /// Draw the window from `snapshot` and present it.
    pub fn show(&self, snapshot: Snapshot) {
        *self.state.snapshot.borrow_mut() = snapshot;
        self.rebuild();
        self.present();
    }

    pub fn toggle(&self, snapshot: Snapshot) {
        if self.is_visible() {
            self.hide();
        } else {
            self.show(snapshot);
        }
    }

    fn rebuild(&self) {
        while let Some(child) = self.host.first_child() {
            self.host.remove(&child);
        }

        self.state.loading.set(true);
        build_ui(&self.state, &self.host);
        self.state.loading.set(false);
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        self.state.drop_pending();

        if let Some(css) = self.css.borrow_mut().take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

// ---------------------------------------------------------------------------
// Cards and rows
// ---------------------------------------------------------------------------

fn text_label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));

    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);

    if !class.is_empty() {
        label.add_css_class(class);
    }

    label
}

fn card_new(title: &str, subtitle: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 2);

    card.add_css_class("settings-card");
    heading.add_css_class("settings-card-heading");
    heading.append(&text_label(title, "settings-card-title"));

    if !subtitle.is_empty() {
        heading.append(&text_label(subtitle, "settings-card-subtitle"));
    }

    card.append(&heading);

    card
}

fn row_add(card: &gtk::Box, title: &str, subtitle: Option<&str>, control: Option<&gtk::Widget>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);

    row.add_css_class("settings-row");
    text.set_hexpand(true);
    text.append(&text_label(title, "settings-row-title"));

    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.is_empty()) {
        text.append(&text_label(subtitle, "settings-row-subtitle"));
    }

    row.append(&text);

    if let Some(control) = control {
        control.set_valign(gtk::Align::Center);
        row.append(control);
    }

    card.append(&row);
}

fn card_note(card: &gtk::Box, text: &str) {
    let note = text_label(text, "settings-note");

    note.set_margin_top(10);
    card.append(&note);
}

fn row_switch(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: bool,
    module: Option<(&str, &str)>,
) {
    let control = gtk::Switch::new();
    let mut available = true;

    if let Some((kind, label)) = module {
        available = module_available(card, state, kind, label);
    }

    control.set_active(value);
    control.set_sensitive(available);

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_active_notify(move |control| {
        state.queue(&key, PreferenceValue::Bool(control.is_active()));
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

fn module_available(card: &gtk::Box, state: &Rc<State>, kind: &str, label: &str) -> bool {
    if [
        &state.snapshot.borrow().bar.left,
        &state.snapshot.borrow().bar.center,
        &state.snapshot.borrow().bar.right,
    ]
    .into_iter()
    .flatten()
    .any(|spec| spec.kind == kind)
    {
        return true;
    }

    card_note(
        card,
        &format!(
            "This bar has no {label} module; the setting will apply once one is added to the \
             configuration."
        ),
    );

    false
}

fn row_spin(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: i64,
    (minimum, maximum): (i64, i64),
) -> gtk::SpinButton {
    let control = gtk::SpinButton::with_range(minimum as f64, maximum as f64, 1.0);

    control.set_value(value as f64);
    control.set_numeric(true);
    control.set_width_chars(5);

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_value_changed(move |control| {
        state.queue(
            &key,
            PreferenceValue::Integer(i64::from(control.value_as_int())),
        );
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));

    control
}

fn row_double(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: f64,
    (minimum, maximum, step): (f64, f64, f64),
) {
    let control = gtk::SpinButton::with_range(minimum, maximum, step);

    control.set_value(value);
    control.set_digits(3);
    control.set_numeric(true);
    control.set_width_chars(7);

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_value_changed(move |control| {
        state.queue(&key, PreferenceValue::Decimal(control.value()));
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

#[allow(clippy::too_many_arguments)]
fn row_scale(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: f64,
    (minimum, maximum, step): (f64, f64, f64),
    digits: i32,
) {
    let control = gtk::Scale::with_range(gtk::Orientation::Horizontal, minimum, maximum, step);

    control.set_value(value);
    control.set_digits(digits);
    control.set_value_pos(gtk::PositionType::Right);
    control.set_size_request(220, -1);

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_value_changed(move |control| {
        state.queue(&key, PreferenceValue::Decimal(control.value()));
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

fn row_color(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: Option<&str>,
) {
    let dialog = gtk::ColorDialog::new();
    let control = gtk::ColorDialogButton::new(Some(dialog));
    let rgba = value
        .and_then(|value| gtk::gdk::RGBA::parse(value).ok())
        .unwrap_or_else(|| gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));

    control.set_rgba(&rgba);
    control.set_tooltip_text(Some("Click to choose a colour"));

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_rgba_notify(move |control| {
        state.queue(&key, PreferenceValue::Text(control.rgba().to_string()));
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

#[derive(Clone, Copy)]
enum EntryKind {
    Text,
    Symbol,
}

fn row_entry(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    value: &str,
    kind: EntryKind,
) -> gtk::Entry {
    let control = gtk::Entry::new();

    control.set_text(value);
    control.set_width_chars(18);
    control.set_size_request(200, -1);

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_changed(move |control| {
        let text = control.text().to_string();
        let value = match kind {
            EntryKind::Text => PreferenceValue::Text(text),
            EntryKind::Symbol => PreferenceValue::Symbol(text),
        };

        state.queue(&key, value);
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));

    control
}

fn row_clock(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    minutes: i64,
) {
    let control = gtk::Entry::new();

    control.set_text(&clock_text(minutes));
    control.set_width_chars(5);
    control.set_max_width_chars(5);
    control.set_tooltip_text(Some("HH:MM, 24-hour clock"));

    let state = Rc::clone(state);
    let key = key.to_owned();

    control.connect_changed(move |control| match parse_clock_text(&control.text()) {
        Some(minutes) => {
            control.remove_css_class("error");
            state.queue(&key, PreferenceValue::Integer(minutes));
        }
        None => control.add_css_class("error"),
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

#[allow(clippy::too_many_arguments)]
fn row_dropdown(
    state: &Rc<State>,
    card: &gtk::Box,
    title: &str,
    subtitle: Option<&str>,
    key: &str,
    labels: &[&str],
    values: &[&str],
    current: &str,
) {
    let control = gtk::DropDown::from_strings(labels);
    let selected = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);

    control.set_selected(selected as u32);

    let state = Rc::clone(state);
    let key = key.to_owned();
    let values: Vec<String> = values.iter().map(|value| (*value).to_owned()).collect();

    control.connect_selected_notify(move |control| {
        if let Some(value) = values.get(control.selected() as usize) {
            state.queue(&key, PreferenceValue::Symbol(value.clone()));
        }
    });
    row_add(card, title, subtitle, Some(control.upcast_ref()));
}

fn page(stack: &gtk::Stack, name: &str, title: &str) -> gtk::Box {
    let scroller = gtk::ScrolledWindow::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);

    body.add_css_class("settings-page");
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&body));
    stack.add_titled(&scroller, Some(name), title);

    body
}

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

fn build_ui(state: &Rc<State>, host: &gtk::Box) {
    let stack = gtk::Stack::new();
    let switcher = gtk::StackSwitcher::new();

    stack.add_css_class("settings-stack");
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(160);
    switcher.set_stack(Some(&stack));
    switcher.set_halign(gtk::Align::Start);

    build_bar_page(state, &stack);
    build_theme_page(state, &stack);
    build_night_light_page(state, &stack);
    build_notifications_page(state, &stack);
    build_wifi_page(state, &stack);
    build_menu_page(state, &stack);
    build_modules_page(state, &stack);
    build_launcher_page(state, &stack);
    build_more_page(state, &stack);

    host.append(&header());
    host.append(&switcher);
    host.append(&stack);
    host.append(&footer(state, host));
    stack.set_vexpand(true);
}

fn header() -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let badge = text_label("GUI OVERRIDES", "settings-badge");

    header.add_css_class("settings-header");
    title_row.add_css_class("settings-title-row");
    title_row.append(&text_label("whelk settings", "settings-heading"));
    badge.set_halign(gtk::Align::End);
    badge.set_hexpand(true);
    title_row.append(&badge);
    header.append(&title_row);
    header.append(&text_label(
        "Shape the bar without editing your configuration. Changes are saved separately and win \
         when the bar reloads; settings.scm is never rewritten.",
        "settings-subtitle",
    ));

    header
}

fn footer(state: &Rc<State>, host: &gtk::Box) -> gtk::Box {
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let note = text_label(
        "Reset returns to settings.scm exactly as it is.",
        "settings-footer-note",
    );
    let reset = gtk::Button::with_label("Reset GUI settings");
    let close = gtk::Button::with_label("Close");

    footer.add_css_class("settings-footer");
    note.set_hexpand(true);
    footer.append(&note);
    reset.add_css_class("destructive-action");
    close.add_css_class("suggested-action");

    {
        let state = Rc::clone(state);
        let host = host.clone();

        reset.connect_clicked(move |_| {
            state.drop_pending();

            if !(state.callbacks.reset)() {
                eprintln!("whelk: could not reset the GUI preferences");
            }

            let snapshot = (state.callbacks.reload)();

            *state.snapshot.borrow_mut() = snapshot;

            while let Some(child) = host.first_child() {
                host.remove(&child);
            }

            state.loading.set(true);
            build_ui(&state, &host);
            state.loading.set(false);
        });
    }

    {
        let host = host.clone();

        close.connect_clicked(move |_| {
            if let Some(window) = host.root().and_downcast::<gtk::Window>() {
                window.set_visible(false);
            }
        });
    }

    footer.append(&reset);
    footer.append(&close);

    footer
}

fn build_bar_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "bar", "Bar");
    let placement = card_new(
        "Placement",
        "Where the bar sits and how much room it takes.",
    );
    let bar = state.snapshot.borrow().bar.clone();

    body.append(&placement);
    row_dropdown(
        state,
        &placement,
        "Edge",
        Some("The screen edge the bar belongs to."),
        "bar-edge",
        &["Top", "Bottom", "Left", "Right"],
        &["top", "bottom", "left", "right"],
        edge_name(bar.edge),
    );
    row_dropdown(
        state,
        &placement,
        "Layer",
        Some("Top stays over the overview; bottom moves with a workspace."),
        "bar-layer",
        &["Top layer", "Bottom layer", "Background", "Overlay"],
        &["top", "bottom", "background", "overlay"],
        layer_name(bar.layer),
    );
    row_switch(
        state,
        &placement,
        "Reserve space",
        Some("Keep windows out of the bar's strip of the screen."),
        "bar-exclusive?",
        bar.exclusive,
        None,
    );
    row_spin(
        state,
        &placement,
        "Height",
        Some("Thickness in pixels."),
        "bar-height",
        i64::from(bar.height),
        (20, 128),
    );
    row_spin(
        state,
        &placement,
        "Module spacing",
        Some("Gap between items."),
        "bar-spacing",
        i64::from(bar.spacing),
        (0, 64),
    );
    row_spin(
        state,
        &placement,
        "Padding",
        Some("Space at the bar's two ends."),
        "bar-padding",
        i64::from(bar.padding),
        (0, 96),
    );

    let typography = card_new(
        "Typography",
        "The text face used by the bar's readings and menus.",
    );

    body.append(&typography);
    row_entry(
        state,
        &typography,
        "Font family",
        Some("For example Inter or Cantarell."),
        "bar-font",
        bar.font.as_deref().unwrap_or("Inter"),
        EntryKind::Text,
    );
    row_entry(
        state,
        &typography,
        "Font size",
        Some("A CSS/Pango size, for example 11pt."),
        "bar-font-size",
        bar.font_size.as_deref().unwrap_or("11pt"),
        EntryKind::Text,
    );
}

fn build_theme_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "theme", "Theme");
    let snapshot = state.snapshot.borrow().clone();
    let bar = snapshot.bar.clone();
    let material = card_new(
        "Material",
        "The palette, the light on the surface and how much of the desktop shows through it.",
    );
    let mut themes: Vec<String> = snapshot.themes.clone();

    if themes.is_empty() {
        themes.push("gloss-2007".to_owned());
    }

    let current = bar.theme.clone().unwrap_or_default();

    if !current.is_empty() && !themes.contains(&current) {
        themes.push(current.clone());
    }

    body.append(&material);

    {
        let labels: Vec<&str> = themes.iter().map(String::as_str).collect();

        row_dropdown(
            state,
            &material,
            "Theme",
            Some("The named material the bar is made from."),
            "bar-theme",
            &labels,
            &labels,
            &current,
        );
    }

    card_note(
        &material,
        "The theme supplies the starting colours and material numbers; every control below layers \
         on top of it.",
    );

    let colours = card_new(
        "Colours",
        "The bar's ink, surface, accent and hairline.  A theme fills these in until one is chosen \
         here.",
    );

    body.append(&colours);
    row_color(
        state,
        &colours,
        "Surface",
        Some("The material's background colour; alpha is kept."),
        "bar-background",
        bar.background.as_deref(),
    );
    row_color(
        state,
        &colours,
        "Ink",
        Some("Text and glyphs."),
        "bar-foreground",
        bar.foreground.as_deref(),
    );
    row_color(
        state,
        &colours,
        "Accent",
        Some("Active workspaces and highlights."),
        "bar-accent",
        bar.accent.as_deref(),
    );
    row_color(
        state,
        &colours,
        "Hairline",
        Some("The one-pixel border."),
        "bar-border-color",
        bar.border_color.as_deref(),
    );

    let light = card_new(
        "Light and depth",
        "The generated surface: transparency, shadow, top shine, lower reflection and the accent \
         flare.",
    );

    body.append(&light);
    row_scale(
        state,
        &light,
        "Opacity",
        Some("Multiplies the surface colour's own alpha."),
        "bar-opacity",
        bar.opacity,
        (0.2, 1.0, 0.01),
        2,
    );
    row_scale(
        state,
        &light,
        "Shadow blur",
        Some("Softness of the shadow under the surface, in pixels."),
        "bar-blur",
        f64::from(bar.blur),
        (0.0, 64.0, 1.0),
        0,
    );
    row_scale(
        state,
        &light,
        "Shine",
        Some("Strength of the light falling on the top edge."),
        "bar-shine",
        bar.shine,
        (0.0, 1.0, 0.01),
        2,
    );
    row_scale(
        state,
        &light,
        "Reflection",
        Some("Strength of the colour reflected up from the lower edge."),
        "bar-reflection",
        bar.reflection,
        (0.0, 1.0, 0.01),
        2,
    );
    row_scale(
        state,
        &light,
        "Flare",
        Some("Strength of the accent glow along the top edge."),
        "bar-flare",
        bar.flare,
        (0.0, 1.0, 0.01),
        2,
    );
    row_spin(
        state,
        &light,
        "Corner radius",
        Some("The surface's corner radius in pixels."),
        "bar-radius",
        i64::from(bar.radius),
        (0, 48),
    );

    let about = card_new(
        "How overrides work",
        "A GUI choice is saved separately from your configuration.",
    );

    body.append(&about);
    card_note(
        &about,
        "Changes are written under your XDG state directory in a preferences file. Your \
         settings.scm is never edited. Reset removes the overrides and returns to the \
         configuration exactly as it is.",
    );
}

fn build_night_light_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "night-light", "Night light");
    let light = state.snapshot.borrow().night_light.clone();
    let filter = card_new(
        "A warmer white point",
        "The display server changes the gamma table itself, so every window, video and the bar \
         are warmed together.  Lower kelvin is warmer.",
    );

    body.append(&filter);
    row_switch(
        state,
        &filter,
        "Night light",
        Some("Filter the screen for easier reading after dark."),
        "night-light-enabled?",
        light.enabled,
        None,
    );

    let temperature = row_spin(
        state,
        &filter,
        "Temperature",
        Some("The white point in kelvin; 6500 K is neutral."),
        "night-light-temperature",
        i64::from(light.temperature),
        (
            i64::from(nightlight::MIN_TEMPERATURE),
            i64::from(nightlight::MAX_TEMPERATURE),
        ),
    );

    // A slider-sized step is what a person actually adjusts; typing still
    // reaches any single kelvin.
    temperature.set_increments(100.0, 500.0);
    row_dropdown(
        state,
        &filter,
        "Mode",
        Some("Follow the clock below, or stay warm all day."),
        "night-light-mode",
        &["Scheduled", "Always on"],
        &["schedule", "always"],
        match light.mode {
            nightlight::Mode::Always => "always",
            nightlight::Mode::Scheduled => "schedule",
        },
    );

    let when = card_new(
        "When",
        "The window the filter follows and the fade at each end.",
    );

    body.append(&when);
    row_clock(
        state,
        &when,
        "Starts at",
        Some("The fade in begins at this time.  The window may cross midnight."),
        "night-light-start",
        i64::from(light.start_minute),
    );
    row_clock(
        state,
        &when,
        "Ends at",
        Some("The fade out finishes at this time."),
        "night-light-end",
        i64::from(light.end_minute),
    );
    row_spin(
        state,
        &when,
        "Transition",
        Some("Minutes spent fading at each end of the window."),
        "night-light-transition",
        i64::from(light.transition_minutes),
        (0, 240),
    );
    card_note(
        &when,
        "A window whose start and end are the same minute means all day.",
    );
}

fn build_notifications_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "notifications", "Notifications");
    let bell = card_new(
        "The bar's bell",
        "How the notification service appears on the bar.",
    );
    let show_count = state
        .snapshot
        .borrow()
        .bar
        .module("notifications")
        .and_then(|spec| spec.text("format"))
        .is_some_and(|format| format.contains("{count}"));

    body.append(&bell);
    row_switch(
        state,
        &bell,
        "Show the notification count",
        Some("Draw the number of waiting notifications beside the bell."),
        "notifications-show-count?",
        show_count,
        Some(("notifications", "notification")),
    );
    card_note(
        &bell,
        "Do Not Disturb and the notification centre are available from the bar's bell and the \
         shell menu; this page controls the reading at rest.",
    );
}

fn build_wifi_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "wifi", "Wi-Fi");
    let wifi = card_new(
        "Network reading",
        "The Wi-Fi indicator's name and refresh rate.",
    );
    let bar = state.snapshot.borrow().bar.clone();
    let spec = bar.module("wifi");
    let show_ssid = spec
        .as_ref()
        .and_then(|spec| spec.boolean("show-ssid?"))
        .unwrap_or(false);
    let interval = spec
        .as_ref()
        .and_then(|spec| spec.integer("interval"))
        .unwrap_or(5);

    body.append(&wifi);
    row_switch(
        state,
        &wifi,
        "Show the network name",
        Some("Replace the bare Wi-Fi glyph with the SSID you are joined to."),
        "wifi-show-ssid?",
        show_ssid,
        Some(("wifi", "Wi-Fi")),
    );
    row_spin(
        state,
        &wifi,
        "Refresh interval",
        Some("Seconds between scans; a longer interval costs less power."),
        "wifi-interval",
        interval,
        (1, 300),
    );
    card_note(
        &wifi,
        "The network panel, connect and disconnect are reached by clicking the Wi-Fi reading or \
         from the shell menu.",
    );
}

fn build_menu_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "menu", "Global menu");
    let menu = card_new(
        "The application menu",
        "The File and Edit menus that follow the focused window on the bar.",
    );
    let bar = state.snapshot.borrow().bar.clone();
    let spec = bar.module("menu");
    let shortcuts = spec
        .as_ref()
        .and_then(|spec| spec.boolean("shortcuts?"))
        .unwrap_or(true);
    let app_name = spec
        .as_ref()
        .and_then(|spec| spec.boolean("app-name?"))
        .unwrap_or(true);
    let style = spec
        .as_ref()
        .and_then(|spec| spec.text("accelerator-style"))
        .unwrap_or_else(|| "cua".to_owned());

    body.append(&menu);
    row_switch(
        state,
        &menu,
        "Show shortcut menus",
        Some("Show each application's own menus beside the shell menu."),
        "menu-shortcuts?",
        shortcuts,
        Some(("menu", "global menu")),
    );
    row_switch(
        state,
        &menu,
        "Show the application name",
        Some("Put the focused window's name before its menus."),
        "menu-app-name?",
        app_name,
        Some(("menu", "global menu")),
    );
    row_dropdown(
        state,
        &menu,
        "Shortcut style",
        Some("How accelerators are written in the menus."),
        "menu-accelerator-style",
        &["CUA (Ctrl+C)", "Emacs (C-c)"],
        &["cua", "emacs"],
        &style,
    );
    card_note(
        &menu,
        "The menu reads the application's exported menu over D-Bus. An application without one \
         gets whelk's shared Edit and View menus.",
    );
}

fn build_modules_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "modules", "Modules");
    let bar = state.snapshot.borrow().bar.clone();
    let cards = module_cards(&bar);
    let mut skipped_line_breaks = false;
    let mut skipped_lists = false;

    body.append(&text_label(
        "Every option this bar's modules were given is editable here.  A module's key is its kind \
         and occurrence, such as module-clock-0-format for the first clock, so reordering the bar \
         does not move its settings.  Symbols stay symbols and numbers stay numbers when the \
         window writes them back.  A change is an override, and Reset deletes all of them.",
        "settings-note",
    ));

    for card in &cards {
        let module = card_new(
            &card.kind,
            &format!("{}, {} {}", card.group, card.kind, card.occurrence + 1),
        );

        body.append(&module);
        row_switch(
            state,
            &module,
            "Show in the bar",
            Some("Turn this module off without removing it from settings.scm."),
            &module_visible_key(&card.kind, card.occurrence),
            !card.hidden,
            None,
        );

        for option in &card.options {
            let key = module_key(&card.kind, card.occurrence, &option.name);

            match &option.kind {
                OptionKind::Bool(value) => {
                    row_switch(state, &module, &option.name, None, &key, *value, None);
                }
                OptionKind::Integer(value) => {
                    row_spin(
                        state,
                        &module,
                        &option.name,
                        None,
                        &key,
                        *value,
                        (0, 1_000_000),
                    );
                }
                OptionKind::Number(value) => {
                    row_double(
                        state,
                        &module,
                        &option.name,
                        None,
                        &key,
                        *value,
                        (-1_000_000.0, 1_000_000.0, 0.1),
                    );
                }
                OptionKind::Symbol(value) => {
                    row_entry(
                        state,
                        &module,
                        &option.name,
                        None,
                        &key,
                        value,
                        EntryKind::Symbol,
                    );
                }
                OptionKind::Text(value) => {
                    row_entry(
                        state,
                        &module,
                        &option.name,
                        None,
                        &key,
                        value,
                        EntryKind::Text,
                    );
                }
            }
        }

        skipped_line_breaks |= card.skipped_line_breaks;
        skipped_lists |= card.skipped_lists;
    }

    if skipped_line_breaks {
        body.append(&text_label(
            "Options containing line breaks, such as a tooltip, stay in settings.scm so their \
             shape is not flattened.",
            "settings-note",
        ));
    }

    if skipped_lists {
        body.append(&text_label(
            "List-valued options and exact fractions stay in settings.scm: a single text field \
             cannot write back their Scheme shape.",
            "settings-note",
        ));
    }
}

fn build_launcher_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "keys", "Launcher");
    let launcher = state.snapshot.borrow().launcher;
    let appearance = card_new(
        "Window and zoom",
        "The card's starting size and how large its icons and text are.",
    );

    body.append(&appearance);
    row_spin(
        state,
        &appearance,
        "Width",
        Some("Pixels."),
        "launcher-width",
        i64::from(launcher.width),
        (360, 1600),
    );
    row_spin(
        state,
        &appearance,
        "Height",
        Some("Pixels."),
        "launcher-height",
        i64::from(launcher.height),
        (300, 1200),
    );
    row_spin(
        state,
        &appearance,
        "Icon size",
        Some("Pixels per launcher icon."),
        "launcher-icon-size",
        i64::from(launcher.icon_size),
        (16, 64),
    );
    row_spin(
        state,
        &appearance,
        "Text size",
        Some("Percent of the bar's configured font size."),
        "launcher-font-scale",
        i64::from(launcher.font_scale),
        (75, 200),
    );
    card_note(
        &appearance,
        "These are GUI overrides.  A bar's launcher-width, launcher-height, launcher-icon-size \
         and launcher-font-scale keywords are the defaults, and Reset restores them.",
    );

    let start = card_new(
        "When the launcher opens",
        "The mode a bare `whelk --launcher' starts in.",
    );
    let stored = (state.callbacks.read)("launcher-default-mode");
    let selected = stored
        .filter(|mode| LauncherKeys::is_mode(mode))
        .unwrap_or_else(|| "app".to_owned());
    let modes: Vec<&str> = crate::launcherkeys::MODES.to_vec();
    let labels: Vec<&str> = modes.iter().map(|mode| launcher_mode_label(mode)).collect();

    body.append(&start);
    row_dropdown(
        state,
        &start,
        "Default mode",
        None,
        "launcher-default-mode",
        &labels,
        &modes,
        &selected,
    );

    let keys = card_new(
        "Mode keys",
        "Type one of these at the start of the query to choose a mode; leave a field empty to \
         disable that key.",
    );
    let defaults = LauncherKeys::new();

    body.append(&keys);

    for mode in crate::launcherkeys::MODES {
        // `app' is the mode with no key of its own: it is what is left when no
        // prefix is typed.
        if mode == "app" {
            continue;
        }

        let key = format!("launcher-key-{mode}");
        let prefix =
            (state.callbacks.read)(&key).unwrap_or_else(|| defaults.prefix(mode).to_owned());
        let subtitle = format!("Search {}.", launcher_mode_label(mode));
        let entry = row_entry(
            state,
            &keys,
            mode,
            Some(&subtitle),
            &key,
            &prefix,
            EntryKind::Text,
        );

        entry.set_max_width_chars(4);
    }

    card_note(
        &keys,
        "The binding that opens the launcher belongs to your compositor.  Everything above is \
         whelk's own and is saved as an override.",
    );
}

fn build_more_page(state: &Rc<State>, stack: &gtk::Stack) {
    let body = page(stack, "more", "More");
    let bar = state.snapshot.borrow().bar.clone();
    let tray = card_new("Tray", "The status icons hosted at the end of the bar.");
    let max_items = bar
        .module("tray")
        .and_then(|spec| spec.integer("max-items"))
        .unwrap_or(6);

    body.append(&tray);
    row_spin(
        state,
        &tray,
        "Maximum items",
        Some("Icons shown before the overflow chevron takes over."),
        "tray-max-items",
        max_items,
        (1, 32),
    );

    let clock = card_new("Clock", "The time reading in the bar.");
    let format = bar
        .module("clock")
        .and_then(|spec| spec.text("format"))
        .unwrap_or_else(|| "%a %d %b  %H:%M".to_owned());

    body.append(&clock);
    row_entry(
        state,
        &clock,
        "Format",
        Some("A strftime(3) format."),
        "clock-format",
        &format,
        EntryKind::Text,
    );
    card_note(
        &clock,
        "The clock's click and right-click actions, calendar tooltip and alternate format stay in \
         settings.scm.",
    );

    let escape = card_new(
        "The rest of the file",
        "Desktop widgets, clipboard retention, compositor keybindings and anything else with no \
         card in this window.",
    );
    let open = gtk::Button::with_label("Open settings.scm at line 1");

    body.append(&escape);
    open.set_halign(gtk::Align::Start);

    {
        let config_path = state.config_path.clone();

        open.connect_clicked(move |_| open_settings_file(&config_path));
    }

    escape.append(&open);
    card_note(
        &escape,
        "Your $VISUAL or $EDITOR, at the first line.  The window edits only what it can undo; the \
         rest is a line in a file you own.",
    );
}

/// The escape hatch: everything the window cannot express is still one line in
/// a file the user owns.  `$VISUAL` and `$EDITOR` come first, and `+1` is the
/// spelling vi, nano and emacs all understand; `xdg-open` is the fallback,
/// where the line number is simply ignored.
fn open_settings_file(config_path: &Path) {
    let editor = ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "xdg-open".to_owned());

    command::spawn(&format!(
        "{editor} +1 {}",
        shell_quote(&config_path.to_string_lossy())
    ));
}

/// `g_shell_quote` for the one path a command line carries.
fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./=:+@%".contains(character))
    {
        return text.to_owned();
    }

    format!("'{}'", text.replace('\'', "'\\''"))
}

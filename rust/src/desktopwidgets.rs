// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The desktop widget cards, ported from `src/widgets.c`.
//!
//! Each card is a bottom-layer surface at its own place on its own monitor,
//! moved by the grip in its header, and refreshed by the cadence its kind
//! needs.  Everything the cards *say* lives in [`crate::widgets`]; this module
//! only paints it and keeps the surfaces alive.
//!
//! The C manager kept a `WidgetGuard` so an asynchronous answer could tell
//! whether the card that asked still existed.  Here the same question is a
//! weak reference: every timer and every answer holds `Weak` handles to the
//! widgets it would update, so a card that is gone drops its reading on the
//! floor without a liveness token and without a single `unsafe` block.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::batterybar;
use crate::command;
use crate::sources;
use crate::style;
use crate::ui::layer_shell_available;
use crate::widgets::{
    self, Area, FALLBACK_AREA, ReminderRow, Reminders, StockQuote, WidgetConfig, WidgetKind,
};
use crate::widgetstate;

/// The structural rules every card wears.  The palette, radius and material
/// come from the generated skin below; these are the pieces that make a card
/// readable in any theme.
const CARD_CSS: &str = "
    .whelk-widget-card {
        padding: 14px;
    }
    .whelk-widget-header {
        margin-bottom: 2px;
    }
    .whelk-widget-glyph {
        opacity: 0.85;
        font-size: 15px;
    }
    .whelk-widget-title {
        font-weight: 700;
        letter-spacing: 0.3px;
    }
    .whelk-widget-grip {
        opacity: 0.35;
        padding: 0 2px;
    }
    .whelk-widget-card.dragging .whelk-widget-header {
        opacity: 0.9;
    }
    .whelk-widget-subtle {
        opacity: 0.68;
        font-size: 12px;
    }
    .whelk-widget-clock-time {
        font-size: 30px;
        font-weight: 700;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-clock-time.digital {
        font-size: 44px;
        letter-spacing: 1px;
    }
    .whelk-widget-clock-face {
        margin: 4px 0 2px;
    }
    .whelk-widget-calendar-nav {
        margin-top: 2px;
    }
    .whelk-widget-calendar-month {
        font-weight: 700;
    }
    .whelk-widget-weekday {
        opacity: 0.55;
        font-size: 11px;
        font-weight: 700;
    }
    .whelk-widget-day {
        font-size: 13px;
    }
    .whelk-widget-day.today {
        font-weight: 700;
        background-color: alpha(currentColor, 0.18);
        border-radius: 6px;
    }
    .whelk-widget-weather-icon {
        font-size: 34px;
    }
    .whelk-widget-temperature {
        font-size: 30px;
        font-weight: 700;
    }
    .whelk-widget-reading {
        font-size: 14px;
    }
    .whelk-widget-battery-percent {
        font-size: 30px;
        font-weight: 700;
    }
    .whelk-widget-note {
        font-size: 13px;
        margin-top: 2px;
    }
    .whelk-widget-note.empty {
        opacity: 0.5;
        font-style: italic;
    }
    .whelk-widget-reminder {
        font-size: 13px;
    }
    .whelk-widget-reminder-list {
        margin-top: 2px;
    }
    .whelk-widget-reminder-row {
        padding: 3px 6px;
        border-radius: 7px;
        min-height: 26px;
    }
    .whelk-widget-reminder-row:hover {
        background-color: alpha(currentColor, 0.07);
    }
    .whelk-widget-reminder-mark {
        opacity: 0.45;
    }
    .whelk-widget-reminder-done {
        opacity: 1;
    }
    .whelk-widget-reminder.done {
        opacity: 0.5;
    }
    .whelk-widget-progress {
        font-size: 11px;
        opacity: 0.5;
    }
    .whelk-widget-city {
        font-size: 14px;
    }
    .whelk-widget-world-time {
        font-size: 15px;
        font-weight: 700;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-stock-row {
        padding: 3px 0;
    }
    .whelk-widget-stock-symbol {
        font-weight: 700;
        font-size: 13px;
    }
    .whelk-widget-stock-price {
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-stock-change {
        font-size: 12px;
        font-weight: 700;
    }
    .whelk-widget-stock-change.up {
        color: #4ade80;
    }
    .whelk-widget-stock-change.down {
        color: #f87171;
    }
    .whelk-widget-footnote {
        opacity: 0.55;
        font-size: 11px;
    }
    .whelk-widget-card.warning {
        border-color: #ffb340;
    }
    .whelk-widget-card.critical {
        border-color: #ff5f56;
    }
";

// ---------------------------------------------------------------------------
// Policy that needs no display
// ---------------------------------------------------------------------------

/// One monitor's connector and the area a card may be dragged across on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorArea {
    pub connector: String,
    pub area: Area,
}

/// The area a card belongs to: the connector the settings file names, else the
/// one its surface is on, else the display's first monitor, else
/// [`FALLBACK_AREA`] for a display with no monitor model.
pub fn area_for(configured: &str, at_surface: Option<&str>, monitors: &[MonitorArea]) -> Area {
    let named = (!configured.trim().is_empty())
        .then_some(configured)
        .filter(|connector| *connector != "default");

    for wanted in [named, at_surface].into_iter().flatten() {
        if let Some(monitor) = monitors.iter().find(|monitor| monitor.connector == wanted) {
            return monitor.area;
        }
    }

    monitors
        .first()
        .map(|monitor| monitor.area)
        .unwrap_or(FALLBACK_AREA)
}

/// Where the three hands of an analog clock point, in radians from three
/// o'clock clockwise, at `hour:minute:second`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockAngles {
    pub hour: f64,
    pub minute: f64,
    pub second: f64,
}

/// The clock face's hand angles.  The hour and minute hands creep with the
/// smaller unit, exactly as the C face draws them.
pub fn clock_angles(hour: u32, minute: u32, second: u32) -> ClockAngles {
    let hour_angle = (f64::from(hour % 12) + f64::from(minute) / 60.0) * std::f64::consts::PI / 6.0
        - std::f64::consts::FRAC_PI_2;
    let minute_angle = (f64::from(minute) + f64::from(second) / 60.0) * std::f64::consts::PI / 30.0
        - std::f64::consts::FRAC_PI_2;
    let second_angle =
        f64::from(second) * std::f64::consts::PI / 30.0 - std::f64::consts::FRAC_PI_2;

    ClockAngles {
        hour: hour_angle,
        minute: minute_angle,
        second: second_angle,
    }
}

/// One battery card reading: the two numbers it draws and the card classes
/// that colour it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatterySummary {
    pub percent: String,
    pub status: String,
    pub classes: Vec<&'static str>,
}

/// The battery card's words and classes.  A critical pack is critical even
/// while charging, so the order matches the C card: threshold first, then the
/// charging mark.
pub fn battery_summary(capacity: i32, status: &str, warning: i32, critical: i32) -> BatterySummary {
    let charging = plugged_in(status);
    let mut classes = Vec::new();

    if capacity <= critical {
        classes.push("critical");
    } else if capacity <= warning {
        classes.push("warning");
    }

    if charging {
        classes.push("charging");
    }

    BatterySummary {
        percent: format!("{capacity}%"),
        status: format!(
            "{status} · {}",
            if charging { "charging" } else { "on battery" }
        ),
        classes,
    }
}

/// Whether a pack counts as on power on the widget card.  The card follows the
/// C widget's three statuses rather than the bar module's stricter rule, so a
/// full pack and one that stopped taking power still read as plugged in.
fn plugged_in(status: &str) -> bool {
    matches!(status, "Charging" | "Full" | "Not charging")
}

/// The markup one reminder row is drawn with: its text escaped, struck
/// through when it is done.
pub fn reminder_markup(row: &ReminderRow) -> String {
    let mut escaped = String::with_capacity(row.text.len());

    for character in row.text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&apos;"),
            '"' => escaped.push_str("&quot;"),
            character => escaped.push(character),
        }
    }

    if row.done {
        format!("<s>{escaped}</s>")
    } else {
        escaped
    }
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// Every desktop widget the current configuration asks for.  Dropping a card
/// removes its timer, its skin and its surface.
pub struct Widgets {
    cards: Vec<Card>,
    state_path: Option<PathBuf>,
}

impl Widgets {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            state_path: None,
        }
    }

    pub fn count(&self) -> usize {
        self.cards.len()
    }

    /// Replace every card with the ones `configs` asks for.  An unknown kind is
    /// reported and skipped, as the C manager does.
    pub fn build(
        &mut self,
        application: &gtk::Application,
        configs: &[WidgetConfig],
        style: &BarSettings,
        config_path: &Path,
    ) {
        self.clear();
        self.state_path = widgetstate::state_path(config_path);

        for (index, config) in configs.iter().enumerate() {
            if config.kind().is_none() {
                eprintln!("whelk: unknown desktop widget kind '{}'", config.spec.kind);
                continue;
            }

            if let Some(card) = Card::present(
                application,
                config,
                index,
                style,
                self.state_path.as_deref(),
            ) {
                self.cards.push(card);
            }
        }

        if !self.cards.is_empty() {
            eprintln!("whelk: {} desktop widget(s) up", self.cards.len());
        }
    }

    pub fn clear(&mut self) {
        self.cards.clear();
    }
}

impl Default for Widgets {
    fn default() -> Self {
        Self::new()
    }
}

/// One presented card.  Its content, timer and drag state are owned by the
/// widgets inside it; this struct is what a rebuild drops.
struct Card {
    window: gtk::ApplicationWindow,
    css: Option<gtk::CssProvider>,
    timers: Vec<gtk::glib::SourceId>,
}

impl Card {
    fn present(
        application: &gtk::Application,
        config: &WidgetConfig,
        index: usize,
        style: &BarSettings,
        state_path: Option<&Path>,
    ) -> Option<Self> {
        let id = config.id(index);
        let widget_id = style::widget_id(&id);
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk widget")
            .build();

        window.set_widget_name(&widget_id);
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(false);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 9);

        card.add_css_class("whelk-widget-card");
        card.add_css_class(&config.object_class());
        card.set_size_request(config.width, config.height);

        if let Some(tooltip) = config.tooltip() {
            card.set_tooltip_text(Some(&tooltip));
        }

        attach_clicks(&card, config);

        append_header(&card, config);

        // A dragged position wins over the settings file, and it is clamped
        // again here because the monitor may have been resized or replaced
        // since it was saved.
        let connector = configured_connector(config);
        let saved = state_path
            .and_then(|path| widgetstate::load_position_for(path, &id, connector.as_deref()));
        let position = config.restore_position(saved, current_area(None, &config.monitor));

        let timers = build_content(&card, config, style, state_path, &id);

        // The drag gesture reads the card back off its window, so the tree is
        // in place before the controller is attached.
        window.set_child(Some(&card));

        if let Some(grip) = grip(&card) {
            attach_drag(
                &grip,
                &window,
                config,
                &id,
                state_path,
                Rc::new(Cell::new(position)),
            );
        }

        crate::a11y::describe(&card, &config.spec.kind, &config.title(), None, false);

        let skin = install_skin(&widget_id, style, config);

        if layer_shell_available() {
            window.init_layer_shell();
            window.set_layer(Layer::Bottom);
            window.set_namespace(Some("whelk-widget"));
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_exclusive_zone(-1);

            if let Some(monitor) = monitor_for(&config.monitor) {
                window.set_monitor(Some(&monitor));
            }

            set_position(&window, position.0, position.1);
        } else {
            // Broadway and a plain X11 test display have no layer shell; a
            // normal window is still a useful smoke test for the tree.
            window.set_default_size(config.width, config.height);
        }

        window.present();

        Some(Self {
            window,
            css: skin,
            timers,
        })
    }
}

impl Drop for Card {
    fn drop(&mut self) {
        for timer in self.timers.drain(..) {
            sources::remove(timer);
        }

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

// ---------------------------------------------------------------------------
// Surface plumbing
// ---------------------------------------------------------------------------

fn label(text: &str, class: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));

    // `xalign` puts the text where the card wants it *inside* the label, so the
    // label itself fills whatever width it is given.  Aligning the widget
    // instead (`halign` = Start for a left-aligned label) handed a wrapping
    // card only its natural width, which is how a note ended up wrapping five
    // characters to a line inside a card half a screen wide.
    label.set_xalign(xalign);
    label.set_halign(gtk::Align::Fill);

    if !class.is_empty() {
        label.add_css_class(class);
    }

    label
}

fn append_header(card: &gtk::Box, config: &WidgetConfig) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let fill = label("", "whelk-widget-header-fill", 0.0);
    let glyph = config.kind().map(WidgetKind::glyph).unwrap_or("◌");

    row.add_css_class("whelk-widget-header");
    row.append(&label(glyph, "whelk-widget-glyph", 0.0));
    row.append(&label(&config.title(), "whelk-widget-title", 0.0));
    fill.set_hexpand(true);
    row.append(&fill);
    card.append(&row);
}

/// The drag handle inside the header.  Cards are moved the way a window is:
/// by the small piece that says so, so their own buttons keep their clicks.
fn grip(card: &gtk::Box) -> Option<gtk::Label> {
    let header = card.first_child()?;
    let grip = label("⠿", "whelk-widget-grip", 1.0);

    grip.set_cursor_from_name(Some("grab"));
    grip.set_tooltip_text(Some("Drag to move this widget"));
    header.downcast::<gtk::Box>().ok()?.append(&grip);

    Some(grip)
}

/// The mouse vocabulary a bar module also has, from the same option names.
fn attach_clicks(card: &gtk::Box, config: &WidgetConfig) {
    let mut scroll_up = None;
    let mut scroll_down = None;

    for (key, command_line) in config.click_commands() {
        match key {
            "on-click" => click(card, gtk::gdk::BUTTON_PRIMARY, command_line),
            "on-click-middle" => click(card, gtk::gdk::BUTTON_MIDDLE, command_line),
            "on-click-right" => click(card, gtk::gdk::BUTTON_SECONDARY, command_line),
            "on-scroll-up" => scroll_up = Some(command_line),
            "on-scroll-down" => scroll_down = Some(command_line),
            _ => {}
        }
    }

    if scroll_up.is_none() && scroll_down.is_none() {
        return;
    }

    match (scroll_up, scroll_down) {
        (Some(up), Some(down)) => scroll(card, Some(up), Some(down)),
        (Some(up), None) => scroll(card, Some(up), None),
        (None, Some(down)) => scroll(card, None, Some(down)),
        (None, None) => {}
    }
}

fn click(card: &gtk::Box, button: u32, command_line: String) {
    let gesture = gtk::GestureClick::new();

    gesture.set_button(button);
    gesture.connect_pressed(move |_, _, _, _| command::spawn(&command_line));
    card.add_controller(gesture);
}

fn scroll(card: &gtk::Box, up: Option<String>, down: Option<String>) {
    let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);

    controller.connect_scroll(move |_, _, dy| {
        if dy < 0.0
            && let Some(command_line) = up.as_deref()
        {
            command::spawn(command_line);

            return gtk::glib::Propagation::Stop;
        }

        if dy > 0.0
            && let Some(command_line) = down.as_deref()
        {
            command::spawn(command_line);

            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });
    card.add_controller(controller);
}

fn attach_drag(
    grip: &gtk::Label,
    window: &gtk::ApplicationWindow,
    config: &WidgetConfig,
    id: &str,
    state_path: Option<&Path>,
    position: Rc<Cell<(i32, i32)>>,
) {
    let drag = gtk::GestureDrag::new();
    let origin = Rc::new(Cell::new(position.get()));
    let begin_position = Rc::clone(&position);

    drag.set_button(gtk::gdk::BUTTON_PRIMARY);

    {
        let Some(card) = window.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let handle = grip.clone();
        let origin = Rc::clone(&origin);

        drag.connect_drag_begin(move |_, _, _| {
            origin.set(begin_position.get());
            card.add_css_class("dragging");
            handle.set_cursor_from_name(Some("grabbing"));
        });
    }

    {
        let window = window.clone();
        let moving = Rc::clone(&position);
        let monitor = config.monitor.clone();
        let size = (config.width, config.height);
        let origin = Rc::clone(&origin);

        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let area = current_area(Some(&window), &monitor);
            let (x, y) = widgets::drag_position(origin.get(), (offset_x, offset_y), size, area);

            set_position(&window, x, y);
            moving.set((x, y));
        });
    }

    {
        let Some(card) = window.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let handle = grip.clone();
        let window = window.clone();
        let monitor = config.monitor.clone();
        let anchor = config.anchor;
        let size = (config.width, config.height);
        let id = id.to_owned();
        let state_path = state_path.map(Path::to_path_buf);
        let landing = Rc::clone(&position);

        drag.connect_drag_end(move |_, _, _| {
            card.remove_css_class("dragging");
            handle.set_cursor_from_name(Some("grab"));

            if let Some(path) = state_path.as_deref() {
                let connector = monitor_key(&window, &monitor);
                let area = current_area(Some(&window), &monitor);
                // The state file stores the card's distance from its anchor,
                // so a card pinned to a corner keeps that corner when the
                // output is resized.
                let offset = widgets::anchored_offset(anchor, area, landing.get(), size);

                widgetstate::save_position_for(path, &id, connector.as_deref(), offset.0, offset.1);
            }
        });
    }

    grip.add_controller(drag);
}

fn set_position(window: &gtk::ApplicationWindow, x: i32, y: i32) {
    if !layer_shell_available() {
        return;
    }

    window.set_margin(Edge::Top, y);
    window.set_margin(Edge::Left, x);
}

fn monitor_for(connector: &str) -> Option<gtk::gdk::Monitor> {
    if connector.is_empty() || connector == "default" {
        return None;
    }

    let display = gtk::gdk::Display::default()?;
    let model = display.monitors();

    for index in 0..model.n_items() {
        let monitor = model.item(index).and_downcast::<gtk::gdk::Monitor>()?;
        let name = monitor
            .connector()
            .map(|name| name.to_string())
            .unwrap_or_default();

        if name == connector {
            return Some(monitor);
        }
    }

    None
}

/// The connector a placement is keyed under: the settings file's monitor, or
/// the one the card's surface is actually on.
fn monitor_key(window: &gtk::ApplicationWindow, configured: &str) -> Option<String> {
    let connector = window
        .surface()
        .and_then(|surface| gtk::gdk::Display::default()?.monitor_at_surface(&surface))
        .and_then(|monitor| monitor.connector())
        .map(|name| name.to_string());

    widgets::monitor_key(configured, connector.as_deref()).map(str::to_owned)
}

fn configured_connector(config: &WidgetConfig) -> Option<String> {
    widgets::monitor_key(&config.monitor, None).map(str::to_owned)
}

/// The area a card may be dragged across right now.
fn current_area(window: Option<&gtk::ApplicationWindow>, configured: &str) -> Area {
    let Some(display) = gtk::gdk::Display::default() else {
        return FALLBACK_AREA;
    };
    let model = display.monitors();
    let mut monitors = Vec::new();

    for index in 0..model.n_items() {
        let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let geometry = monitor.geometry();

        monitors.push(MonitorArea {
            connector: monitor
                .connector()
                .map(|name| name.to_string())
                .unwrap_or_default(),
            area: Area {
                width: geometry.width(),
                height: geometry.height(),
            },
        });
    }

    let at_surface = window
        .and_then(|window| window.surface())
        .and_then(|surface| display.monitor_at_surface(&surface))
        .and_then(|monitor| monitor.connector())
        .map(|name| name.to_string());

    area_for(configured, at_surface.as_deref(), &monitors)
}

fn install_skin(
    widget_id: &str,
    style: &BarSettings,
    config: &WidgetConfig,
) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    // A card is skinned from the first bar's palette so the desktop looks like
    // one shell; a card that named its own material changes only what it named.
    let mut skin = style.clone();

    if let Some(material) = config.skin() {
        material.apply(&mut skin);
    }

    let generated = style::css(
        widget_id,
        &skin,
        "whelk-widget-card",
        config.css().as_deref(),
    );
    let css = gtk::CssProvider::new();

    css.load_from_string(&format!("{CARD_CSS}\n{generated}"));
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(css)
}

// ---------------------------------------------------------------------------
// The update cadence
// ---------------------------------------------------------------------------

/// Run `update` every `interval` seconds; it answers `false` once the card it
/// paints is gone, which removes the timer.
fn repeat(interval: u32, update: Rc<dyn Fn() -> bool>) -> gtk::glib::SourceId {
    gtk::glib::timeout_add_seconds_local(interval, move || {
        if update() {
            gtk::glib::ControlFlow::Continue
        } else {
            gtk::glib::ControlFlow::Break
        }
    })
}

/// Paint once now, then on the card's cadence when it has one.
fn schedule(interval: u32, update: Rc<dyn Fn() -> bool>) -> Option<gtk::glib::SourceId> {
    if !update() || interval == 0 {
        return None;
    }

    Some(repeat(interval, update))
}

/// The name the cards introduce themselves with.  Yahoo answers 429 to a
/// request that carries no `User-Agent` at all and 200 to the same URL with
/// any one; Open-Meteo does not care either way.
const USER_AGENT: &str = concat!("whelk/", env!("CARGO_PKG_VERSION"));

/// How often a Weather card asks Open-Meteo and a Stocks card asks Yahoo.
/// Each card's quiet age line closes in once its reading is older than twice
/// its own interval, so the two are kept beside the numbers they explain.
const WEATHER_INTERVAL: u32 = 1800;
const STOCKS_INTERVAL: u32 = 300;

/// How often a card re-reads its own age.  Short enough that "x min ago" is
/// current, slow enough that it is not a clock.
const FRESHNESS_TICK: u32 = 30;

/// Why a card has no reading.
///
/// The two failures need different words.  A refused or timed-out request may
/// answer on the next tick, but a missing `curl` never will, and a card that
/// says "unavailable" for both leaves the user with nothing to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchError {
    /// `curl` is not on `PATH`; nothing about the network was tried.
    NoCurl,
    /// `curl` ran and answered nothing usable: refused, timed out, or empty.
    Failed,
}

/// What a card says when it has no reading: a headline, and the line under it.
///
/// The headline names `curl` when `curl` is the reason, because that is the one
/// failure the user has to fix; `what` only appears in the ordinary case, so a
/// Weather card reads "Weather unavailable" and a Stocks footnote "Quotes
/// unavailable", as they always have.
pub fn failure_lines(error: FetchError, what: &str) -> (String, &'static str) {
    match error {
        FetchError::NoCurl => (
            "curl is not installed".to_owned(),
            "install curl, or put it on PATH",
        ),
        FetchError::Failed => (format!("{what} unavailable"), ""),
    }
}

// ---------------------------------------------------------------------------
// How old a reading is
// ---------------------------------------------------------------------------

/// The card clock: seconds since the first card asked for the time.
///
/// Only differences between two readings of it are ever compared, so a change
/// to the wall clock cannot make a reading look older or fresher than it is.
pub fn now_seconds() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs()
}

/// A reading's age in whole minutes, and never "0 min ago".
///
/// A card only asks for the age once the reading is old enough to talk about,
/// so the coarseness is the point: the line is a note under the reading, not a
/// second hand.
pub fn age_text(seconds: u64) -> String {
    format!("{} min ago", (seconds / 60).max(1))
}

/// How a card's last reading stands, as one value the networked cards share.
///
/// The three states are the three questions a user has about a card that has
/// stopped moving: is this now, when was it, and why did it stop?  The reading
/// keeps its own timestamp rather than an age, because an age stored at the
/// moment of failure would freeze: Weather retries every half hour, so a line
/// written then would still say "1 h ago" four hours later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// The last read answered; `at` is when, on the card clock.
    Ready { at: u64 },
    /// The last read failed and the reading still on the card was taken at
    /// `since`; `error` is what the failed refresh said.
    Stale { since: u64, error: FetchError },
    /// Nothing has ever been read; `error` is why, `at` when it last failed.
    Failed { error: FetchError, at: u64 },
}

impl Health {
    /// The state after a read that answered `answer` at `now`.
    ///
    /// A success replaces everything, because the error it would otherwise keep
    /// no longer explains anything on the card.  A failure keeps the last good
    /// reading and only remembers why the refresh did not replace it.
    pub fn after(previous: Option<Self>, answer: Result<(), FetchError>, now: u64) -> Self {
        match answer {
            Ok(()) => Self::Ready { at: now },
            Err(error) => match previous {
                Some(Self::Ready { at }) | Some(Self::Stale { since: at, .. }) => {
                    Self::Stale { since: at, error }
                }
                Some(Self::Failed { .. }) | None => Self::Failed { error, at: now },
            },
        }
    }

    /// How old the reading on the card is, in seconds; `None` when nothing has
    /// ever been read.
    pub fn age(&self, now: u64) -> Option<u64> {
        match self {
            Self::Ready { at } | Self::Stale { since: at, .. } => Some(now.saturating_sub(*at)),
            Self::Failed { .. } => None,
        }
    }

    /// The quiet line under a reading older than two intervals: "x min ago".
    ///
    /// Two intervals is late enough that an ordinary refresh never shows it --
    /// one missed tick is not worth a sentence -- and early enough that a card
    /// which really has stopped says so while its numbers are still worth
    /// believing.
    pub fn quiet_age(&self, now: u64, interval: u64) -> Option<String> {
        let age = self.age(now)?;

        (age > interval.saturating_mul(2)).then(|| age_text(age))
    }

    /// The hover title: the error that stopped the last refresh, and for a card
    /// that has never loaded, how long ago that last try was.  A card with a
    /// good reading has nothing to report.
    pub fn tooltip(&self, what: &str, now: u64) -> Option<String> {
        let (error, failed_at) = match self {
            Self::Ready { .. } => return None,
            Self::Stale { error, .. } => (*error, None),
            Self::Failed { error, at } => (*error, Some(*at)),
        };
        let (headline, hint) = failure_lines(error, what);
        let mut lines = vec![headline];

        if let Some(at) = failed_at {
            lines.push(format!("failed {}", age_text(now.saturating_sub(at))));
        }

        if !hint.is_empty() {
            lines.push(hint.to_owned());
        }

        Some(lines.join("\n"))
    }
}

/// The one line a session with no `curl` gets, so a card that never loads has
/// an explanation in the journal and not only in its own pixels.
fn warn_missing_curl() {
    static WARNED: std::sync::Once = std::sync::Once::new();

    WARNED.call_once(|| {
        eprintln!(
            "whelk: weather and stock cards read the network with curl, which is not on PATH"
        );
    });
}

/// One asynchronous answer, dropped when the card that asked is gone.
///
/// The request is `curl`'s, not GIO's: `gio::File::for_uri` reaches the network
/// through GVfs, and a shell started by a compositor commonly has no GVfs
/// daemon or HTTP backend at all -- which is why a Weather and a Stocks card
/// sat on "unavailable" on a machine where `curl` fetched the same URL in a
/// second.  `command::run` keeps the child off the main loop and delivers its
/// answer there, so the card still never blocks.
fn fetch(
    url: String,
    busy: Rc<Cell<bool>>,
    done: impl FnOnce(Result<String, FetchError>) + 'static,
) {
    if busy.replace(true) {
        return;
    }

    // A missing `curl` is not a network failure: asking the shell for it first
    // means the card can say what is wrong instead of retrying a program that
    // is not there, every tick, forever.
    if crate::session::program_in_path("curl").is_none() {
        busy.set(false);
        warn_missing_curl();
        done(Err(FetchError::NoCurl));

        return;
    }

    let command_line = format!(
        "curl --fail --silent --show-error --location --max-time 20 --user-agent {} {}",
        gtk::glib::shell_quote(USER_AGENT).to_string_lossy(),
        gtk::glib::shell_quote(url.as_str()).to_string_lossy()
    );

    command::run(&command_line, move |answer| {
        busy.set(false);

        // `curl --fail` reports a refusal as a failure, so an error page is not
        // parsed as a reading; an empty answer is not one either.
        done(match answer {
            Some(text) if !text.trim().is_empty() => Ok(text),
            _ => Err(FetchError::Failed),
        });
    });
}

// ---------------------------------------------------------------------------
// The kinds
// ---------------------------------------------------------------------------

fn build_content(
    card: &gtk::Box,
    config: &WidgetConfig,
    style: &BarSettings,
    state_path: Option<&Path>,
    id: &str,
) -> Vec<gtk::glib::SourceId> {
    let Some(kind) = config.kind() else {
        return Vec::new();
    };
    let timer = match kind {
        // The two networked cards carry a second timer that only re-reads
        // their age, so they answer with every source they own.
        WidgetKind::Weather => return build_weather(card, config),
        WidgetKind::Stocks => return build_stocks(card, config),
        WidgetKind::Clock => build_clock(card, config, style),
        WidgetKind::Calendar => build_calendar(card),
        WidgetKind::Battery => build_battery(card, config),
        WidgetKind::Notes => build_notes(card, config),
        WidgetKind::Reminders => {
            build_reminders(card, config, state_path, id);

            None
        }
        WidgetKind::WorldClock => build_world_clock(card, config),
        WidgetKind::Custom => build_custom(card, config),
    };

    timer.into_iter().collect()
}

/// Draw what a card's [`Health`] says: the quiet age under a reading that has
/// gone old, and the error that stopped the last refresh in the hover title.
///
/// The title falls back to the card's configured tooltip when there is no
/// error, so a card that is working still says whatever the settings file told
/// it to say.
fn paint_health(
    card: &gtk::Box,
    freshness: &gtk::Label,
    health: Option<Health>,
    interval: u64,
    what: &str,
    configured_tooltip: Option<&str>,
) {
    let now = now_seconds();

    match health.and_then(|value| value.quiet_age(now, interval)) {
        Some(text) => {
            freshness.set_text(&text);
            freshness.set_visible(true);
        }
        None => {
            freshness.set_text("");
            freshness.set_visible(false);
        }
    }

    let tooltip = health.and_then(|value| value.tooltip(what, now));

    card.set_tooltip_text(tooltip.as_deref().or(configured_tooltip));
}

fn ink(style: &BarSettings) -> gtk::gdk::RGBA {
    style
        .foreground
        .as_deref()
        .filter(|value| !value.is_empty())
        .and_then(|value| gtk::gdk::RGBA::parse(value).ok())
        .unwrap_or_else(|| gtk::gdk::RGBA::new(0.92, 0.92, 0.94, 1.0))
}

fn build_clock(
    card: &gtk::Box,
    config: &WidgetConfig,
    style: &BarSettings,
) -> Option<gtk::glib::SourceId> {
    let analog = config.spec.text("style").as_deref() != Some("digital");
    let seconds = config.spec.boolean("show-seconds?").unwrap_or(true);
    let colour = ink(style);

    let face = analog.then(|| {
        let face = gtk::DrawingArea::new();

        face.add_css_class("whelk-widget-clock-face");
        face.set_hexpand(true);
        face.set_vexpand(true);
        face.set_size_request(120, 120);
        face.set_content_width(170);
        face.set_content_height(170);
        face.set_draw_func(move |_, cairo, width, height| {
            draw_clock_face(cairo, width, height, colour, seconds);
        });
        card.append(&face);

        face
    });

    let time = label("--:--", "whelk-widget-clock-time", 0.5);

    if !analog {
        time.add_css_class("digital");
    }

    let date = label("", "whelk-widget-subtle", 0.5);

    card.append(&time);
    card.append(&date);

    let weak_time = time.downgrade();
    let weak_date = date.downgrade();
    let weak_face = face.as_ref().map(gtk::DrawingArea::downgrade);

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(time), Some(date)) = (weak_time.upgrade(), weak_date.upgrade()) else {
            return false;
        };
        let now = gtk::glib::DateTime::now_local().ok();
        let time_text = now
            .as_ref()
            .and_then(|now| now.format("%H:%M").ok())
            .map(|text| text.to_string())
            .unwrap_or_else(|| "--:--".to_owned());
        let date_text = now
            .as_ref()
            .and_then(|now| now.format("%A, %d %B").ok())
            .map(|text| text.to_string())
            .unwrap_or_default();

        time.set_text(&time_text);
        date.set_text(&date_text);

        if let Some(face) = weak_face.as_ref().and_then(|face| face.upgrade()) {
            face.queue_draw();
        }

        true
    });

    schedule(1, update)
}

fn draw_clock_face(
    cairo: &gtk::cairo::Context,
    width: i32,
    height: i32,
    colour: gtk::gdk::RGBA,
    seconds: bool,
) {
    let Some(now) = gtk::glib::DateTime::now_local().ok() else {
        return;
    };

    let cx = f64::from(width) / 2.0;
    let cy = f64::from(height) / 2.0;
    let radius = (f64::from(width.min(height)) / 2.0 - 7.0).max(1.0);
    let (red, green, blue) = (
        f64::from(colour.red()),
        f64::from(colour.green()),
        f64::from(colour.blue()),
    );

    cairo.set_source_rgba(red, green, blue, 0.82);
    cairo.set_line_width(1.2);
    cairo.arc(cx, cy, radius, 0.0, 2.0 * std::f64::consts::PI);
    let _ = cairo.stroke();

    for tick in 0..60 {
        let angle = f64::from(tick) * std::f64::consts::PI / 30.0 - std::f64::consts::FRAC_PI_2;
        let inner = radius - if tick % 5 == 0 { 10.0 } else { 5.0 };
        let outer = radius - 3.0;

        cairo.set_line_width(if tick % 5 == 0 { 2.0 } else { 0.8 });
        cairo.move_to(cx + angle.cos() * inner, cy + angle.sin() * inner);
        cairo.line_to(cx + angle.cos() * outer, cy + angle.sin() * outer);
        let _ = cairo.stroke();
    }

    let angles = clock_angles(now.hour() as u32, now.minute() as u32, now.second() as u32);

    cairo.set_line_cap(gtk::cairo::LineCap::Round);
    cairo.set_line_width(5.0);
    cairo.move_to(cx, cy);
    cairo.line_to(
        cx + angles.hour.cos() * radius * 0.48,
        cy + angles.hour.sin() * radius * 0.48,
    );
    let _ = cairo.stroke();

    cairo.set_line_width(3.0);
    cairo.move_to(cx, cy);
    cairo.line_to(
        cx + angles.minute.cos() * radius * 0.70,
        cy + angles.minute.sin() * radius * 0.70,
    );
    let _ = cairo.stroke();

    if seconds {
        cairo.set_source_rgba(red, green, blue, 0.55);
        cairo.set_line_width(1.2);
        cairo.move_to(cx, cy);
        cairo.line_to(
            cx + angles.second.cos() * radius * 0.82,
            cy + angles.second.sin() * radius * 0.82,
        );
        let _ = cairo.stroke();
    }

    cairo.set_source_rgba(red, green, blue, 0.92);
    cairo.arc(cx, cy, 3.0, 0.0, 2.0 * std::f64::consts::PI);
    let _ = cairo.fill();
}

fn build_calendar(card: &gtk::Box) -> Option<gtk::glib::SourceId> {
    let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let shown = Rc::new(Cell::new(today_month()));
    let previous = gtk::Button::with_label("‹");
    let month = gtk::Button::with_label("");
    let next = gtk::Button::with_label("›");
    let grid = gtk::Grid::new();

    navigation.add_css_class("whelk-widget-calendar-nav");

    for (button, tooltip) in [
        (&previous, "The previous month"),
        (&month, "Back to today"),
        (&next, "The next month"),
    ] {
        button.add_css_class("flat");
        button.add_css_class("whelk-widget-calendar-button");
        button.set_focusable(false);
        button.set_tooltip_text(Some(tooltip));
    }

    month.add_css_class("whelk-widget-calendar-month");
    month.set_hexpand(true);
    navigation.append(&previous);
    navigation.append(&month);
    navigation.append(&next);
    card.append(&navigation);

    grid.add_css_class("whelk-widget-calendar-grid");
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(2);
    grid.set_vexpand(true);
    card.append(&grid);

    let weak_grid = grid.downgrade();
    let weak_month = month.downgrade();
    let drawn = Rc::clone(&shown);

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(grid), Some(month)) = (weak_grid.upgrade(), weak_month.upgrade()) else {
            return false;
        };

        render_month(&grid, &month, drawn.get());

        true
    });

    for (button, shift) in [(&previous, -1), (&next, 1)] {
        let shown = Rc::clone(&shown);
        let redraw = Rc::clone(&update);

        button.connect_clicked(move |_| {
            shown.set(shift_month(shown.get(), shift));
            redraw();
        });
    }

    {
        let shown = Rc::clone(&shown);
        let redraw = Rc::clone(&update);

        month.connect_clicked(move |_| {
            shown.set(today_month());
            redraw();
        });
    }

    schedule(60, update)
}

/// The current month, as `(year, month)`.
fn today_month() -> (i32, u32) {
    gtk::glib::DateTime::now_local()
        .ok()
        .map(|now| (now.year(), now.month() as u32))
        .unwrap_or((1970, 1))
}

fn shift_month(month: (i32, u32), by: i32) -> (i32, u32) {
    let index = month.0 * 12 + month.1 as i32 - 1 + by;

    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

fn render_month(grid: &gtk::Grid, heading: &gtk::Button, month: (i32, u32)) {
    while let Some(child) = grid.first_child() {
        child.unparent();
    }

    let today = gtk::glib::DateTime::now_local()
        .ok()
        .map(|now| (now.year(), now.month() as u32, now.day_of_month() as u32))
        .unwrap_or((1970, 1, 1));
    let layout = widgets::month_grid(month.0, month.1, today);

    heading.set_label(&layout.heading);

    for (column, weekday) in widgets::WEEKDAYS.iter().enumerate() {
        let label = label(weekday, "whelk-widget-weekday", 0.5);

        label.set_hexpand(true);
        grid.attach(&label, column as i32, 0, 1, 1);
    }

    for day in 1..=layout.days {
        let slot = layout.leading + day as usize - 1;
        let label = label(&day.to_string(), "whelk-widget-day", 0.5);

        label.set_hexpand(true);
        label.set_valign(gtk::Align::Fill);

        if layout.today == Some(day) {
            label.add_css_class("today");
        }

        grid.attach(&label, (slot % 7) as i32, (slot / 7 + 1) as i32, 1, 1);
    }
}

fn build_weather(card: &gtk::Box, config: &WidgetConfig) -> Vec<gtk::glib::SourceId> {
    let place = label("", "whelk-widget-subtle", 0.0);
    let reading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = label("·", "whelk-widget-weather-icon", 0.5);
    let temperature = label("—", "whelk-widget-temperature", 0.0);
    let condition = label("", "whelk-widget-reading", 0.0);
    let range = label("", "whelk-widget-subtle", 0.0);
    let details = label("", "whelk-widget-subtle", 0.0);
    // The card's age, written only once the reading has gone old.
    let freshness = label("", "whelk-widget-subtle", 0.0);

    // The glyph, the degrees and the condition are one line; the place is only
    // written when the card was told where it is, because "San Francisco" on a
    // card pointed at another city is worse than no name at all.
    reading.set_hexpand(true);
    icon.set_valign(gtk::Align::Center);
    temperature.set_valign(gtk::Align::Baseline);
    condition.set_valign(gtk::Align::Baseline);
    place.set_visible(false);
    freshness.set_visible(false);
    reading.append(&icon);
    reading.append(&temperature);
    reading.append(&condition);
    card.append(&place);
    card.append(&reading);
    card.append(&range);
    card.append(&details);
    card.append(&freshness);

    let latitude = config
        .spec
        .text("latitude")
        .unwrap_or_else(|| "37.7749".to_owned());
    let longitude = config
        .spec
        .text("longitude")
        .unwrap_or_else(|| "-122.4194".to_owned());
    let imperial = config.spec.text("units").as_deref() == Some("imperial");
    let url = widgets::weather_url(&latitude, &longitude, imperial);
    let location = config.spec.text("location");
    let busy = Rc::new(Cell::new(false));
    let health: Rc<Cell<Option<Health>>> = Rc::new(Cell::new(None));
    let (update_health, tick_health) = (Rc::clone(&health), Rc::clone(&health));
    let configured_tooltip = config.tooltip();
    let tick_tooltip = configured_tooltip.clone();
    let own_tooltip = configured_tooltip;
    let (weak_place, weak_icon) = (place.downgrade(), icon.downgrade());
    let (weak_temperature, weak_condition) = (temperature.downgrade(), condition.downgrade());
    let (weak_range, weak_details) = (range.downgrade(), details.downgrade());
    let (weak_card, weak_freshness) = (card.downgrade(), freshness.downgrade());

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (
            Some(place_label),
            Some(icon_label),
            Some(temperature_label),
            Some(condition_label),
            Some(range_label),
            Some(details_label),
            Some(card),
            Some(freshness),
        ) = (
            weak_place.upgrade(),
            weak_icon.upgrade(),
            weak_temperature.upgrade(),
            weak_condition.upgrade(),
            weak_range.upgrade(),
            weak_details.upgrade(),
            weak_card.upgrade(),
            weak_freshness.upgrade(),
        )
        else {
            return false;
        };

        if let Some(location) = location.as_deref() {
            place_label.set_text(location);
            place_label.set_visible(true);
        }

        let record = Rc::clone(&update_health);
        let tooltip = own_tooltip.clone();

        fetch(url.clone(), Rc::clone(&busy), move |answer| {
            // A body that parsed to nothing is a failed read too.
            let outcome = match &answer {
                Ok(text) => widgets::parse_weather(text, !imperial).ok_or(FetchError::Failed),
                Err(error) => Err(*error),
            };
            let now = now_seconds();
            let previous = record.get();
            let had_reading = previous.and_then(|value| value.age(now)).is_some();

            match &outcome {
                Ok(reading) => {
                    icon_label.set_text(reading.glyph);
                    temperature_label.set_text(&reading.temperature);
                    condition_label.set_text(reading.condition);
                    range_label.set_text(&reading.range);
                    details_label.set_text(&reading.details);
                }
                // A card that has never loaded says why.  One that has keeps the
                // reading it is showing, and the age line and the hover title
                // carry the failure instead of painting over it.
                Err(error) if !had_reading => {
                    let (headline, hint) = failure_lines(*error, "Weather");

                    icon_label.set_text("·");
                    temperature_label.set_text("—");
                    condition_label.set_text(&headline);
                    range_label.set_text("");
                    details_label.set_text(hint);
                }
                Err(_) => {}
            }

            let next = Health::after(
                previous,
                outcome.as_ref().map(|_| ()).map_err(|error| *error),
                now,
            );

            record.set(Some(next));
            paint_health(
                &card,
                &freshness,
                Some(next),
                u64::from(WEATHER_INTERVAL),
                "Weather",
                tooltip.as_deref(),
            );
        });

        true
    });

    // The reading's age keeps moving between refreshes; without this tick the
    // line would be written once and then disagree with the clock for half an
    // hour.
    let tick_card = card.downgrade();
    let tick_freshness = freshness.downgrade();
    let ticker: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(card), Some(freshness)) = (tick_card.upgrade(), tick_freshness.upgrade()) else {
            return false;
        };

        paint_health(
            &card,
            &freshness,
            tick_health.get(),
            u64::from(WEATHER_INTERVAL),
            "Weather",
            tick_tooltip.as_deref(),
        );

        true
    });
    let mut timers = Vec::new();

    if let Some(timer) = schedule(WEATHER_INTERVAL, update) {
        timers.push(timer);
    }

    timers.push(repeat(FRESHNESS_TICK, ticker));

    timers
}

fn build_battery(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let percent = label("—", "whelk-widget-battery-percent", 0.0);
    let level = gtk::LevelBar::for_interval(0.0, 100.0);
    let status = label("", "whelk-widget-reading", 0.0);
    let device = label("", "whelk-widget-subtle", 0.0);

    level.add_css_class("whelk-widget-battery-level");
    level.set_value(0.0);
    level.set_hexpand(true);
    card.append(&percent);
    card.append(&level);
    card.append(&status);
    card.append(&device);

    let device_option = config.spec.text("device");
    let warning = config
        .spec
        .integer("warning")
        .map_or(30, |value| value as i32);
    let critical = config
        .spec
        .integer("critical")
        .map_or(15, |value| value as i32);

    let (weak_percent, weak_level) = (percent.downgrade(), level.downgrade());
    let (weak_status, weak_device) = (status.downgrade(), device.downgrade());

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(percent_label), Some(level), Some(status_label), Some(device_label)) = (
            weak_percent.upgrade(),
            weak_level.upgrade(),
            weak_status.upgrade(),
            weak_device.upgrade(),
        ) else {
            return false;
        };

        let Some(reading) = batterybar::read(device_option.as_deref()).filter(|info| info.present)
        else {
            percent_label.set_text("—");
            status_label.set_text("No battery");
            device_label.set_text("");
            level.set_visible(false);

            return true;
        };
        let summary = battery_summary(reading.capacity, &reading.status, warning, critical);
        let card = percent_label
            .ancestor(gtk::Box::static_type())
            .and_downcast::<gtk::Box>();

        percent_label.set_text(&summary.percent);
        status_label.set_text(&summary.status);
        device_label.set_text(&reading.name);
        level.set_visible(true);
        level.set_value(f64::from(reading.capacity));

        if let Some(card) = card {
            for class in ["warning", "critical", "charging"] {
                card.remove_css_class(class);
            }

            for class in summary.classes {
                card.add_css_class(class);
            }
        }

        true
    });

    schedule(30, update)
}

fn build_notes(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let body = label("", "whelk-widget-note", 0.0);

    body.set_wrap(true);
    // Words, not characters: a note is read, and `WordChar` split words across
    // lines whenever a line was a pixel short of the next word.
    body.set_wrap_mode(gtk::pango::WrapMode::Word);
    body.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.set_lines(9);
    body.set_justify(gtk::Justification::Left);
    body.set_yalign(0.0);
    body.set_vexpand(true);
    body.set_hexpand(true);
    card.append(&body);

    let file = config.spec.text("file");
    let text = config.spec.text("text");
    let configured = file.is_some() || text.is_some();
    let interval = config.interval();
    let weak_body = body.downgrade();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let Some(body) = weak_body.upgrade() else {
            return false;
        };

        let note = widgets::note_display(&widgets::note_text(file.as_deref(), text.as_deref()));

        if !configured {
            body.set_text("No note yet — set #:text or #:file.");
            body.add_css_class("empty");
        } else {
            body.set_text(&note);
            body.remove_css_class("empty");
        }

        true
    });

    schedule(interval, update)
}

fn build_reminders(card: &gtk::Box, config: &WidgetConfig, state_path: Option<&Path>, id: &str) {
    let saved = state_path.and_then(|path| widgetstate::get(path, id, widgets::REMINDER_STATE_KEY));
    let reminders = Reminders::restore(saved.as_deref(), config.spec.text("items").as_deref());
    let progress = label("", "whelk-widget-progress", 0.0);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let state = Rc::new(ReminderCard {
        list: list.clone(),
        progress,
        reminders: RefCell::new(reminders),
        state_path: state_path.map(Path::to_path_buf),
        id: id.to_owned(),
    });

    list.add_css_class("whelk-widget-reminder-list");
    list.set_vexpand(true);
    card.append(&state.progress);
    card.append(&list);
    state.render();
}

/// A reminders card: its list, the model behind it, and where a toggle is
/// written.  The rows hold a weak reference to it, so a card that is gone can
/// not be toggled back to life.
struct ReminderCard {
    list: gtk::Box,
    /// "2 of 5 done", or a line saying what to configure when there is nothing
    /// to tick.
    progress: gtk::Label,
    reminders: RefCell<Reminders>,
    state_path: Option<PathBuf>,
    id: String,
}

impl ReminderCard {
    fn render(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            child.unparent();
        }

        let rows = self.reminders.borrow().rows();
        let done = rows.iter().filter(|row| row.done).count();

        self.progress.set_text(&if rows.is_empty() {
            "No reminders — add #:items.".to_owned()
        } else if done == rows.len() {
            format!("All {} done", rows.len())
        } else {
            format!("{done} of {} done", rows.len())
        });

        for (index, row) in rows.iter().enumerate() {
            let button = gtk::Button::new();
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let mark = label(
                if row.done { "✓" } else { "○" },
                if row.done {
                    "whelk-widget-reminder-done"
                } else {
                    "whelk-widget-reminder-mark"
                },
                0.0,
            );
            let text = label("", "whelk-widget-reminder", 0.0);
            let card = Rc::downgrade(self);

            text.set_markup(&reminder_markup(row));
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.set_hexpand(true);

            if row.done {
                text.add_css_class("done");
            }

            content.append(&mark);
            content.append(&text);
            button.set_child(Some(&content));
            button.add_css_class("flat");
            button.add_css_class("whelk-widget-reminder-row");
            button.set_tooltip_text(Some(if row.done {
                "Mark this reminder as not done"
            } else {
                "Mark this reminder done"
            }));
            button.connect_clicked(move |_| {
                let Some(card) = card.upgrade() else {
                    return;
                };

                if card.reminders.borrow_mut().toggle(index) {
                    card.save();
                    card.render();
                }
            });
            self.list.append(&button);
        }
    }

    fn save(&self) {
        let Some(path) = self.state_path.as_deref() else {
            return;
        };
        let value = self.reminders.borrow().serialised();

        widgetstate::set(path, &self.id, widgets::REMINDER_STATE_KEY, Some(&value));
    }
}

fn build_world_clock(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let cities = widgets::cities(config.spec.text("cities").as_deref().unwrap_or_default());
    let mut rows = Vec::new();

    for city in cities {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let name = label(&city.label, "whelk-widget-city", 0.0);
        let time = label("--:--", "whelk-widget-world-time", 1.0);
        let day = label("", "whelk-widget-subtle", 1.0);

        name.set_hexpand(true);
        row.append(&name);
        row.append(&time);
        row.append(&day);
        day.set_halign(gtk::Align::End);
        card.append(&row);
        rows.push((city.zone, time.downgrade(), day.downgrade()));
    }

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let mut alive = false;

        for (zone, time, day) in &rows {
            let (Some(time), Some(day)) = (time.upgrade(), day.upgrade()) else {
                continue;
            };

            alive = true;

            if let Some((reading, weekday)) = widgets::city_time(zone) {
                time.set_text(&reading);
                day.set_text(&weekday);
            }
        }

        alive
    });

    schedule(30, update)
}

fn build_stocks(card: &gtk::Box, config: &WidgetConfig) -> Vec<gtk::glib::SourceId> {
    let symbols =
        widgets::stock_symbols(config.spec.text("symbols").as_deref().unwrap_or_default());
    let mut rows = Vec::new();

    for symbol in &symbols {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = label(symbol, "whelk-widget-stock-symbol", 0.0);
        let price = label("—", "whelk-widget-stock-price", 1.0);
        let change = label("", "whelk-widget-stock-change", 1.0);

        // Two fixed columns so the prices and the changes line up down the
        // card instead of each row finding its own width.
        name.set_hexpand(true);
        price.set_width_chars(9);
        change.set_width_chars(8);
        line.add_css_class("whelk-widget-stock-row");
        line.append(&name);
        line.append(&price);
        line.append(&change);
        change.set_halign(gtk::Align::End);
        card.append(&line);
        rows.push((symbol.clone(), price.downgrade(), change.downgrade()));
    }

    let footnote = label("", "whelk-widget-footnote", 0.0);
    // The quotes' age, written only once they have gone old.
    let freshness = label("", "whelk-widget-subtle", 0.0);

    freshness.set_visible(false);
    card.append(&footnote);
    card.append(&freshness);

    let url = widgets::stocks_url(&symbols);
    let busy = Rc::new(Cell::new(false));
    let health: Rc<Cell<Option<Health>>> = Rc::new(Cell::new(None));
    let (update_health, tick_health) = (Rc::clone(&health), Rc::clone(&health));
    let configured_tooltip = config.tooltip();
    let tick_tooltip = configured_tooltip.clone();
    let own_tooltip = configured_tooltip;
    let weak_footnote = footnote.downgrade();
    let (weak_card, weak_freshness) = (card.downgrade(), freshness.downgrade());

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(footnote), Some(card), Some(freshness)) = (
            weak_footnote.upgrade(),
            weak_card.upgrade(),
            weak_freshness.upgrade(),
        ) else {
            return false;
        };

        if symbols.is_empty() {
            footnote.set_text("Add symbols");

            return true;
        }

        let quoted: Vec<(String, gtk::Label, gtk::Label)> = rows
            .iter()
            .filter_map(|(symbol, price, change)| {
                Some((symbol.clone(), price.upgrade()?, change.upgrade()?))
            })
            .collect();
        let footnote_answer = footnote.clone();
        let wanted = symbols.clone();
        let record = Rc::clone(&update_health);
        let tooltip = own_tooltip.clone();

        fetch(url.clone(), Rc::clone(&busy), move |answer| {
            let outcome = match &answer {
                Ok(text) => {
                    let quotes = widgets::parse_stocks(text, &wanted);

                    if quotes.is_empty() {
                        Err(FetchError::Failed)
                    } else {
                        Ok(quotes)
                    }
                }
                Err(error) => Err(*error),
            };
            let now = now_seconds();
            let previous = record.get();
            let had_reading = previous.and_then(|value| value.age(now)).is_some();
            let delayed = "Delayed quotes · Yahoo Finance".to_owned();

            let footnote_text = match &outcome {
                Ok(quotes) => {
                    apply_quotes(&quoted, quotes);
                    delayed
                }
                // The rows still hold the last quotes, so the card keeps its
                // usual footnote and says how old they are below it; the hover
                // title carries the reason they stopped moving.
                Err(_) if had_reading => delayed,
                Err(error) => failure_lines(*error, "Quotes").0,
            };

            footnote_answer.set_text(&footnote_text);

            let next = Health::after(
                previous,
                outcome.as_ref().map(|_| ()).map_err(|error| *error),
                now,
            );

            record.set(Some(next));
            paint_health(
                &card,
                &freshness,
                Some(next),
                u64::from(STOCKS_INTERVAL),
                "Quotes",
                tooltip.as_deref(),
            );
        });

        true
    });

    // Quotes go stale between refreshes as well; the tick rewrites the age and
    // the hover title without asking Yahoo anything.
    let tick_card = card.downgrade();
    let tick_freshness = freshness.downgrade();
    let ticker: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(card), Some(freshness)) = (tick_card.upgrade(), tick_freshness.upgrade()) else {
            return false;
        };

        paint_health(
            &card,
            &freshness,
            tick_health.get(),
            u64::from(STOCKS_INTERVAL),
            "Quotes",
            tick_tooltip.as_deref(),
        );

        true
    });
    let mut timers = Vec::new();

    if let Some(timer) = schedule(STOCKS_INTERVAL, update) {
        timers.push(timer);
    }

    timers.push(repeat(FRESHNESS_TICK, ticker));

    timers
}

fn apply_quotes(rows: &[(String, gtk::Label, gtk::Label)], quotes: &[StockQuote]) {
    for (symbol, price, change) in rows {
        let Some(quote) = quotes.iter().find(|quote| &quote.symbol == symbol) else {
            continue;
        };

        price.set_text(&quote.price_text());
        change.set_text(&quote.change_text());
        change.remove_css_class("up");
        change.remove_css_class("down");
        change.add_css_class(if quote.is_up() { "up" } else { "down" });
    }
}

fn build_custom(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let body = label("", "whelk-widget-note", 0.0);

    body.set_wrap(true);
    body.set_wrap_mode(gtk::pango::WrapMode::Word);
    body.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.set_lines(8);
    body.set_yalign(0.0);
    body.set_vexpand(true);
    body.set_hexpand(true);
    card.append(&body);

    let exec = config.spec.text("exec");
    let empty = config.spec.text("empty");
    let busy = Rc::new(Cell::new(false));
    let interval = config.interval();
    let weak_body = body.downgrade();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let Some(body) = weak_body.upgrade() else {
            return false;
        };

        let Some(command_line) = exec.as_deref().filter(|line| !line.is_empty()) else {
            body.set_text("Set #:exec on this widget.");

            return true;
        };

        if busy.replace(true) {
            return true;
        }

        let body = body.clone();
        let empty = empty.clone();
        let busy = Rc::clone(&busy);

        command::run(command_line, move |output| {
            busy.set(false);
            body.set_text(&widgets::custom_text(output.as_deref(), empty.as_deref()));
        });

        true
    });

    schedule(interval, update)
}

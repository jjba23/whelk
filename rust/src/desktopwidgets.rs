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
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::batterybar;
use crate::command;
use crate::cpubar;
use crate::memorybar;
use crate::mpris::{self, Control, Mpris, Reading};
use crate::sources;
use crate::style;
use crate::ui::layer_shell_available;
use crate::visualizer::Cava;
use crate::widgets::{
    self, Area, FALLBACK_AREA, NewsItem, RecentFile, ReminderRow, Reminders, StockQuote,
    WidgetConfig, WidgetKind,
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
    /* The pack's reading is part of the card's ink, not a stock GNOME bar:
     * the stock blue was the only cool colour on a warm desktop. */
    .whelk-widget-battery-level trough {
        min-height: 8px;
        border-radius: 4px;
        background-color: alpha(currentColor, 0.14);
    }
    .whelk-widget-battery-level progress {
        min-height: 8px;
        border-radius: 4px;
        background-color: alpha(currentColor, 0.62);
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
    .whelk-widget-calendar-week {
        opacity: 0.4;
        font-size: 10px;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-forecast-row {
        padding: 1px 0;
    }
    .whelk-widget-forecast-day {
        font-size: 12px;
        opacity: 0.78;
    }
    .whelk-widget-forecast-range {
        font-size: 12px;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-player-title {
        font-size: 17px;
        font-weight: 700;
    }
    .whelk-widget-player-detail {
        font-size: 12px;
        opacity: 0.72;
    }
    .whelk-widget-player-time {
        font-size: 11px;
        opacity: 0.6;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-player-controls {
        margin-top: 2px;
    }
    .whelk-widget-player-controls button {
        color: inherit;
        padding: 2px 10px;
        min-height: 24px;
    }
    .whelk-widget-player-progress trough {
        min-height: 5px;
        border-radius: 3px;
        background-color: alpha(currentColor, 0.14);
    }
    .whelk-widget-player-progress progress {
        min-height: 5px;
        border-radius: 3px;
        background-color: alpha(currentColor, 0.62);
    }
    .whelk-widget-visualizer {
        min-height: 74px;
    }
    .whelk-widget-visualizer-status {
        font-size: 11px;
        opacity: 0.65;
    }
    .whelk-widget-media-art {
        border-radius: 8px;
        background-color: alpha(currentColor, 0.08);
    }
    .whelk-widget-media-title {
        font-size: 18px;
        font-weight: 700;
    }
    .whelk-widget-media-detail {
        font-size: 12px;
        opacity: 0.72;
    }
    .whelk-widget-media-time {
        font-size: 11px;
        opacity: 0.6;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-media-scale trough {
        min-height: 5px;
        border-radius: 3px;
        background-color: alpha(currentColor, 0.14);
    }
    .whelk-widget-media-scale highlight {
        min-height: 5px;
        border-radius: 3px;
        background-color: alpha(currentColor, 0.62);
    }
    .whelk-widget-media-controls button {
        color: inherit;
        font-size: 20px;
        min-width: 42px;
        min-height: 36px;
        padding: 2px 8px;
    }
    .whelk-widget-media-controls button.whelk-widget-media-primary {
        font-size: 24px;
        min-width: 52px;
    }
    .whelk-widget-photo {
        border-radius: 9px;
        background-color: alpha(black, 0.22);
    }
    .whelk-widget-photo-caption {
        font-size: 12px;
        opacity: 0.72;
    }
    .whelk-widget-photo-count {
        font-size: 10px;
        opacity: 0.45;
    }
    .whelk-widget-file-row {
        padding: 4px 6px;
        border-radius: 7px;
        min-height: 28px;
    }
    .whelk-widget-file-row:hover {
        background-color: alpha(currentColor, 0.07);
    }
    .whelk-widget-file-kind {
        opacity: 0.45;
        font-size: 10px;
    }
    .whelk-widget-news-row {
        padding: 4px 6px;
        border-radius: 7px;
    }
    .whelk-widget-news-row:hover {
        background-color: alpha(currentColor, 0.07);
    }
    .whelk-widget-news-title {
        font-size: 13px;
    }
    .whelk-widget-news-source {
        opacity: 0.5;
        font-size: 10px;
    }
    .whelk-widget-system-row {
        padding: 2px 0;
    }
    .whelk-widget-system-name {
        font-size: 12px;
        opacity: 0.72;
    }
    .whelk-widget-system-value {
        font-size: 13px;
        font-feature-settings: \"tnum\";
    }
    .whelk-widget-spark {
        min-width: 54px;
        min-height: 18px;
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

    // The pack's own status already names the state; the second half is the
    // *source*, not the same word again.  "Charging · charging" was the card
    // saying one thing twice.
    let source = if charging { "on power" } else { "on battery" };

    BatterySummary {
        percent: format!("{capacity}%"),
        status: if status.is_empty() {
            source.to_owned()
        } else {
            format!("{status} · {source}")
        },
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
    /// What the cards on screen were built from.  A settings save that leaves
    /// the widgets, their material and the outputs untouched reuses them
    /// instead of tearing every timer and surface down and building it again.
    configs: Vec<WidgetConfig>,
    style: BarSettings,
    signature: String,
    state_present: bool,
    visible: bool,
}

impl Widgets {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            state_path: None,
            configs: Vec::new(),
            style: BarSettings::default(),
            signature: String::new(),
            state_present: false,
            visible: true,
        }
    }

    pub fn count(&self) -> usize {
        self.cards.len()
    }

    /// Whether the cards are being shown.  Hiding them keeps their timers and
    /// their surfaces alive, so showing them again is instant.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Show or hide every card without rebuilding any of them.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;

        for card in &self.cards {
            card.window.set_visible(visible);
        }
    }

    /// Replace every card with the ones `configs` asks for.  An unknown kind is
    /// reported and skipped, as the C manager does.
    ///
    /// The same configuration on the same outputs keeps the cards it already
    /// has: a reload is a settings save, and a save of the bar's colour must
    /// not restart a note's file watch or a player's MPRIS subscription.
    pub fn build(
        &mut self,
        application: &gtk::Application,
        configs: &[WidgetConfig],
        style: &BarSettings,
        config_path: &Path,
    ) {
        let state_path = widgetstate::state_path(config_path);
        let signature = display_signature();

        let previous = CardSet {
            configs: &self.configs,
            style: &self.style,
            signature: &self.signature,
            state_path: self.state_path.as_deref(),
        };
        let next = CardSet {
            configs,
            style,
            signature: &signature,
            state_path: state_path.as_deref(),
        };

        if same_configuration(previous, next)
            && !state_disappeared(self.state_present, state_path.as_deref())
        {
            self.state_present = state_path.as_deref().is_some_and(Path::exists);

            return;
        }

        self.clear();
        self.configs = configs.to_vec();
        self.style = style.clone();
        self.signature = signature;
        self.state_present = state_path.as_deref().is_some_and(Path::exists);
        self.state_path = state_path;

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
                if !self.visible {
                    card.window.set_visible(false);
                }

                self.cards.push(card);
            }
        }

        if !self.cards.is_empty() {
            eprintln!("whelk: {} desktop widget(s) up", self.cards.len());
        }
    }

    pub fn clear(&mut self) {
        self.cards.clear();
        self.configs.clear();
        self.signature.clear();
        self.state_present = false;
    }
}

impl Default for Widgets {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything that makes one card set what it is.  A rebuild compares two of
/// these rather than eight loose arguments.
#[derive(Clone, Copy)]
pub struct CardSet<'a> {
    pub configs: &'a [WidgetConfig],
    pub style: &'a BarSettings,
    pub signature: &'a str,
    pub state_path: Option<&'a Path>,
}

/// Whether a card set is the one already on screen.
///
/// A settings save is not a reason to restart every timer, MPRIS subscription
/// and photo frame; the cards only need replacing when their configuration,
/// their material, the state file or the outputs changed.  This is the whole
/// rule, as a value a test can state.
pub fn same_configuration(previous: CardSet<'_>, next: CardSet<'_>) -> bool {
    previous.configs == next.configs
        && previous.style == next.style
        && previous.signature == next.signature
        && previous.state_path == next.state_path
}

/// Whether a state file the last build saw is gone now.  Deleting
/// =widgets-<stem>.ini= is the documented way to send every card back to its
/// settings position, so a lost file has to rebuild even though the cards'
/// configuration did not change.
pub fn state_disappeared(previous_present: bool, path: Option<&Path>) -> bool {
    previous_present && path.is_some_and(|path| !path.exists())
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
            // The card's size is the configuration's, not the longest line's:
            // a note used to widen its own card past the width in settings.scm
            // and then get clipped at the surface edge instead of wrapping.
            window.set_default_size(config.width, config.height);

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

/// The outputs this card set was placed against.  A hotplug or a resolution
/// change rebuilds the cards; the same outputs let a settings save keep them.
fn display_signature() -> String {
    let Some(display) = gtk::gdk::Display::default() else {
        return String::new();
    };
    let model = display.monitors();
    let mut signature = String::new();

    for index in 0..model.n_items() {
        let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let geometry = monitor.geometry();
        let connector = monitor
            .connector()
            .map(|name| name.to_string())
            .unwrap_or_default();

        signature.push_str(&format!(
            "{connector}:{}x{};",
            geometry.width(),
            geometry.height()
        ));
    }

    signature
}

thread_local! {
    /// The structural card rules are the same for every card.  Installing them
    /// once per display keeps a desktop of a dozen cards from parsing the same
    /// stylesheet a dozen times.
    static CARD_RULES: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

fn install_card_rules(display: &gtk::gdk::Display) {
    CARD_RULES.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let css = gtk::CssProvider::new();

        css.load_from_string(CARD_CSS);
        gtk::style_context_add_provider_for_display(
            display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *slot.borrow_mut() = Some(css);
    });
}

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
    let named = config.spec.boolean("header?").unwrap_or(true);

    row.add_css_class("whelk-widget-header");

    if named {
        row.append(&label(glyph, "whelk-widget-glyph", 0.0));
        row.append(&label(&config.title(), "whelk-widget-title", 0.0));
    }

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

    install_card_rules(&display);

    let mut generated = style::css(
        widget_id,
        &skin,
        "whelk-widget-card",
        config.css().as_deref(),
    );

    // Full transparency is a material of its own, not just an opacity of zero:
    // the generated skin may draw a frosted background, a border or a shadow
    // from the bar's theme, and a card over a video wants none of them.  These
    // rules come last and with the same specificity, so they win whatever the
    // palette said.
    if config.spec.boolean("transparent?").unwrap_or(false) {
        generated.push_str(&format!(
            "\nwindow#{widget_id} .whelk-widget-card {{\n  background-color: transparent;\n  background-image: none;\n  border: none;\n  box-shadow: none;\n}}\n"
        ));
    }

    let css = gtk::CssProvider::new();

    css.load_from_string(&generated);
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
        "curl --fail --silent --show-error --location --max-time 20 --max-filesize 2097152 --user-agent {} {}",
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
        // The networked cards carry a second timer that only re-reads their
        // age, so they answer with every source they own.
        WidgetKind::Weather => return build_weather(card, config),
        WidgetKind::Stocks => return build_stocks(card, config, style),
        WidgetKind::Clock => build_clock(card, config, style),
        WidgetKind::Calendar => build_calendar(card, config),
        WidgetKind::Battery => build_battery(card, config),
        WidgetKind::Notes => build_notes(card, config),
        WidgetKind::Reminders => {
            build_reminders(card, config, state_path, id);

            None
        }
        WidgetKind::WorldClock => build_world_clock(card, config),
        WidgetKind::NowPlaying => build_now_playing(card, config),
        WidgetKind::Visualizer => {
            build_visualizer(card, config, style);

            None
        }
        WidgetKind::Media => build_media(card, config),
        WidgetKind::Photos => build_photos(card, config),
        WidgetKind::Files => build_files(card, config),
        WidgetKind::News => return build_news(card, config),
        WidgetKind::System => build_system(card),
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
    let text = health
        .and_then(|value| value.quiet_age(now, interval))
        .unwrap_or_default();

    // A tick that says what the card already says is not a repaint.  Without
    // this, every networked card rewrote its own tooltip twice a minute.
    if freshness.text() != text {
        freshness.set_text(&text);
    }

    let visible = !text.is_empty();

    if freshness.is_visible() != visible {
        freshness.set_visible(visible);
    }

    let tooltip = health.and_then(|value| value.tooltip(what, now));
    let wanted = tooltip.as_deref().or(configured_tooltip);

    if card.tooltip_text().as_deref() != wanted {
        card.set_tooltip_text(wanted);
    }
}

fn ink(style: &BarSettings) -> gtk::gdk::RGBA {
    style
        .foreground
        .as_deref()
        .filter(|value| !value.is_empty())
        .and_then(|value| gtk::gdk::RGBA::parse(value).ok())
        .unwrap_or_else(|| gtk::gdk::RGBA::new(0.92, 0.92, 0.94, 1.0))
}

/// The clock at `zone`, or this machine's own clock when the card named none.
fn now_in_zone(zone: Option<&str>) -> Option<gtk::glib::DateTime> {
    match zone.filter(|zone| !zone.is_empty()) {
        Some(zone) => gtk::glib::TimeZone::from_identifier(Some(zone))
            .and_then(|timezone| gtk::glib::DateTime::now(&timezone).ok()),
        None => gtk::glib::DateTime::now_local().ok(),
    }
}

fn build_clock(
    card: &gtk::Box,
    config: &WidgetConfig,
    style: &BarSettings,
) -> Option<gtk::glib::SourceId> {
    let analog = config.spec.text("style").as_deref() != Some("digital");
    let seconds = config.spec.boolean("show-seconds?").unwrap_or(true);
    let colour = ink(style);
    let zone = config
        .spec
        .text("timezone")
        .filter(|zone| !zone.trim().is_empty());
    let time_format = config
        .spec
        .text("format")
        .filter(|format| !format.is_empty())
        .unwrap_or_else(|| {
            if seconds {
                "%H:%M:%S".to_owned()
            } else {
                "%H:%M".to_owned()
            }
        });
    let date_format = config
        .spec
        .text("date-format")
        .filter(|format| !format.is_empty())
        .unwrap_or_else(|| "%A, %d %B".to_owned());
    let face_zone = zone.clone();

    let face = analog.then(|| {
        let face = gtk::DrawingArea::new();

        face.add_css_class("whelk-widget-clock-face");
        face.set_hexpand(true);
        face.set_vexpand(true);
        face.set_size_request(120, 120);
        face.set_content_width(170);
        face.set_content_height(170);
        face.set_draw_func(move |_, cairo, width, height| {
            draw_clock_face(cairo, width, height, colour, seconds, face_zone.as_deref());
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
    let zone = zone.clone();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(time), Some(date)) = (weak_time.upgrade(), weak_date.upgrade()) else {
            return false;
        };
        let now = now_in_zone(zone.as_deref());
        let time_text = now
            .as_ref()
            .and_then(|now| now.format(&time_format).ok())
            .map(|text| text.to_string())
            .unwrap_or_else(|| "--:--".to_owned());
        let date_text = now
            .as_ref()
            .and_then(|now| now.format(&date_format).ok())
            .map(|text| text.to_string())
            .unwrap_or_default();
        let changed = time.text() != time_text || date.text() != date_text;

        // Setting a label with the text it already has still queues a resize;
        // the face is only worth redrawing when the reading moved.
        if time.text() != time_text {
            time.set_text(&time_text);
        }

        if date.text() != date_text {
            date.set_text(&date_text);
        }

        if changed && let Some(face) = weak_face.as_ref().and_then(|face| face.upgrade()) {
            face.queue_draw();
        }

        true
    });

    // A card without a second hand cannot look wrong within fifteen seconds,
    // and a minute hand that moved a quarter of a degree is not worth a wakeup
    // every second.
    schedule(if seconds { 1 } else { 15 }, update)
}

fn draw_clock_face(
    cairo: &gtk::cairo::Context,
    width: i32,
    height: i32,
    colour: gtk::gdk::RGBA,
    seconds: bool,
    zone: Option<&str>,
) {
    let Some(now) = now_in_zone(zone) else {
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

fn build_calendar(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let show_weeks = config.spec.boolean("show-week-numbers?").unwrap_or(false);
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
    let last: Rc<RefCell<Option<widgets::MonthGrid>>> = Rc::new(RefCell::new(None));
    let painted = Rc::clone(&last);

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(grid), Some(month)) = (weak_grid.upgrade(), weak_month.upgrade()) else {
            return false;
        };
        let today = gtk::glib::DateTime::now_local()
            .ok()
            .map(|now| (now.year(), now.month() as u32, now.day_of_month() as u32))
            .unwrap_or((1970, 1, 1));
        let layout = widgets::month_grid(drawn.get().0, drawn.get().1, today);

        // The timer runs every minute so midnight is noticed, not so the whole
        // grid is rebuilt every minute.  A month that has not changed is one
        // comparison and no widget allocation.
        if painted.borrow().as_ref() == Some(&layout) {
            return true;
        }

        *painted.borrow_mut() = Some(layout.clone());
        render_month(&grid, &month, &layout, show_weeks);

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

fn render_month(
    grid: &gtk::Grid,
    heading: &gtk::Button,
    layout: &widgets::MonthGrid,
    show_weeks: bool,
) {
    while let Some(child) = grid.first_child() {
        child.unparent();
    }

    heading.set_label(&layout.heading);

    let offset = usize::from(show_weeks);

    if show_weeks {
        let week = label("Wk", "whelk-widget-weekday", 0.5);

        week.set_hexpand(false);
        week.set_width_chars(2);
        grid.attach(&week, 0, 0, 1, 1);
    }

    for (column, weekday) in widgets::WEEKDAYS.iter().enumerate() {
        let label = label(weekday, "whelk-widget-weekday", 0.5);

        label.set_hexpand(true);
        grid.attach(&label, (column + offset) as i32, 0, 1, 1);
    }

    let rows = layout.weeks.len();

    for (row, week) in layout.weeks.iter().enumerate() {
        if show_weeks {
            let label = label(&week.to_string(), "whelk-widget-calendar-week", 0.5);

            label.set_width_chars(2);
            grid.attach(&label, 0, (row + 1) as i32, 1, 1);
        }
    }

    for day in 1..=layout.days {
        let slot = layout.leading + day as usize - 1;
        let row = slot / 7;

        if row >= rows {
            continue;
        }

        let label = label(&day.to_string(), "whelk-widget-day", 0.5);

        label.set_hexpand(true);
        label.set_valign(gtk::Align::Fill);

        if layout.today == Some(day) {
            label.add_css_class("today");
        }

        grid.attach(&label, (slot % 7 + offset) as i32, (row + 1) as i32, 1, 1);
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
    // The days under today's conditions, when the card asked for them.
    let forecast = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let forecast_days = config
        .spec
        .integer("forecast-days")
        .unwrap_or(0)
        .clamp(0, 5) as usize;

    forecast.add_css_class("whelk-widget-forecast");
    forecast.set_visible(forecast_days > 0);

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
    card.append(&forecast);
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
    let url = if forecast_days > 0 {
        widgets::weather_url_days(&latitude, &longitude, imperial, forecast_days as u32)
    } else {
        widgets::weather_url(&latitude, &longitude, imperial)
    };
    let location = config.spec.text("location");
    let interval = config.interval();
    let health_interval = u64::from(if interval == 0 {
        WEATHER_INTERVAL
    } else {
        interval
    });
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
    let weak_forecast = forecast.downgrade();

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
            Some(forecast_box),
        ) = (
            weak_place.upgrade(),
            weak_icon.upgrade(),
            weak_temperature.upgrade(),
            weak_condition.upgrade(),
            weak_range.upgrade(),
            weak_details.upgrade(),
            weak_card.upgrade(),
            weak_freshness.upgrade(),
            weak_forecast.upgrade(),
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
                    paint_forecast(&forecast_box, &reading.forecast, forecast_days);
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
                    clear_box(&forecast_box);
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
                health_interval,
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
            health_interval,
            "Weather",
            tick_tooltip.as_deref(),
        );

        true
    });
    let mut timers = Vec::new();

    if let Some(timer) = schedule(interval, update) {
        timers.push(timer);
    }

    timers.push(repeat(FRESHNESS_TICK, ticker));

    timers
}

/// Draw the days under today's conditions.  A card that asked for none has an
/// empty box; a card that asked for more than the answer carried draws what
/// the answer carried.
fn paint_forecast(card: &gtk::Box, days: &[widgets::ForecastDay], wanted: usize) {
    clear_box(card);
    card.set_visible(wanted > 0 && !days.is_empty());

    for day in days.iter().take(wanted) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let glyph = label(day.glyph, "whelk-widget-forecast-glyph", 0.0);
        let name = label(&day.label, "whelk-widget-forecast-day", 0.0);
        let range = label(&day.range, "whelk-widget-forecast-range", 1.0);

        row.add_css_class("whelk-widget-forecast-row");
        name.set_hexpand(true);
        range.set_halign(gtk::Align::End);
        row.append(&glyph);
        row.append(&name);
        row.append(&range);
        card.append(&row);
    }
}

/// Remove every child of a box, leaving the box itself in the tree.
fn clear_box(card: &gtk::Box) {
    while let Some(child) = card.first_child() {
        child.unparent();
    }
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
    // A wrapping label still asks for its longest line, and the card's width
    // is a minimum: without a ceiling the note widened the surface past the
    // configured width and was clipped at the edge.  The ceiling is measured
    // in the label's own font, not guessed in characters.
    let content = (config.width - 28).max(40);

    let char_width = body
        .pango_context()
        .metrics(None, None)
        .approximate_char_width();

    if char_width > 0 {
        body.set_max_width_chars((content * 1024 / char_width).max(8));
    }

    card.append(&body);

    let file = config.spec.text("file");
    let text = config.spec.text("text");
    let configured = file.is_some() || text.is_some();
    let interval = config.interval();
    let weak_body = body.downgrade();
    let last_modified: Rc<Cell<Option<std::time::SystemTime>>> = Rc::new(Cell::new(None));
    let last_painted: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let Some(body) = weak_body.upgrade() else {
            return false;
        };

        // A note read from a file is only worth re-reading when the file's own
        // timestamp moved.  Without this, a card that refreshes every thirty
        // seconds parsed and rewrapped the same note all afternoon.
        if let Some(path) = file.as_deref()
            && let Ok(modified) = std::fs::metadata(path).and_then(|meta| meta.modified())
        {
            if last_modified.get() == Some(modified) {
                return true;
            }

            last_modified.set(Some(modified));
        }

        let painted = if configured {
            widgets::note_display(&widgets::note_text(file.as_deref(), text.as_deref()))
        } else {
            "No note yet — set #:text or #:file.".to_owned()
        };

        if *last_painted.borrow() == painted {
            return true;
        }

        *last_painted.borrow_mut() = painted.clone();
        body.set_text(&painted);

        if configured {
            body.remove_css_class("empty");
        } else {
            body.add_css_class("empty");
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
    let day_night = config.spec.boolean("day-night?").unwrap_or(false);
    let interval = config.interval();
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

            if let Some(reading) = widgets::city_reading(zone) {
                let time_text = if day_night {
                    format!(
                        "{} {}",
                        if reading.is_day() { "☀" } else { "☾" },
                        reading.time
                    )
                } else {
                    reading.time.clone()
                };

                if time.text() != time_text {
                    time.set_text(&time_text);
                }

                if day.text() != reading.weekday {
                    day.set_text(&reading.weekday);
                }
            }
        }

        alive
    });

    schedule(interval, update)
}

fn build_visualizer(card: &gtk::Box, config: &WidgetConfig, style: &BarSettings) {
    let bars = config
        .spec
        .integer("bars")
        .unwrap_or(widgets::VISUALIZER_BARS as i64)
        .clamp(8, widgets::VISUALIZER_MAX_BARS as i64) as usize;
    let framerate = config
        .spec
        .integer("framerate")
        .unwrap_or(widgets::VISUALIZER_FRAMERATE as i64)
        .clamp(10, 60) as u32;
    let mirror = config.spec.boolean("mirror?").unwrap_or(false);
    let show_peaks = config.spec.boolean("peaks?").unwrap_or(true);
    let accent = style
        .accent
        .as_deref()
        .filter(|value| !value.is_empty())
        .and_then(|value| gtk::gdk::RGBA::parse(value).ok())
        .unwrap_or_else(|| ink(style));
    let frame = Rc::new(RefCell::new(widgets::VisualizerFrame::new(bars)));
    let area = gtk::DrawingArea::new();
    let status = label("", "whelk-widget-visualizer-status", 0.0);

    area.add_css_class("whelk-widget-visualizer");
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_content_height(74);
    status.set_visible(false);
    card.append(&area);
    card.append(&status);

    let drawn = Rc::clone(&frame);
    area.set_draw_func(move |_, cairo, width, height| {
        draw_visualizer(
            cairo,
            width,
            height,
            &drawn.borrow(),
            accent,
            mirror,
            show_peaks,
        );
    });

    let weak_area = area.downgrade();
    let weak_status = status.downgrade();
    let incoming = Rc::clone(&frame);
    let incoming_area = weak_area.clone();
    let on_frame = move |levels: Vec<f64>| {
        incoming.borrow_mut().push(&levels);

        if let Some(area) = incoming_area.upgrade() {
            area.queue_draw();
        }
    };
    let ended = Rc::clone(&frame);
    let ended_area = weak_area.clone();
    let on_end = move || {
        ended.borrow_mut().clear();

        if let Some(area) = ended_area.upgrade() {
            area.queue_draw();
        }

        if let Some(status) = weak_status.upgrade() {
            status.set_text("Visualizer stopped — is cava running?");
            status.set_visible(true);
        }
    };

    match Cava::start(bars, framerate, on_frame, on_end) {
        Ok(cava) => {
            let stop = Rc::clone(&cava);

            area.connect_destroy(move |_| stop.stop());
        }
        Err(error) => {
            status.set_text(&error);
            status.set_visible(true);
        }
    }
}

/// Draw one visualizer frame.  Bars grow from the bottom, or from the centre
/// when the card asked for a mirrored spectrum; peaks are thin caps that hold
/// their height for a moment before the next frame takes them down.
fn draw_visualizer(
    cairo: &gtk::cairo::Context,
    width: i32,
    height: i32,
    frame: &widgets::VisualizerFrame,
    accent: gtk::gdk::RGBA,
    mirror: bool,
    show_peaks: bool,
) {
    let width = f64::from(width.max(1));
    let height = f64::from(height.max(1));
    let bars = frame.levels.len().max(1);
    let gap = if bars > 40 { 1.0 } else { 2.0 };
    let bar_width = ((width - gap * (bars as f64 - 1.0)) / bars as f64).max(1.0);
    let usable = (height - 6.0).max(1.0);
    let red = f64::from(accent.red());
    let green = f64::from(accent.green());
    let blue = f64::from(accent.blue());

    // A quiet baseline says the card is alive even when the room is silent.
    cairo.set_source_rgba(red, green, blue, 0.14);
    cairo.rectangle(0.0, height - 1.0, width, 1.0);
    let _ = cairo.fill();

    for (index, level) in frame.levels.iter().enumerate() {
        let level = level.clamp(0.0, 1.0);
        let amplitude = (level * usable).max(if level > 0.0 { 2.0 } else { 0.0 });

        if amplitude < 0.5 {
            continue;
        }

        let x = index as f64 * (bar_width + gap);
        let top = if mirror {
            (height - amplitude) / 2.0
        } else {
            height - amplitude
        };
        let alpha = 0.55 + level * 0.35;

        cairo.set_source_rgba(red, green, blue, alpha);
        rounded_bar(cairo, x, top, bar_width, amplitude);

        if show_peaks
            && let Some(peak) = frame.peaks.get(index)
            && *peak > level + 0.01
        {
            let peak = peak.clamp(0.0, 1.0);
            let peak_top = if mirror {
                (height - peak * usable) / 2.0
            } else {
                height - peak * usable
            };

            cairo.set_source_rgba(red, green, blue, 0.92);
            cairo.rectangle(x, (peak_top - 1.0).max(0.0), bar_width, 2.0);

            if mirror {
                cairo.rectangle(
                    x,
                    (height - peak_top - 1.0).min(height - 2.0),
                    bar_width,
                    2.0,
                );
            }

            let _ = cairo.fill();
        }
    }
}

fn rounded_bar(cairo: &gtk::cairo::Context, x: f64, top: f64, width: f64, height: f64) {
    let radius = (width / 2.0).min(3.0).min(height / 2.0).max(0.0);
    let bottom = top + height;

    cairo.move_to(x, bottom);
    cairo.line_to(x, top + radius);
    cairo.arc(
        x + radius,
        top + radius,
        radius,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cairo.line_to(x + width - radius, top);
    cairo.arc(
        x + width - radius,
        top + radius,
        radius,
        1.5 * std::f64::consts::PI,
        2.0 * std::f64::consts::PI,
    );
    cairo.line_to(x + width, bottom);
    cairo.close_path();
    let _ = cairo.fill();
}

fn build_media(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let art = gtk::Picture::new();
    let title = label("Nothing playing", "whelk-widget-media-title", 0.0);
    let detail = label("", "whelk-widget-media-detail", 0.0);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.001);
    let elapsed = label("0:00", "whelk-widget-media-time", 0.0);
    let total = label("0:00", "whelk-widget-media-time", 1.0);
    let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let previous = gtk::Button::with_label("⏮");
    let play = gtk::Button::with_label("⏯");
    let next = gtk::Button::with_label("⏭");
    let player = label("", "whelk-widget-footnote", 1.0);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);

    art.add_css_class("whelk-widget-media-art");
    art.set_content_fit(gtk::ContentFit::Cover);
    art.set_can_shrink(true);
    art.set_size_request(76, 76);
    art.set_visible(false);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&detail);
    top.append(&art);
    top.append(&text);

    scale.add_css_class("whelk-widget-media-scale");
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_sensitive(false);
    timeline.append(&elapsed);
    timeline.append(&scale);
    timeline.append(&total);

    controls.add_css_class("whelk-widget-media-controls");
    controls.set_halign(gtk::Align::Center);
    play.add_css_class("whelk-widget-media-primary");

    for (button, tooltip) in [
        (&previous, "Previous track"),
        (&play, "Play or pause"),
        (&next, "Next track"),
    ] {
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tooltip));
        controls.append(button);
    }

    player.set_halign(gtk::Align::End);
    card.append(&top);
    card.append(&timeline);
    card.append(&controls);
    card.append(&player);

    let weak_art = art.downgrade();
    let weak_title = title.downgrade();
    let weak_detail = detail.downgrade();
    let weak_scale = scale.downgrade();
    let weak_elapsed = elapsed.downgrade();
    let weak_total = total.downgrade();
    let weak_controls = controls.downgrade();
    let weak_player = player.downgrade();
    let artwork = Rc::new(RefCell::new(None));
    let seeking = Rc::new(Cell::new(false));
    let callback_art = weak_art.clone();
    let callback_title = weak_title.clone();
    let callback_detail = weak_detail.clone();
    let callback_scale = weak_scale.clone();
    let callback_elapsed = weak_elapsed.clone();
    let callback_total = weak_total.clone();
    let callback_controls = weak_controls.clone();
    let callback_player = weak_player.clone();
    let callback_artwork = Rc::clone(&artwork);
    let callback_seeking = Rc::clone(&seeking);

    let engine = Mpris::start(config.spec.text("player"), move |reading| {
        paint_media(
            reading,
            &callback_art,
            &callback_artwork,
            &callback_title,
            &callback_detail,
            &callback_scale,
            &callback_elapsed,
            &callback_total,
            &callback_controls,
            &callback_player,
            &callback_seeking,
        );
    });

    paint_media(
        &engine.reading(),
        &weak_art,
        &artwork,
        &weak_title,
        &weak_detail,
        &weak_scale,
        &weak_elapsed,
        &weak_total,
        &weak_controls,
        &weak_player,
        &seeking,
    );

    for (button, control) in [
        (&previous, Control::Previous),
        (&play, Control::PlayPause),
        (&next, Control::Next),
    ] {
        let engine = Rc::clone(&engine);

        button.connect_clicked(move |_| engine.control(control));
    }

    // `change-value` only fires for a gesture or a key the user made, so a
    // programmatic repaint does not seek the player.  A drag produces many
    // values; one seek lands after the hand stops for a moment.
    let pending = Rc::new(Cell::new(None::<i64>));
    let seeker = Rc::clone(&engine);
    let seeking_now = Rc::clone(&seeking);

    scale.connect_change_value(move |_, _, value| {
        pending.set(Some(value.max(0.0) as i64));

        if !seeking_now.replace(true) {
            let pending = Rc::clone(&pending);
            let settled = Rc::clone(&seeking_now);
            let seeker = Rc::clone(&seeker);

            gtk::glib::timeout_add_local_once(Duration::from_millis(120), move || {
                settled.set(false);

                if let Some(position) = pending.take() {
                    seeker.seek_to(position);
                }
            });
        }

        gtk::glib::Propagation::Proceed
    });

    // GtkRange only pages the trough on a click, which does nothing when the
    // page size is zero.  A click on the seek line should land where it was
    // clicked, the way every other player's seek line behaves.  Observing the
    // press leaves the scale's own drag gesture in charge of dragging.
    {
        let weak_scale = scale.downgrade();
        let seeker = Rc::clone(&engine);
        let seeking = Rc::clone(&seeking);
        let click = gtk::GestureClick::new();

        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_pressed(move |_, _, x, _| {
            let Some(scale) = weak_scale.upgrade() else {
                return;
            };
            let width = f64::from(scale.width());
            let upper = scale.adjustment().upper();

            if width <= 0.0 || upper <= 0.0 {
                return;
            }

            let value = (x / width).clamp(0.0, 1.0) * upper;

            seeking.set(true);
            scale.set_value(value);
            seeker.seek_to(value as i64);

            let settled = Rc::clone(&seeking);
            gtk::glib::timeout_add_local_once(Duration::from_millis(200), move || {
                settled.set(false);
            });
        });
        scale.add_controller(click);
    }

    let interval = config.interval();
    let polled = Rc::clone(&engine);
    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        polled.poll_position();

        true
    });

    schedule(interval, update)
}

/// Paint one MPRIS reading into the media card: the words, the artwork, the
/// seek line and the transport.  A card with no player keeps its shape but
/// disables the controls, so the surface does not jump when a player appears.
#[allow(clippy::too_many_arguments)]
fn paint_media(
    reading: &Reading,
    art: &gtk::glib::WeakRef<gtk::Picture>,
    artwork: &Rc<RefCell<Option<PathBuf>>>,
    title: &gtk::glib::WeakRef<gtk::Label>,
    detail: &gtk::glib::WeakRef<gtk::Label>,
    scale: &gtk::glib::WeakRef<gtk::Scale>,
    elapsed: &gtk::glib::WeakRef<gtk::Label>,
    total: &gtk::glib::WeakRef<gtk::Label>,
    controls: &gtk::glib::WeakRef<gtk::Box>,
    player: &gtk::glib::WeakRef<gtk::Label>,
    seeking: &Rc<Cell<bool>>,
) {
    let text = widgets::now_playing_text(reading);
    let length = reading.track.length_us.max(0);

    if let Some(title) = title.upgrade() {
        title.set_text(&text.title);
    }

    if let Some(detail) = detail.upgrade() {
        let detail_text = text.detail();

        detail.set_text(&detail_text);
        detail.set_visible(!detail_text.is_empty());
    }

    let wanted = reading.track.art_url.as_deref().and_then(mpris::art_path);

    if *artwork.borrow() != wanted {
        if let Some(picture) = art.upgrade() {
            if let Some(path) = wanted.as_ref() {
                picture.set_filename(Some(path.as_path()));
            }

            picture.set_visible(wanted.is_some());
        }

        *artwork.borrow_mut() = wanted;
    }

    if let Some(scale) = scale.upgrade() {
        if !seeking.get() {
            scale.set_range(0.0, length.max(1) as f64);
            scale.set_value(reading.position_us.clamp(0, length.max(1)) as f64);
        }

        scale.set_sensitive(text.available && length > 0);
    }

    if let Some(elapsed) = elapsed.upgrade() {
        elapsed.set_text(&text.position);
    }

    if let Some(total) = total.upgrade() {
        total.set_text(&text.length);
    }

    if let Some(controls) = controls.upgrade() {
        controls.set_sensitive(text.available);
    }

    if let Some(player) = player.upgrade() {
        player.set_text(if text.available { &text.player } else { "" });
    }
}

fn build_now_playing(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let glyph = label("♫", "whelk-widget-glyph", 0.0);
    let title = label("Nothing playing", "whelk-widget-player-title", 0.0);
    let detail = label("", "whelk-widget-player-detail", 0.0);
    let progress = gtk::ProgressBar::new();
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let previous = gtk::Button::with_label("⏮");
    let play = gtk::Button::with_label("⏯");
    let next = gtk::Button::with_label("⏭");
    let player = label("", "whelk-widget-footnote", 1.0);

    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    progress.add_css_class("whelk-widget-player-progress");
    progress.set_show_text(true);
    progress.set_fraction(0.0);
    progress.set_text(Some("0:00"));
    controls.add_css_class("whelk-widget-player-controls");
    controls.set_halign(gtk::Align::Center);

    for (button, tooltip) in [
        (&previous, "Previous track"),
        (&play, "Play or pause"),
        (&next, "Next track"),
    ] {
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tooltip));
        controls.append(button);
    }

    player.set_halign(gtk::Align::End);
    header.append(&glyph);
    header.append(&title);
    card.append(&header);
    card.append(&detail);
    card.append(&progress);
    card.append(&controls);
    card.append(&player);

    let weak_glyph = glyph.downgrade();
    let weak_title = title.downgrade();
    let weak_detail = detail.downgrade();
    let weak_progress = progress.downgrade();
    let weak_controls = controls.downgrade();
    let weak_player = player.downgrade();
    let callback_glyph = weak_glyph.clone();
    let callback_title = weak_title.clone();
    let callback_detail = weak_detail.clone();
    let callback_progress = weak_progress.clone();
    let callback_controls = weak_controls.clone();
    let callback_player = weak_player.clone();

    let engine = Mpris::start(config.spec.text("player"), move |reading| {
        paint_now_playing(
            reading,
            &callback_glyph,
            &callback_title,
            &callback_detail,
            &callback_progress,
            &callback_controls,
            &callback_player,
        );
    });

    paint_now_playing(
        &engine.reading(),
        &weak_glyph,
        &weak_title,
        &weak_detail,
        &weak_progress,
        &weak_controls,
        &weak_player,
    );

    for (button, control) in [
        (&previous, Control::Previous),
        (&play, Control::PlayPause),
        (&next, Control::Next),
    ] {
        let engine = Rc::clone(&engine);

        button.connect_clicked(move |_| engine.control(control));
    }

    let interval = config.interval();
    let polled = Rc::clone(&engine);
    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        polled.poll_position();

        true
    });

    schedule(interval, update)
}

/// Draw one MPRIS reading into a now-playing card.  The card's own controls
/// are disabled while there is no player to send them to.
fn paint_now_playing(
    reading: &Reading,
    glyph: &gtk::glib::WeakRef<gtk::Label>,
    title: &gtk::glib::WeakRef<gtk::Label>,
    detail: &gtk::glib::WeakRef<gtk::Label>,
    progress: &gtk::glib::WeakRef<gtk::ProgressBar>,
    controls: &gtk::glib::WeakRef<gtk::Box>,
    player: &gtk::glib::WeakRef<gtk::Label>,
) {
    let text = widgets::now_playing_text(reading);

    if let Some(glyph) = glyph.upgrade() {
        glyph.set_text(text.glyph);
    }

    if let Some(title) = title.upgrade() {
        title.set_text(&text.title);
    }

    if let Some(detail) = detail.upgrade() {
        detail.set_text(&text.detail());
        detail.set_visible(!text.detail().is_empty());
    }

    if let Some(progress) = progress.upgrade() {
        progress.set_fraction(f64::from(text.percent) / 100.0);
        progress.set_text(Some(&if text.available {
            text.timeline()
        } else {
            "0:00".to_owned()
        }));
    }

    if let Some(controls) = controls.upgrade() {
        controls.set_sensitive(text.available);
    }

    if let Some(player) = player.upgrade() {
        player.set_text(if text.available { &text.player } else { "" });
    }
}

fn build_photos(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let roots = widgets::photo_paths(
        &config
            .spec
            .text("path")
            .or_else(|| config.spec.text("paths"))
            .unwrap_or_default(),
    );
    let files = widgets::photo_files(&roots);
    let picture = gtk::Picture::new();
    let caption = label("", "whelk-widget-photo-caption", 0.0);
    let count = label("", "whelk-widget-photo-count", 1.0);
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    picture.add_css_class("whelk-widget-photo");
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_size_request(-1, 150);
    caption.set_hexpand(true);
    caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
    footer.append(&caption);
    footer.append(&count);
    card.append(&picture);
    card.append(&footer);

    if files.is_empty() {
        caption.set_text("No pictures — set #:path to a folder.");
        count.set_text("");
        picture.set_visible(false);

        return None;
    }

    let files = Rc::new(files);
    let index = Rc::new(Cell::new(0usize));
    let paint: Rc<dyn Fn()> = {
        let files = Rc::clone(&files);
        let index = Rc::clone(&index);
        let picture = picture.downgrade();
        let caption = caption.downgrade();
        let count = count.downgrade();

        Rc::new(move || {
            let (Some(picture), Some(caption), Some(count)) =
                (picture.upgrade(), caption.upgrade(), count.upgrade())
            else {
                return;
            };
            let path = &files[index.get() % files.len()];

            picture.set_filename(Some(path.as_path()));
            caption.set_text(&widgets::photo_caption(path));
            caption.set_tooltip_text(Some(&path.to_string_lossy()));
            count.set_text(&format!("{} / {}", index.get() + 1, files.len()));
        })
    };

    paint();

    {
        let files = Rc::clone(&files);
        let index = Rc::clone(&index);
        let click = gtk::GestureClick::new();

        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);

            if let Some(path) = files.get(index.get() % files.len()) {
                open_path(path);
            }
        });
        picture.add_controller(click);
    }

    {
        let pictures = Rc::clone(&files);
        let index = Rc::clone(&index);
        let repaint = Rc::clone(&paint);
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);

        scroll.connect_scroll(move |_, _, dy| {
            if dy.abs() < f64::EPSILON {
                return gtk::glib::Propagation::Proceed;
            }

            index.set(widgets::photo_step(
                index.get(),
                pictures.len(),
                if dy < 0.0 { -1 } else { 1 },
            ));
            repaint();

            gtk::glib::Propagation::Stop
        });
        picture.add_controller(scroll);
    }

    let interval = config.interval();

    if interval == 0 {
        return None;
    }

    let advance = Rc::clone(&index);
    let repaint = Rc::clone(&paint);
    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        advance.set(widgets::photo_step(advance.get(), files.len(), 1));
        repaint();

        true
    });

    Some(repeat(interval, update))
}

fn build_files(card: &gtk::Box, config: &WidgetConfig) -> Option<gtk::glib::SourceId> {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let footnote = label("", "whelk-widget-footnote", 0.0);

    list.set_vexpand(true);
    card.append(&list);
    card.append(&footnote);

    let path = config
        .spec
        .text("file")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let data_home = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from);
            let home = gtk::glib::home_dir();

            widgets::recent_xbel_path(
                data_home.as_deref(),
                (!home.as_os_str().is_empty()).then_some(home.as_path()),
            )
        });
    let limit = config
        .spec
        .integer("limit")
        .unwrap_or(i64::try_from(widgets::MAX_RECENT_FILES).unwrap_or(6))
        .clamp(1, 20) as usize;
    let interval = config.interval();
    let painted: Rc<RefCell<Vec<RecentFile>>> = Rc::new(RefCell::new(Vec::new()));
    let weak_list = list.downgrade();
    let weak_footnote = footnote.downgrade();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (Some(list), Some(footnote)) = (weak_list.upgrade(), weak_footnote.upgrade()) else {
            return false;
        };
        let files = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|xbel| widgets::parse_recent(&xbel, limit))
            .unwrap_or_default();

        if *painted.borrow() == files {
            return true;
        }

        *painted.borrow_mut() = files.clone();
        clear_box(&list);

        if files.is_empty() {
            footnote.set_text("No recent documents");

            return true;
        }

        for file in &files {
            let button = gtk::Button::new();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let title = label(&file.title, "whelk-widget-news-title", 0.0);
            let kind = label(file.kind, "whelk-widget-file-kind", 1.0);
            let uri = file.uri.clone();

            title.set_hexpand(true);
            title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            kind.set_halign(gtk::Align::End);
            row.append(&title);
            row.append(&kind);
            button.set_child(Some(&row));
            button.add_css_class("flat");
            button.add_css_class("whelk-widget-file-row");
            button.set_tooltip_text(Some(&uri));
            button.connect_clicked(move |_| open_uri(&uri));
            list.append(&button);
        }

        footnote.set_text(&format!(
            "{} recent document{}",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        ));

        true
    });

    schedule(interval, update)
}

fn build_news(card: &gtk::Box, config: &WidgetConfig) -> Vec<gtk::glib::SourceId> {
    let feeds = widgets::feed_urls(
        &config
            .spec
            .text("feeds")
            .or_else(|| config.spec.text("url"))
            .unwrap_or_default(),
    );
    let limit = config
        .spec
        .integer("limit")
        .unwrap_or(i64::try_from(widgets::MAX_NEWS_ITEMS).unwrap_or(6))
        .clamp(1, 12) as usize;
    let interval = config.interval();
    let health_interval = u64::from(if interval == 0 { 1800 } else { interval });
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let freshness = label("", "whelk-widget-subtle", 0.0);
    let footnote = label("", "whelk-widget-footnote", 0.0);

    list.set_vexpand(true);
    card.append(&list);
    card.append(&freshness);
    card.append(&footnote);

    if feeds.is_empty() {
        footnote.set_text("No headlines — set #:feeds to a feed URL.");

        return Vec::new();
    }

    let busy = Rc::new(Cell::new(false));
    let health: Rc<Cell<Option<Health>>> = Rc::new(Cell::new(None));
    let generation = Rc::new(Cell::new(0u64));
    let painted: Rc<RefCell<Vec<NewsItem>>> = Rc::new(RefCell::new(Vec::new()));
    let (update_health, tick_health) = (Rc::clone(&health), Rc::clone(&health));
    let configured_tooltip = config.tooltip();
    let tick_tooltip = configured_tooltip.clone();
    let own_tooltip = configured_tooltip;
    let weak_list = list.downgrade();
    let weak_freshness = freshness.downgrade();
    let weak_footnote = footnote.downgrade();
    let weak_card = card.downgrade();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        // The card is the liveness token: every label the answer will touch is
        // upgraded again when the answer arrives.
        let (Some(_list), Some(_freshness), Some(_footnote), Some(_card)) = (
            weak_list.upgrade(),
            weak_freshness.upgrade(),
            weak_footnote.upgrade(),
            weak_card.upgrade(),
        ) else {
            return false;
        };

        if busy.replace(true) {
            return true;
        }

        let run = generation.get() + 1;

        generation.set(run);

        let collected = Rc::new(RefCell::new(Vec::new()));
        let successes = Rc::new(Cell::new(0usize));
        let collected_for_done = Rc::clone(&collected);
        let done_successes = Rc::clone(&successes);
        let done_generation = Rc::clone(&generation);
        let done_busy = Rc::clone(&busy);
        let done_health = Rc::clone(&update_health);
        let done_painted = Rc::clone(&painted);
        let done_list = weak_list.clone();
        let done_freshness = weak_freshness.clone();
        let done_footnote = weak_footnote.clone();
        let done_card = weak_card.clone();
        let done_limit = limit;
        let done_tooltip = own_tooltip.clone();

        let done: Rc<dyn Fn(bool)> = Rc::new(move |any_read: bool| {
            if done_generation.get() != run {
                return;
            }

            done_busy.set(false);

            let (Some(list), Some(freshness), Some(footnote), Some(card)) = (
                done_list.upgrade(),
                done_freshness.upgrade(),
                done_footnote.upgrade(),
                done_card.upgrade(),
            ) else {
                return;
            };
            let mut items = collected_for_done.borrow().clone();

            items.truncate(done_limit);

            if *done_painted.borrow() != items {
                paint_news(&list, &items);
                *done_painted.borrow_mut() = items.clone();
            }

            let read = done_successes.get();

            footnote.set_text(&if items.is_empty() {
                "No headlines".to_owned()
            } else {
                format!(
                    "{} headline{} from {} feed{}",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" },
                    read,
                    if read == 1 { "" } else { "s" }
                )
            });

            let now = now_seconds();
            let previous = done_health.get();
            let outcome = if any_read {
                Ok(())
            } else {
                Err(FetchError::Failed)
            };
            let next = Health::after(previous, outcome, now);

            done_health.set(Some(next));
            paint_health(
                &card,
                &freshness,
                Some(next),
                health_interval,
                "News",
                done_tooltip.as_deref(),
            );
        });

        fetch_feeds(feeds.clone(), 0, collected, successes, done);

        true
    });

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
            health_interval,
            "News",
            tick_tooltip.as_deref(),
        );

        true
    });
    let mut timers = Vec::new();

    if let Some(timer) = schedule(interval, update) {
        timers.push(timer);
    }

    timers.push(repeat(FRESHNESS_TICK, ticker));

    timers
}

/// Read every feed in turn and hand the merged headlines to `done`.
///
/// One `curl` per feed, one at a time: a card is not worth four concurrent
/// processes, and the answers arrive in the configured order.
fn fetch_feeds(
    feeds: Vec<String>,
    index: usize,
    collected: Rc<RefCell<Vec<NewsItem>>>,
    successes: Rc<Cell<usize>>,
    done: Rc<dyn Fn(bool)>,
) {
    let Some(url) = feeds.get(index).cloned() else {
        done(successes.get() > 0);

        return;
    };
    let source = feed_label(&url);
    let next_collected = Rc::clone(&collected);
    let next_successes = Rc::clone(&successes);
    let next_done = Rc::clone(&done);

    fetch(url, Rc::new(Cell::new(false)), move |answer| {
        if let Ok(text) = answer {
            let items = widgets::parse_feed(&text, &source, widgets::MAX_NEWS_ITEMS);

            if !items.is_empty() {
                next_successes.set(next_successes.get() + 1);
                next_collected.borrow_mut().extend(items);
            }
        }

        fetch_feeds(feeds, index + 1, next_collected, next_successes, next_done);
    });
}

fn paint_news(list: &gtk::Box, items: &[NewsItem]) {
    clear_box(list);

    for item in items {
        let button = gtk::Button::new();
        let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = label(&item.title, "whelk-widget-news-title", 0.0);
        let source = label(&item.source, "whelk-widget-news-source", 0.0);
        let link = item.link.clone();

        // One line per headline: six rows have to fit the standard card, and
        // the full sentence is in the tooltip when the row ellipsizes it.
        title.set_wrap(false);
        title.set_lines(1);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&title);
        row.append(&source);
        button.set_child(Some(&row));
        button.add_css_class("flat");
        button.add_css_class("whelk-widget-news-row");
        button.set_tooltip_text(Some(&format!(
            "{}\n{}{}",
            item.title,
            item.link,
            if item.published.is_empty() {
                String::new()
            } else {
                format!("\n{}", item.published)
            }
        )));

        button.connect_clicked(move |_| open_uri(&link));
        list.append(&button);
    }
}

fn feed_label(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);

    host.strip_prefix("www.").unwrap_or(host).to_owned()
}

fn build_system(card: &gtk::Box) -> Option<gtk::glib::SourceId> {
    let cpu_value = label("—", "whelk-widget-system-value", 1.0);
    let cpu_bar = gtk::LevelBar::for_interval(0.0, 100.0);
    let memory_value = label("—", "whelk-widget-system-value", 1.0);
    let memory_bar = gtk::LevelBar::for_interval(0.0, 100.0);
    let load_value = label("—", "whelk-widget-system-value", 1.0);
    let uptime_value = label("—", "whelk-widget-system-value", 1.0);
    let cpu_name = label("CPU", "whelk-widget-system-name", 0.0);
    let memory_name = label("Memory", "whelk-widget-system-name", 0.0);
    let load_name = label("Load", "whelk-widget-system-name", 0.0);
    let uptime_name = label("Uptime", "whelk-widget-system-name", 0.0);
    let cpu_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let memory_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let load_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let uptime_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    for bar in [&cpu_bar, &memory_bar] {
        bar.add_css_class("whelk-widget-battery-level");
        bar.set_hexpand(true);
        bar.set_value(0.0);
    }

    for (row, name, value) in [
        (&cpu_row, &cpu_name, &cpu_value),
        (&memory_row, &memory_name, &memory_value),
        (&load_row, &load_name, &load_value),
        (&uptime_row, &uptime_name, &uptime_value),
    ] {
        row.add_css_class("whelk-widget-system-row");
        name.set_hexpand(true);
        value.set_halign(gtk::Align::End);
        row.append(name);
        row.append(value);
    }

    card.append(&cpu_row);
    card.append(&cpu_bar);
    card.append(&memory_row);
    card.append(&memory_bar);
    card.append(&load_row);
    card.append(&uptime_row);

    let sampler = Rc::new(RefCell::new(cpubar::CpuSampler::new()));
    let weak_cpu = cpu_value.downgrade();
    let weak_memory = memory_value.downgrade();
    let weak_load = load_value.downgrade();
    let weak_uptime = uptime_value.downgrade();
    let weak_cpu_bar = cpu_bar.downgrade();
    let weak_memory_bar = memory_bar.downgrade();

    let update: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let (
            Some(cpu_value),
            Some(memory_value),
            Some(load_value),
            Some(uptime_value),
            Some(cpu_bar),
            Some(memory_bar),
        ) = (
            weak_cpu.upgrade(),
            weak_memory.upgrade(),
            weak_load.upgrade(),
            weak_uptime.upgrade(),
            weak_cpu_bar.upgrade(),
            weak_memory_bar.upgrade(),
        )
        else {
            return false;
        };

        if let Some(usage) = sampler.borrow_mut().usage() {
            let text = format!("{usage:.0}%");

            if cpu_value.text() != text {
                cpu_value.set_text(&text);
            }

            cpu_bar.set_value(usage);
        }

        if let Some(info) = memorybar::read() {
            let percent = f64::from(info.percentage());
            let text = format!("{percent:.0}%");

            if memory_value.text() != text {
                memory_value.set_text(&text);
            }

            memory_bar.set_value(percent);
            memory_value
                .set_tooltip_text(Some(&widgets::memory_text(info.used_kb(), info.total_kb)));
        }

        if let Some(load) = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|text| widgets::parse_loadavg(&text))
        {
            let text = widgets::load_text(load);

            if load_value.text() != text {
                load_value.set_text(&text);
            }
        }

        if let Some(uptime) = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|text| widgets::parse_uptime(&text))
        {
            let text = widgets::uptime_text(uptime);

            if uptime_value.text() != text {
                uptime_value.set_text(&text);
            }
        }

        true
    });

    schedule(5, update)
}

/// Open a URI with whatever the desktop uses for it.  A failure is a log line,
/// not a dialog: a card has nowhere to put one.
fn open_uri(uri: &str) {
    if uri.is_empty() {
        return;
    }

    if let Err(error) =
        gtk::gio::AppInfo::launch_default_for_uri(uri, None::<&gtk::gio::AppLaunchContext>)
    {
        eprintln!("whelk: could not open {uri}: {error}");
    }
}

fn open_path(path: &Path) {
    match gtk::glib::filename_to_uri(path, None) {
        Ok(uri) => open_uri(&uri),
        Err(error) => eprintln!("whelk: could not name {}: {error}", path.display()),
    }
}

fn build_stocks(
    card: &gtk::Box,
    config: &WidgetConfig,
    style: &BarSettings,
) -> Vec<gtk::glib::SourceId> {
    let symbols =
        widgets::stock_symbols(config.spec.text("symbols").as_deref().unwrap_or_default());
    let colour = ink(style);
    let mut rows = Vec::new();

    for symbol in &symbols {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = label(symbol, "whelk-widget-stock-symbol", 0.0);
        let price = label("—", "whelk-widget-stock-price", 1.0);
        let change = label("", "whelk-widget-stock-change", 1.0);
        let spark = gtk::DrawingArea::new();
        let spark_values: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
        let drawn = Rc::clone(&spark_values);

        spark.add_css_class("whelk-widget-spark");
        spark.set_content_width(54);
        spark.set_content_height(18);
        spark.set_size_request(54, 18);
        spark.set_valign(gtk::Align::Center);
        spark.set_draw_func(move |_, cairo, width, height| {
            draw_sparkline(cairo, width, height, &drawn.borrow(), colour);
        });

        // Fixed columns so the names, prices and changes line up down the card
        // instead of each row finding its own width.
        name.set_hexpand(true);
        price.set_width_chars(9);
        change.set_width_chars(8);
        line.add_css_class("whelk-widget-stock-row");
        line.append(&name);
        line.append(&spark);
        line.append(&price);
        line.append(&change);
        change.set_halign(gtk::Align::End);
        card.append(&line);
        rows.push((
            symbol.clone(),
            price.downgrade(),
            change.downgrade(),
            spark.downgrade(),
            spark_values,
        ));
    }

    let footnote = label("", "whelk-widget-footnote", 0.0);
    // The quotes' age, written only once they have gone old.
    let freshness = label("", "whelk-widget-subtle", 0.0);

    freshness.set_visible(false);
    card.append(&footnote);
    card.append(&freshness);

    let url = widgets::stocks_url(&symbols);
    let interval = config.interval();
    let health_interval = u64::from(if interval == 0 {
        STOCKS_INTERVAL
    } else {
        interval
    });
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

        let quoted: Vec<QuoteRow> = rows
            .iter()
            .filter_map(|(symbol, price, change, spark, values)| {
                Some((
                    symbol.clone(),
                    price.upgrade()?,
                    change.upgrade()?,
                    spark.upgrade()?,
                    Rc::clone(values),
                ))
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
                health_interval,
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
            health_interval,
            "Quotes",
            tick_tooltip.as_deref(),
        );

        true
    });
    let mut timers = Vec::new();

    if let Some(timer) = schedule(interval, update) {
        timers.push(timer);
    }

    timers.push(repeat(FRESHNESS_TICK, ticker));

    timers
}

/// One stock row's live parts: symbol, price, change, sparkline and its values.
type QuoteRow = (
    String,
    gtk::Label,
    gtk::Label,
    gtk::DrawingArea,
    Rc<RefCell<Vec<f64>>>,
);

fn apply_quotes(rows: &[QuoteRow], quotes: &[StockQuote]) {
    for (symbol, price, change, spark, values) in rows {
        let Some(quote) = quotes.iter().find(|quote| &quote.symbol == symbol) else {
            continue;
        };

        price.set_text(&quote.price_text());
        change.set_text(&quote.change_text());
        change.remove_css_class("up");
        change.remove_css_class("down");
        change.add_css_class(if quote.is_up() { "up" } else { "down" });
        *values.borrow_mut() = quote.spark.clone();
        spark.queue_draw();
    }
}

/// A day's shape, drawn in the reading's ink: green when the last point is
/// above the first, red when it is below, and the card's own ink for a flat
/// line that has no direction.
fn draw_sparkline(
    cairo: &gtk::cairo::Context,
    width: i32,
    height: i32,
    values: &[f64],
    ink: gtk::gdk::RGBA,
) {
    let points = widgets::sparkline_points(
        values,
        f64::from((width - 2).max(1)),
        f64::from((height - 4).max(1)),
    );

    if points.len() < 2 {
        return;
    }

    let first = values.first().copied().unwrap_or(0.0);
    let last = values.last().copied().unwrap_or(0.0);

    if last > first {
        cairo.set_source_rgba(0.29, 0.87, 0.50, 0.9);
    } else if last < first {
        cairo.set_source_rgba(0.97, 0.45, 0.45, 0.9);
    } else {
        cairo.set_source_rgba(
            f64::from(ink.red()),
            f64::from(ink.green()),
            f64::from(ink.blue()),
            0.7,
        );
    }

    cairo.set_line_width(1.4);
    cairo.translate(1.0, 2.0);
    cairo.move_to(points[0].0, points[0].1);

    for point in &points[1..] {
        cairo.line_to(point.0, point.1);
    }

    let _ = cairo.stroke();
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

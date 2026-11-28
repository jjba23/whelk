// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The desktop widgets' data, ported from the display-free half of
//! `src/widgets.c` and the the desktop settings records behind it.
//!
//! A widget is a floating card described by the same settings file as the bar:
//! a kind, a pixel position and size, a monitor, and an option alist.  This
//! module owns everything about that description that does not need a display:
//! the kind catalogue and its standard sizes, the geometry rules, the pure data
//! transforms each kind performs (weather and stock answers, the calendar's
//! month grid, a world clock's cities, a note's text, a command's reading) and
//! the reminders model whose done state is runtime state rather than
//! configuration.
//!
//! The surfaces, the drag gesture and the refreshers stay with GTK in
//! [`crate::widgetstate`]'s callers; what is here is what they all agree on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk4 as gtk;

use crate::format;
use crate::json::Json;
use crate::mpris::{self, Reading};
use crate::skin::Skin;

/// A Scheme value as Rust data, for the options a widget reads.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemeValue {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Symbol(String),
    List(Vec<SchemeValue>),
    Alist(Vec<(String, SchemeValue)>),
    Other,
}

/// The kind and options of one widget, the shape a bar module shares.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModuleSpec {
    pub kind: String,
    pub options: BTreeMap<String, SchemeValue>,
}

impl ModuleSpec {
    pub fn text(&self, key: &str) -> Option<String> {
        match self.options.get(key) {
            Some(SchemeValue::Text(value) | SchemeValue::Symbol(value)) => Some(value.clone()),
            _ => None,
        }
    }

    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.options.get(key) {
            Some(SchemeValue::Integer(value)) => Some(*value),
            Some(SchemeValue::Number(value)) if value.is_finite() && value.fract() == 0.0 => {
                Some(*value as i64)
            }
            _ => None,
        }
    }

    pub fn boolean(&self, key: &str) -> Option<bool> {
        match self.options.get(key) {
            Some(SchemeValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }
}

/// Where a widget goes when the settings file names no position.
pub const DEFAULT_X: i32 = 48;
pub const DEFAULT_Y: i32 = 64;

/// The smallest card a settings file can ask for.
pub const MIN_SIZE: i32 = 100;

/// The most reminder rows a card draws.
pub const MAX_REMINDERS: usize = 8;

/// The most stock rows a card draws.
pub const MAX_STOCKS: usize = 6;

/// The per-widget state key the reminders' done flags live under.
pub const REMINDER_STATE_KEY: &str = "items";

/// The reminders a card starts with when neither state nor settings name any.
pub const DEFAULT_REMINDERS: &str = "Review the release,Water the plants,Read the manual";

/// The cities a world clock starts with.
pub const DEFAULT_CITIES: &str =
    "Cupertino=America/Los_Angeles,London=Europe/London,Tokyo=Asia/Tokyo";

/// The symbols a stock card starts with.
pub const DEFAULT_SYMBOLS: &str = "AAPL,MSFT,NVDA";

/// The image extensions a photo card can draw.
pub const PHOTO_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif"];

/// The most recent documents a Files card lists.
pub const MAX_RECENT_FILES: usize = 6;

/// The most headlines a News card lists.
pub const MAX_NEWS_ITEMS: usize = 6;

/// The most feeds one News card reads in a refresh.
pub const MAX_FEEDS: usize = 4;

/// The bars a visualizer asks cava for when the card names no count.
pub const VISUALIZER_BARS: usize = 32;

/// The most bars a visualizer draws, so a wild setting cannot ask cava for a
/// thousand columns or a card for a thousand rectangles.
pub const VISUALIZER_MAX_BARS: usize = 64;

/// The frames per second a visualizer asks cava for when the card names none.
pub const VISUALIZER_FRAMERATE: u32 = 30;

/// The note a Notes card shows when it was given nothing else.
pub const DEFAULT_NOTE: &str = "No note yet.\nSet #:text or #:file on this widget.";

/// The forecast request's fixed half.
pub const WEATHER_URL: &str = "https://api.open-meteo.com/v1/forecast";

/// The quote request's fixed half.
pub const STOCKS_URL: &str = "https://query1.finance.yahoo.com/v8/finance/spark";

/// The grid a card's size is snapped to, in pixels -- the same one a drag
/// snaps to, so a family size and a dragged card land on the same lines.
pub const GRID: i32 = 8;

/// The three card families.  A card's family scales the size its kind
/// takes by default, which is how a settings file says "the big weather card"
/// without writing pixels that then have to be maintained by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeFamily {
    Small,
    #[default]
    Medium,
    Large,
}

impl SizeFamily {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// The multiplier a family applies to its kind's standard size.
    pub fn scale(self) -> f64 {
        match self {
            Self::Small => 0.75,
            Self::Medium => 1.0,
            Self::Large => 1.5,
        }
    }
}

fn snapped(value: i32) -> i32 {
    let steps = (f64::from(value) / f64::from(GRID)).round() as i32;

    (steps * GRID).max(MIN_SIZE)
}

/// A card's size in FAMILY: its kind's standard size scaled, snapped to the
/// grid and never smaller than [`MIN_SIZE`].
pub fn family_size(standard: (i32, i32), family: SizeFamily) -> (i32, i32) {
    // Medium is the kind's own size, untouched: snapping it would move every
    // card anyone already placed, for no gain.  The other two families are
    // new sizes, so they land on the grid a drag snaps to.
    if family == SizeFamily::Medium {
        return (standard.0.max(MIN_SIZE), standard.1.max(MIN_SIZE));
    }

    let scale = family.scale();

    (
        snapped((f64::from(standard.0) * scale).round() as i32),
        snapped((f64::from(standard.1) * scale).round() as i32),
    )
}

/// Where a card is pinned.  A pinned card keeps its distance from its edges
/// when the output is a different size, which is what a card in a corner is
/// for: `top-right` at 24 pixels is 24 pixels from the right on any screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "top-left" => Some(Self::TopLeft),
            "top" => Some(Self::Top),
            "top-right" => Some(Self::TopRight),
            "left" => Some(Self::Left),
            "center" | "centre" => Some(Self::Center),
            "right" => Some(Self::Right),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom" => Some(Self::Bottom),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::Top => "top",
            Self::TopRight => "top-right",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::BottomLeft => "bottom-left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom-right",
        }
    }

    /// How the card sits on each axis: 0 is the near edge (left/top), 1 the
    /// far edge (right/bottom) and 2 the centre.
    fn axes(self) -> (u8, u8) {
        match self {
            Self::TopLeft => (0, 0),
            Self::Top => (2, 0),
            Self::TopRight => (1, 0),
            Self::Left => (0, 2),
            Self::Center => (2, 2),
            Self::Right => (1, 2),
            Self::BottomLeft => (0, 1),
            Self::Bottom => (2, 1),
            Self::BottomRight => (1, 1),
        }
    }
}

fn place(near: u8, area: i32, offset: i32, size: i32) -> i32 {
    match near {
        0 => offset,
        1 => area - size - offset,
        _ => (area - size) / 2 + offset,
    }
}

fn unplace(near: u8, area: i32, origin: i32, size: i32) -> i32 {
    match near {
        0 => origin,
        1 => area - size - origin,
        _ => origin - (area - size) / 2,
    }
}

/// The top-left origin a card has when it is pinned to ANCHOR at OFFSET.
///
/// OFFSET is the card's distance from its anchor's edges: `(24, 24)` on
/// `bottom-right` is 24 pixels in from the corner, and the same value is what a
/// drag saves, so a card keeps its place when a monitor is a different size.
pub fn anchored_origin(
    anchor: Anchor,
    area: Area,
    offset: (i32, i32),
    size: (i32, i32),
) -> (i32, i32) {
    let (horizontal, vertical) = anchor.axes();

    (
        place(horizontal, area.width, offset.0, size.0),
        place(vertical, area.height, offset.1, size.1),
    )
}

/// The inverse of [`anchored_origin`]: the offset a dragged card is saved as.
pub fn anchored_offset(
    anchor: Anchor,
    area: Area,
    origin: (i32, i32),
    size: (i32, i32),
) -> (i32, i32) {
    let (horizontal, vertical) = anchor.axes();

    (
        unplace(horizontal, area.width, origin.0, size.0),
        unplace(vertical, area.height, origin.1, size.1),
    )
}

/// The colours and finish a card can wear instead of the bar's.
///
/// A widget is skinned from the first bar's palette so the desktop looks like
/// one shell, but a card that sits over a photo may want to be quieter or
/// warmer than the bar upstairs.  Every field is optional: a card that names
/// none is exactly the bar's skin, and a card that names one changes one thing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WidgetSkin {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub accent: Option<String>,
    pub opacity: Option<f64>,
    pub blur: Option<i32>,
    pub radius: Option<i32>,
    pub shine: Option<f64>,
    pub reflection: Option<f64>,
    pub flare: Option<f64>,
}

impl WidgetSkin {
    /// Read the skin keys out of a widget's options; `None` when it named none.
    pub fn from_spec(spec: &ModuleSpec) -> Option<Self> {
        let number = |key: &str| match spec.options.get(key) {
            Some(SchemeValue::Number(value)) if value.is_finite() => Some(*value),
            Some(SchemeValue::Integer(value)) => Some(*value as f64),
            _ => None,
        };
        let skin = Self {
            foreground: spec.text("foreground"),
            background: spec.text("background"),
            accent: spec.text("accent"),
            opacity: number("opacity").map(|value| value.clamp(0.0, 1.0)),
            blur: spec.integer("blur").map(|value| value.clamp(0, 64) as i32),
            radius: spec
                .integer("radius")
                .map(|value| value.clamp(0, 64) as i32),
            shine: number("shine").map(|value| value.clamp(0.0, 1.0)),
            reflection: number("reflection").map(|value| value.clamp(0.0, 1.0)),
            flare: number("flare").map(|value| value.clamp(0.0, 1.0)),
        };

        if skin == Self::default() {
            None
        } else {
            Some(skin)
        }
    }

    /// Lay this skin over the host's material.
    pub fn apply(&self, skin: &mut Skin) {
        if let Some(value) = self.foreground.as_ref() {
            skin.foreground = Some(value.clone());
        }

        if let Some(value) = self.background.as_ref() {
            skin.background = Some(value.clone());
        }

        if let Some(value) = self.accent.as_ref() {
            skin.accent = Some(value.clone());
        }

        if let Some(value) = self.opacity {
            skin.opacity = value;
        }

        if let Some(value) = self.blur {
            skin.blur = value;
        }

        if let Some(value) = self.radius {
            skin.radius = value;
        }

        if let Some(value) = self.shine {
            skin.shine = value;
        }

        if let Some(value) = self.reflection {
            skin.reflection = value;
        }

        if let Some(value) = self.flare {
            skin.flare = value;
        }
    }
}

/// One of the card kinds the factory table knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Clock,
    Calendar,
    Weather,
    Battery,
    Notes,
    Reminders,
    WorldClock,
    Stocks,
    NowPlaying,
    Visualizer,
    Media,
    Photos,
    Files,
    News,
    System,
    Custom,
}

impl WidgetKind {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "clock" => Some(Self::Clock),
            "calendar" => Some(Self::Calendar),
            "weather" => Some(Self::Weather),
            "battery" => Some(Self::Battery),
            "notes" => Some(Self::Notes),
            "reminders" => Some(Self::Reminders),
            "world-clock" => Some(Self::WorldClock),
            "stocks" => Some(Self::Stocks),
            "now-playing" | "music" => Some(Self::NowPlaying),
            "visualizer" | "spectrum" => Some(Self::Visualizer),
            "media" | "media-controls" => Some(Self::Media),
            "photos" => Some(Self::Photos),
            "files" => Some(Self::Files),
            "news" => Some(Self::News),
            "system" => Some(Self::System),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::Calendar => "calendar",
            Self::Weather => "weather",
            Self::Battery => "battery",
            Self::Notes => "notes",
            Self::Reminders => "reminders",
            Self::WorldClock => "world-clock",
            Self::Stocks => "stocks",
            Self::NowPlaying => "now-playing",
            Self::Visualizer => "visualizer",
            Self::Media => "media",
            Self::Photos => "photos",
            Self::Files => "files",
            Self::News => "news",
            Self::System => "system",
            Self::Custom => "custom",
        }
    }

    /// The standard size this kind's card gets, from `%widget-sizes` in
    /// `(whelk state)`.  A kind of the configuration's own
    /// gets the generic card size.
    pub fn standard_size(self) -> (i32, i32) {
        match self {
            Self::Clock => (220, 240),
            Self::Calendar => (340, 330),
            Self::Weather => (340, 220),
            Self::Battery => (240, 180),
            Self::Notes => (340, 260),
            Self::Reminders => (340, 300),
            Self::WorldClock => (260, 260),
            Self::Stocks => (340, 280),
            Self::NowPlaying => (340, 220),
            Self::Visualizer => (360, 220),
            Self::Media => (360, 260),
            Self::Photos => (280, 260),
            Self::Files => (320, 260),
            Self::News => (340, 300),
            Self::System => (280, 280),
            Self::Custom => (320, 220),
        }
    }

    /// The size this kind takes in FAMILY.
    pub fn family_size(self, family: SizeFamily) -> (i32, i32) {
        family_size(self.standard_size(), family)
    }

    pub fn default_title(self) -> &'static str {
        match self {
            Self::Clock => "Clock",
            Self::Calendar => "Calendar",
            Self::Weather => "Weather",
            Self::Battery => "Batteries",
            Self::Notes => "Notes",
            Self::Reminders => "Reminders",
            Self::WorldClock => "World Clock",
            Self::Stocks => "Stocks",
            Self::NowPlaying => "Now Playing",
            Self::Visualizer => "Visualizer",
            Self::Media => "Media",
            Self::Photos => "Photos",
            Self::Files => "Files",
            Self::News => "News",
            Self::System => "System",
            Self::Custom => "Widget",
        }
    }

    /// The glyph the card's header wears.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Clock => "◷",
            Self::Calendar => "▦",
            Self::Weather => "☁",
            Self::Battery => "▮",
            Self::Notes => "✎",
            Self::Reminders => "☑",
            Self::WorldClock => "⊕",
            Self::Stocks => "↗",
            Self::NowPlaying => "♫",
            Self::Visualizer => "≋",
            Self::Media => "⏯",
            Self::Photos => "◫",
            Self::Files => "▤",
            Self::News => "≣",
            Self::System => "⚙",
            Self::Custom => "◌",
        }
    }

    /// The refresh cadence the C module starts with.  Zero means the card is
    /// painted once and has no timer.
    pub fn interval(self) -> u32 {
        match self {
            Self::Clock => 1,
            Self::Calendar => 60,
            Self::Weather => 1800,
            Self::Battery => 30,
            Self::Notes | Self::Reminders => 0,
            Self::WorldClock => 30,
            Self::Stocks | Self::Custom => 300,
            Self::NowPlaying => 5,
            Self::Visualizer => 0,
            Self::Media => 1,
            Self::Photos => 30,
            Self::Files => 60,
            Self::News => 1800,
            Self::System => 5,
        }
    }
}

/// One floating card, as the settings file described it.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetConfig {
    /// The kind and its options, the same shape a bar module has.
    pub spec: ModuleSpec,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// `"default"` or a connector name such as `"DP-1"`.
    pub monitor: String,
    /// The family the card belongs to: what a bare size is derived from.
    pub size: SizeFamily,
    /// The edge the card is pinned to.
    pub anchor: Anchor,
}

impl WidgetConfig {
    /// Apply the `(desktop-widget ...)` defaults: kind sizes when the settings
    /// file gave none, a non-negative origin, a card at least
    /// [`MIN_SIZE`] in each direction, and `"default"` for a monitor nobody
    /// named.
    pub fn new(
        spec: ModuleSpec,
        x: i32,
        y: i32,
        width: Option<i32>,
        height: Option<i32>,
        monitor: &str,
    ) -> Self {
        let kind = WidgetKind::from_name(&spec.kind);
        let (standard_width, standard_height) =
            kind.map(WidgetKind::standard_size).unwrap_or((320, 220));

        Self {
            spec,
            x: x.max(0),
            y: y.max(0),
            width: width.unwrap_or(standard_width).max(MIN_SIZE),
            height: height.unwrap_or(standard_height).max(MIN_SIZE),
            monitor: if monitor.is_empty() {
                "default".to_owned()
            } else {
                monitor.to_owned()
            },
            size: SizeFamily::Medium,
            anchor: Anchor::TopLeft,
        }
    }

    /// The card's size in FAMILY, for a builder that asks before it has pixels.
    pub fn in_family(mut self, family: SizeFamily) -> Self {
        if let Some(kind) = WidgetKind::from_name(&self.spec.kind) {
            let (width, height) = kind.family_size(family);

            self.width = width;
            self.height = height;
        }

        self.size = family;

        self
    }

    /// The card pinned to ANCHOR.
    pub fn anchored(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;

        self
    }

    /// The card's own material, when it named one.
    pub fn skin(&self) -> Option<WidgetSkin> {
        WidgetSkin::from_spec(&self.spec)
    }

    /// Read one `shell-widgets->alists` row.
    pub fn from_alist(entries: &[(String, SchemeValue)]) -> Option<Self> {
        let mut kind = None;
        let mut x = DEFAULT_X;
        let mut y = DEFAULT_Y;
        let mut width = None;
        let mut height = None;
        let mut monitor = "default".to_owned();
        let mut size = SizeFamily::Medium;
        let mut anchor = Anchor::TopLeft;
        let mut options = std::collections::BTreeMap::new();

        for (key, value) in entries {
            match key.as_str() {
                "kind" => kind = value_text(value),
                "x" => x = value_int(value).unwrap_or(x),
                "y" => y = value_int(value).unwrap_or(y),
                "width" => width = value_int(value),
                "height" => height = value_int(value),
                "monitor" => monitor = value_text(value).unwrap_or(monitor),
                "size" => {
                    size = value_text(value)
                        .as_deref()
                        .and_then(SizeFamily::from_name)
                        .unwrap_or(SizeFamily::Medium);
                }
                "anchor" => {
                    anchor = value_text(value)
                        .as_deref()
                        .and_then(Anchor::from_name)
                        .unwrap_or(Anchor::TopLeft);
                }
                "options" => {
                    if let SchemeValue::Alist(values) = value {
                        options = values.iter().cloned().collect();
                    }
                }
                _ => {}
            }
        }

        let mut card = Self::new(
            ModuleSpec {
                kind: kind?,
                options,
            },
            x,
            y,
            width,
            height,
            &monitor,
        );

        // A family only decides the size nobody wrote down: an explicit
        // width or height is the configuration's word.
        if width.is_none() && height.is_none() {
            card = card.in_family(size);
        } else {
            card.size = size;
        }

        Some(card.anchored(anchor))
    }

    pub fn kind(&self) -> Option<WidgetKind> {
        WidgetKind::from_name(&self.spec.kind)
    }

    /// The runtime identity of the card: its kind and the index it was built
    /// at, which is what the state file keys a position under.
    pub fn id(&self, index: usize) -> String {
        format!("widget-{}-{index}", self.spec.kind)
    }

    /// The CSS class every card of this kind wears.
    pub fn object_class(&self) -> String {
        format!("whelk-widget-{}", self.spec.kind)
    }

    pub fn title(&self) -> String {
        self.spec
            .text("title")
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| {
                self.kind()
                    .map(WidgetKind::default_title)
                    .unwrap_or("Widget")
                    .to_owned()
            })
    }

    pub fn tooltip(&self) -> Option<String> {
        self.spec.text("tooltip").filter(|text| !text.is_empty())
    }

    pub fn css(&self) -> Option<String> {
        self.spec.text("css").filter(|css| !css.is_empty())
    }

    /// The mouse vocabulary a bar module has, from the same option names.
    pub fn click_commands(&self) -> Vec<(&'static str, String)> {
        [
            "on-click",
            "on-click-middle",
            "on-click-right",
            "on-scroll-up",
            "on-scroll-down",
        ]
        .into_iter()
        .filter_map(|key| {
            self.spec
                .text(key)
                .filter(|command| !command.is_empty())
                .map(|command| (key, command))
        })
        .collect()
    }

    /// The card's own refresh cadence.  Notes reads its file every 30 seconds
    /// when it was given one, and a custom card never refreshes faster than
    /// five seconds.
    pub fn interval(&self) -> u32 {
        // An explicit `interval` wins over the kind's cadence: a card that says
        // how often to read is not overruled by what its kind usually does.  A
        // custom card keeps its floor, because a command every second is a
        // process every second.
        if let Some(interval) = self.spec.integer("interval") {
            if self.kind() == Some(WidgetKind::Custom) {
                return custom_interval(Some(interval));
            }

            return if interval > 0 {
                interval.min(i64::from(u32::MAX)) as u32
            } else {
                0
            };
        }

        match self.kind() {
            Some(WidgetKind::Notes) => {
                if self.spec.text("file").is_some() {
                    30
                } else {
                    0
                }
            }
            Some(WidgetKind::Custom) => custom_interval(self.spec.integer("interval")),
            Some(kind) => kind.interval(),
            None => 0,
        }
    }

    /// The position this card should be drawn at: a saved drag wins over the
    /// settings file, and a saved position is clamped into the monitor again
    /// because the screen may have shrunk or been replaced since.
    pub fn restore_position(&self, saved: Option<(i32, i32)>, area: Area) -> (i32, i32) {
        // A saved position is an *offset from the anchor*, which is what makes
        // a card in a corner stay in that corner on a differently sized screen.
        let offset = saved.unwrap_or((self.x, self.y));
        let (x, y) = anchored_origin(self.anchor, area, offset, (self.width, self.height));

        clamp_into(x, y, self.width, self.height, area)
    }
}

/// The monitor a drag is allowed across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub width: i32,
    pub height: i32,
}

/// The C module's fallback when the display has no monitor model.
pub const FALLBACK_AREA: Area = Area {
    width: 1920,
    height: 1080,
};

pub fn clamp_into(x: i32, y: i32, width: i32, height: i32, area: Area) -> (i32, i32) {
    crate::widgetstate::clamp(x, y, width, height, area.width, area.height)
}

/// Where a drag has moved a card to: the snapped pointer offset from where the
/// drag started, kept inside the monitor.  `offset` is the gesture's pixel
/// delta, which is fractional.
pub fn drag_position(
    origin: (i32, i32),
    offset: (f64, f64),
    size: (i32, i32),
    area: Area,
) -> (i32, i32) {
    let x = crate::widgetstate::snap(
        (f64::from(origin.0) + offset.0).round() as i32,
        crate::widgetstate::GRID,
    );
    let y = crate::widgetstate::snap(
        (f64::from(origin.1) + offset.1).round() as i32,
        crate::widgetstate::GRID,
    );

    clamp_into(x, y, size.0, size.1, area)
}

/// The monitor a placement belongs to: the one the settings file names, or the
/// one the card's surface is actually on.  `None` leaves the placement in the
/// monitor-less key.
pub fn monitor_key<'a>(configured: &'a str, connector: Option<&'a str>) -> Option<&'a str> {
    if !configured.is_empty() && configured != "default" {
        return Some(configured);
    }

    connector.filter(|name| !name.is_empty())
}

// ---------------------------------------------------------------------------
// Reminders
// ---------------------------------------------------------------------------

/// One reminder row, as the card draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderRow {
    pub text: String,
    pub done: bool,
}

/// A card's reminder list.  The done flags are runtime state: a click toggles
/// a row and the new list is written to the widget state file, never to the
/// settings file the user versions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reminders {
    items: Vec<String>,
}

impl Reminders {
    /// Split a settings or state value into items.  Comma and newline both
    /// separate, surrounding whitespace is dropped and empty items are not
    /// rows.
    pub fn parse(items: &str) -> Self {
        Self {
            items: items
                .split([',', '\n'])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// The list a card opens with: the state file's copy when it has one, then
    /// the configured `#:items`, then the built-in list.
    pub fn restore(saved: Option<&str>, configured: Option<&str>) -> Self {
        if let Some(saved) = saved.filter(|value| !value.is_empty()) {
            return Self::parse(saved);
        }

        if let Some(configured) = configured.filter(|value| !value.is_empty()) {
            return Self::parse(configured);
        }

        Self::parse(DEFAULT_REMINDERS)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The rows the card draws, at most [`MAX_REMINDERS`] of them.
    pub fn rows(&self) -> Vec<ReminderRow> {
        self.items
            .iter()
            .take(MAX_REMINDERS)
            .map(|item| ReminderRow {
                text: reminder_text(item).to_owned(),
                done: reminder_done(item),
            })
            .collect()
    }

    /// Toggle one row's done flag.  `false` when the index is not a row.
    pub fn toggle(&mut self, index: usize) -> bool {
        let Some(item) = self.items.get(index) else {
            return false;
        };
        let text = reminder_text(item).to_owned();

        self.items[index] = if reminder_done(item) {
            text
        } else {
            format!("[x] {text}")
        };

        true
    }

    /// The value the state file stores: one item per line.
    pub fn serialised(&self) -> String {
        self.items.join("\n")
    }
}

/// The spellings a done reminder accepts.
const DONE_MARKERS: [&str; 4] = ["[x] ", "[X] ", "x ", "X "];

/// Whether a reminder item is marked done.  The card accepts the Markdown and
/// the plain spellings.
pub fn reminder_done(text: &str) -> bool {
    done_marker(text).is_some()
}

/// The item without its done marker.  The C module strips four characters for
/// every spelling, so `x water the plants` loses its first two letters; this
/// one strips the marker it matched.
pub fn reminder_text(text: &str) -> &str {
    done_marker(text).map_or(text, |rest| rest)
}

/// The item after its marker, when it has one.
fn done_marker(text: &str) -> Option<&str> {
    DONE_MARKERS
        .into_iter()
        .find_map(|marker| text.strip_prefix(marker))
}

// ---------------------------------------------------------------------------
// World clock
// ---------------------------------------------------------------------------

/// One city row in a world clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct City {
    pub label: String,
    pub zone: String,
}

/// The cities a world clock card shows.  `"Name=Zone"` entries are separated by
/// commas; an entry without a name is named after the last component of its
/// zone (`Europe/London` becomes `London`).
pub fn cities(input: &str) -> Vec<City> {
    let input = if input.trim().is_empty() {
        DEFAULT_CITIES
    } else {
        input
    };

    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (label, zone) = match entry.split_once('=') {
                Some((label, zone)) => (label.trim(), zone.trim()),
                None => ("", entry),
            };
            let label = if label.is_empty() {
                zone.rsplit('/').next().unwrap_or(zone).to_owned()
            } else {
                label.to_owned()
            };

            City {
                label,
                zone: zone.to_owned(),
            }
        })
        .collect()
}

/// One city's clock, as the world-clock card draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CityReading {
    pub time: String,
    pub weekday: String,
    /// The hour in that city, which is what a day/night mark is chosen by.
    pub hour: u32,
}

impl CityReading {
    /// Whether the city is in daylight, for a card that draws a mark beside
    /// the time.  Seven to nineteen is a deliberately generous day: the card
    /// is a picture of the world, not an almanac.
    pub fn is_day(&self) -> bool {
        (7..19).contains(&self.hour)
    }
}

/// A city's clock at `now`, or `None` when this machine does not know the
/// zone.  The caller falls back to UTC, as the C card does.
pub fn city_reading(zone: &str) -> Option<CityReading> {
    // `from_identifier` is `g_time_zone_new_identifier`.  The C card tries
    // the configured zone and falls back to UTC when this machine does not
    // know it.
    let timezone = gtk::glib::TimeZone::from_identifier(Some(zone))
        .or_else(|| gtk::glib::TimeZone::from_identifier(Some("UTC")))?;
    let now = gtk::glib::DateTime::now(&timezone).ok()?;

    Some(CityReading {
        time: now.format("%H:%M").ok()?.to_string(),
        weekday: now.format("%a").ok()?.to_string(),
        hour: now.hour() as u32,
    })
}

/// A city's time and weekday, the two strings the first card drew.
pub fn city_time(zone: &str) -> Option<(String, String)> {
    city_reading(zone).map(|reading| (reading.time, reading.weekday))
}

// ---------------------------------------------------------------------------
// Weather
// ---------------------------------------------------------------------------

/// One day of the forecast under the current conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForecastDay {
    pub label: String,
    pub glyph: &'static str,
    pub range: String,
}

/// One Open-Meteo answer, formatted the way the card reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherReading {
    pub glyph: &'static str,
    pub temperature: String,
    pub condition: &'static str,
    pub range: String,
    pub details: String,
    /// The next few days, when the request asked for them.
    pub forecast: Vec<ForecastDay>,
}

/// The WMO code's words.
pub fn weather_condition(code: i64) -> &'static str {
    match code {
        0 => "Clear",
        1..=3 => "Partly cloudy",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 => "Rain",
        71..=77 => "Snow",
        80..=82 => "Showers",
        85 | 86 => "Snow showers",
        95.. => "Thunderstorm",
        _ => "Weather",
    }
}

/// The WMO code's glyph.
pub fn weather_glyph(code: i64) -> &'static str {
    match code {
        0 => "☀",
        1..=3 => "⛅",
        45 | 48 => "🌫",
        51..=57 => "🌦",
        61..=67 => "☔",
        71..=86 => "❄",
        95.. => "⛈",
        _ => "☁",
    }
}

/// Read Open-Meteo's current conditions and today's range.  An answer that is
/// missing a piece the card shows is not a reading.
pub fn parse_weather(text: &str, metric: bool) -> Option<WeatherReading> {
    let root = Json::parse(text).ok()?;
    let current = root.get("current")?;
    let daily = root.get("daily")?;
    let high = daily.get("temperature_2m_max")?.at(0)?.as_f64()?;
    let low = daily.get("temperature_2m_min")?.at(0)?.as_f64()?;
    let temperature = current.get_number("temperature_2m", 0.0);
    let feels = current.get_number("apparent_temperature", temperature);
    let humidity = current.get_number("relative_humidity_2m", -1.0);
    let code = current.get_number("weather_code", 0.0) as i64;
    let unit = if metric { 'C' } else { 'F' };

    Some(WeatherReading {
        glyph: weather_glyph(code),
        temperature: format!("{temperature:.0}°{unit}"),
        condition: weather_condition(code),
        range: format!("H {high:.0}°  L {low:.0}°"),
        details: if humidity >= 0.0 {
            format!("Feels like {feels:.0}° · {humidity:.0}% humidity")
        } else {
            format!("Feels like {feels:.0}°")
        },
        forecast: parse_forecast(daily),
    })
}

/// The days under today's conditions, read from the same answer.  A daily
/// array without a matching date is left out: a row that cannot say which day
/// it is would be a row nobody can use.
fn parse_forecast(daily: &Json) -> Vec<ForecastDay> {
    let Some(times) = daily.get("time").and_then(Json::array) else {
        return Vec::new();
    };
    let highs = daily
        .get("temperature_2m_max")
        .and_then(Json::array)
        .unwrap_or(&[]);
    let lows = daily
        .get("temperature_2m_min")
        .and_then(Json::array)
        .unwrap_or(&[]);
    let codes = daily
        .get("weather_code")
        .and_then(Json::array)
        .unwrap_or(&[]);
    let mut forecast = Vec::new();

    for (index, date) in times.iter().enumerate() {
        let (Some(date), Some(high), Some(low)) = (
            date.as_str(),
            highs.get(index).and_then(Json::as_f64),
            lows.get(index).and_then(Json::as_f64),
        ) else {
            continue;
        };
        let code = codes.get(index).and_then(Json::as_f64).unwrap_or(0.0) as i64;

        forecast.push(ForecastDay {
            label: forecast_day_label(date).unwrap_or_else(|| date.to_owned()),
            glyph: weather_glyph(code),
            range: format!("{high:.0}°  {low:.0}°"),
        });
    }

    forecast
}

/// `2026-11-24` as `Tue`; any other shape is left to the caller.
fn forecast_day_label(date: &str) -> Option<String> {
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;

    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let index = (days_from_civil(year, month, day) + 3).rem_euclid(7) as usize;

    Some(WEEKDAYS[index].to_owned())
}

/// The forecast request for one place.
pub fn weather_url(latitude: &str, longitude: &str, imperial: bool) -> String {
    weather_url_days(latitude, longitude, imperial, 1)
}

/// The forecast request for one place and DAYS of daily rows.  Open-Meteo
/// answers at most sixteen; a card can draw far fewer.
pub fn weather_url_days(latitude: &str, longitude: &str, imperial: bool, days: u32) -> String {
    let unit = if imperial {
        "&temperature_unit=fahrenheit"
    } else {
        ""
    };
    let days = days.clamp(1, 16);

    format!(
        "{WEATHER_URL}?latitude={latitude}&longitude={longitude}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,\
         weather_code&daily=temperature_2m_max,temperature_2m_min,weather_code,time\
         &forecast_days={days}&timezone=auto{unit}"
    )
}

// ---------------------------------------------------------------------------
// Stocks
// ---------------------------------------------------------------------------

/// One delayed quote, as the card's row reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct StockQuote {
    pub symbol: String,
    pub price: f64,
    pub change_percent: f64,
    /// Today's closing prices, oldest first, when the answer carried them.
    pub spark: Vec<f64>,
}

impl StockQuote {
    pub fn price_text(&self) -> String {
        format!("{:.2}", self.price)
    }

    pub fn change_text(&self) -> String {
        format!("{:+.2}%", self.change_percent)
    }

    pub fn is_up(&self) -> bool {
        self.change_percent >= 0.0
    }
}

/// The symbols a card asks about: trimmed, upper-cased, empty entries dropped
/// and at most [`MAX_STOCKS`] of them.
pub fn stock_symbols(input: &str) -> Vec<String> {
    let input = if input.trim().is_empty() {
        DEFAULT_SYMBOLS
    } else {
        input
    };

    input
        .split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .take(MAX_STOCKS)
        .map(str::to_uppercase)
        .collect()
}

/// Read the spark answer for `symbols`.  A symbol without a price is left out
/// rather than drawn as zero, and a quote without a change percent falls back
/// to the previous close.
pub fn parse_stocks(text: &str, symbols: &[String]) -> Vec<StockQuote> {
    let Ok(root) = Json::parse(text) else {
        return Vec::new();
    };

    symbols
        .iter()
        .filter_map(|symbol| {
            let quote = root.get(symbol)?;
            let price = quote.get_number("fulldayPrice", 0.0);

            if price <= 0.0 {
                return None;
            }

            let change = match quote.get("fulldayChangePercent").and_then(Json::as_f64) {
                Some(change) => change,
                None => {
                    let previous = quote.get_number("chartPreviousClose", 0.0);

                    if previous > 0.0 {
                        (price - previous) / previous * 100.0
                    } else {
                        0.0
                    }
                }
            };

            let spark = quote
                .get("close")
                .and_then(Json::array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Json::as_f64)
                        .filter(|value| value.is_finite())
                        .collect()
                })
                .unwrap_or_default();

            Some(StockQuote {
                symbol: symbol.clone(),
                price,
                change_percent: change,
                spark,
            })
        })
        .collect()
}

/// The points of a sparkline for `values`, in a `width` by `height` box.
///
/// A flat line is drawn down the middle rather than as a division by zero, and
/// a single value is a point at the centre.  The caller draws the polyline; the
/// arithmetic is here because it is what can be wrong.
pub fn sparkline_points(values: &[f64], width: f64, height: f64) -> Vec<(f64, f64)> {
    if values.is_empty()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return Vec::new();
    }

    let finite: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();

    if finite.is_empty() {
        return Vec::new();
    }

    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;

    if finite.len() == 1 {
        return vec![(width / 2.0, height / 2.0)];
    }

    finite
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 * width / (finite.len() - 1) as f64;
            let y = if span <= 0.0 {
                height / 2.0
            } else {
                height - (value - min) / span * height
            };

            (x, y)
        })
        .collect()
}

/// The quote request for a set of symbols.
pub fn stocks_url(symbols: &[String]) -> String {
    let encoded = symbols
        .iter()
        .map(|symbol| percent_encode(symbol))
        .collect::<Vec<_>>()
        .join(",");

    format!("{STOCKS_URL}?symbols={encoded}&range=1d&interval=5m")
}

/// `g_uri_escape_string(..., allow_utf8 = TRUE)` for the handful of characters
/// a ticker can contain.
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Notes and custom readings
// ---------------------------------------------------------------------------

/// A Notes card's body: the file it names when that file can be read, else the
/// `#:text` it carries, else what to write to make it show something.
pub fn note_text(file: Option<&str>, text: Option<&str>) -> String {
    if let Some(file) = file.filter(|file| !file.is_empty())
        && let Ok(contents) = std::fs::read_to_string(file)
    {
        return contents.trim().to_owned();
    }

    text.map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_NOTE.to_owned())
}

/// A note as a card draws it.
///
/// A note is usually a file someone typed in an editor: trailing spaces,
/// tabs, and the two or three blank lines a person leaves between thoughts.
/// Drawn as they are, those read as ragged text in a narrow card.  This keeps
/// the paragraph breaks that mean something, expands a tab to four spaces,
/// drops trailing whitespace from every line and collapses every run of blank
/// lines to one -- so a paragraph gap is the same height wherever it happens.
pub fn note_display(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut pending_blank = false;

    for line in text.lines() {
        let expanded = line.replace('\t', "    ");
        let trimmed = expanded.trim_end();

        if trimmed.trim().is_empty() {
            // A blank line only separates anything once something precedes it.
            pending_blank = !lines.is_empty();
            continue;
        }

        if pending_blank {
            lines.push(String::new());
            pending_blank = false;
        }

        lines.push(trimmed.to_owned());
    }

    lines.join("\n")
}

/// A custom card's reading: the command's output, the configured empty answer,
/// or the one that says how to configure it.
pub fn custom_text(output: Option<&str>, empty: Option<&str>) -> String {
    match output.filter(|output| !output.is_empty()) {
        Some(output) => output.to_owned(),
        None => empty
            .filter(|empty| !empty.is_empty())
            .unwrap_or("No output")
            .to_owned(),
    }
}

/// A custom card's cadence: never faster than five seconds.
pub fn custom_interval(configured: Option<i64>) -> u32 {
    configured.unwrap_or(300).clamp(5, i64::from(u32::MAX)) as u32
}

// ---------------------------------------------------------------------------
// Calendar
// ---------------------------------------------------------------------------

/// The weekday headings, Monday first, as the card draws them.
pub const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// One month laid out the way the card draws it: the heading, the empty cells
/// before the first, the day count and today when the card is showing today's
/// month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthGrid {
    pub heading: String,
    /// Empty cells before the first day, Monday-first.
    pub leading: usize,
    pub days: u32,
    /// The day number that is today, when this grid is the current month.
    pub today: Option<u32>,
    /// The ISO week number of each row, top to bottom.
    pub weeks: Vec<u32>,
}

/// Lay out `month` of `year`, marking `today`'s day only when the grid is
/// today's month.  Dates are the proleptic Gregorian calendar, which is what
/// `GDateTime` uses.
pub fn month_grid(year: i32, month: u32, today: (i32, u32, u32)) -> MonthGrid {
    let month = month.clamp(1, 12) as usize;
    let days = days_in_month(year, month as u32);
    let leading = weekday_of_first(year, month as u32);
    let today = (today.0 == year && today.1 == month as u32 && today.2 >= 1 && today.2 <= days)
        .then_some(today.2);

    let weeks = (0..=(leading + days as usize - 1) / 7)
        .map(|row| {
            let day = (row * 7).saturating_sub(leading) + 1;

            if day > days as usize {
                iso_week(year, month as u32, days)
            } else {
                iso_week(year, month as u32, day as u32)
            }
        })
        .collect();

    MonthGrid {
        heading: format!("{} {year}", MONTHS[month - 1]),
        leading,
        days,
        today,
        weeks,
    }
}

/// The ISO-8601 week number of a date.  This is the number a week column draws,
/// and it is the calendar's own definition, not a count of seven-day runs from
/// January: a week belongs to the year that has most of it.
pub fn iso_week(year: i32, month: u32, day: u32) -> u32 {
    let ordinal = days_from_civil(year, month, day) - days_from_civil(year, 1, 1) + 1;
    let weekday = (days_from_civil(year, month, day) + 3).rem_euclid(7);
    let week = (ordinal - weekday + 10) / 7;

    if week < 1 {
        iso_weeks_in_year(year - 1)
    } else if week > i64::from(iso_weeks_in_year(year)) {
        1
    } else {
        week as u32
    }
}

/// 53 in the years that carry a week 53, 52 otherwise.
fn iso_weeks_in_year(year: i32) -> u32 {
    let p = |year: i32| (year + year / 4 - year / 100 + year / 400).rem_euclid(7);

    if p(year) == 4 || p(year - 1) == 3 {
        53
    } else {
        52
    }
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The Monday-first weekday index of the first of `month`.
fn weekday_of_first(year: i32, month: u32) -> usize {
    // 1970-01-01 was a Thursday, which is index 3 when Monday is 0.
    let days = days_from_civil(year, month, 1);

    (days + 3).rem_euclid(7) as usize
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(if month <= 2 { year - 1 } else { year });
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------
// Now playing
// ---------------------------------------------------------------------------

/// One player's reading, laid out the way a card draws it.
///
/// The card is a view of [`crate::mpris`]'s reading, not a second reader: the
/// same MPRIS proxies that move the bar move the card, and the controls are the
/// player's own methods rather than a command that has to find the player
/// again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlayingText {
    pub available: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub state: String,
    pub glyph: &'static str,
    pub position: String,
    pub length: String,
    pub percent: u32,
    pub player: String,
    pub playing: bool,
}

impl NowPlayingText {
    /// The card with nothing on the bus.
    pub fn idle() -> Self {
        Self {
            available: false,
            title: "Nothing playing".to_owned(),
            artist: String::new(),
            album: String::new(),
            state: "stopped".to_owned(),
            glyph: "⏹",
            position: "0:00".to_owned(),
            length: "0:00".to_owned(),
            percent: 0,
            player: String::new(),
            playing: false,
        }
    }

    /// The line under the title: the artist, the album, or both.
    pub fn detail(&self) -> String {
        match (self.artist.trim(), self.album.trim()) {
            ("", "") => String::new(),
            (artist, "") => artist.to_owned(),
            ("", album) => album.to_owned(),
            (artist, album) => format!("{artist} — {album}"),
        }
    }

    /// `position / length`, or just the position when the player named no end.
    pub fn timeline(&self) -> String {
        if self.length == "0:00" {
            self.position.clone()
        } else {
            format!("{} / {}", self.position, self.length)
        }
    }
}

/// The transport mark a card draws.  The bar's own icon is a private-use
/// glyph from the configured icon font; a card has no such font, so it uses a
/// mark every text face carries.
fn playback_glyph(playback: mpris::Playback) -> &'static str {
    match playback {
        mpris::Playback::Playing => "▶",
        mpris::Playback::Paused => "⏸",
        mpris::Playback::Stopped => "⏹",
    }
}

/// Lay out one MPRIS reading for the card.
pub fn now_playing_text(reading: &Reading) -> NowPlayingText {
    if !reading.available() {
        return NowPlayingText::idle();
    }

    let title = if reading.track.title.trim().is_empty() {
        if reading.track.album.trim().is_empty() {
            "Unknown track"
        } else {
            reading.track.album.as_str()
        }
    } else {
        reading.track.title.as_str()
    };

    NowPlayingText {
        available: true,
        title: title.to_owned(),
        artist: reading.track.artist.clone(),
        album: reading.track.album.clone(),
        state: reading.playback.name().to_owned(),
        glyph: playback_glyph(reading.playback),
        position: mpris::format_time(reading.position_us),
        length: mpris::format_time(reading.track.length_us),
        percent: mpris::progress_percent(reading.position_us, reading.track.length_us),
        player: reading.player.clone().unwrap_or_default(),
        playing: reading.playback.is_playing(),
    }
}

/// One live visualizer frame: the bars cava last reported and the peak each bar
/// has reached since it started falling.
///
/// The peaks are the visualizer's memory.  A bar's peak jumps up with the bar,
/// then falls at [`Self::decay`] a frame; keeping that here rather than in the
/// painter means a card can repaint at any moment and still draw the same
/// falling caps.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualizerFrame {
    /// One level per bar, 0.0..=1.0.
    pub levels: Vec<f64>,
    /// The held peak of each bar, 0.0..=1.0.
    pub peaks: Vec<f64>,
    /// How much a peak falls per frame once the bar has dropped below it.
    pub decay: f64,
}

impl VisualizerFrame {
    pub fn new(bars: usize) -> Self {
        let bars = bars.clamp(1, VISUALIZER_MAX_BARS);

        Self {
            levels: vec![0.0; bars],
            peaks: vec![0.0; bars],
            decay: 0.018,
        }
    }

    pub fn bars(&self) -> usize {
        self.levels.len()
    }

    /// Take one frame from cava and move the peaks toward it.  A short frame is
    /// padded with silence and a long one is cut, so a cava configured with a
    /// different bar count still paints.
    pub fn push(&mut self, frame: &[f64]) {
        for (index, level) in self.levels.iter_mut().enumerate() {
            *level = frame.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        }

        for (peak, level) in self.peaks.iter_mut().zip(&self.levels) {
            *peak = (*peak - self.decay).max(*level).clamp(0.0, 1.0);
        }
    }

    /// Forget every level and peak, for a player that went quiet.
    pub fn clear(&mut self) {
        self.levels.fill(0.0);
        self.peaks.fill(0.0);
    }
}

/// Read one `ascii` cava frame into levels from 0.0 to 1.0.
///
/// cava writes `0;12;100;` followed by a newline in this mode.  Semicolons are
/// the documented separator; commas and whitespace are accepted too, because a
/// hand-written or older config may use them.  Anything that is not a number is
/// ignored, and at most `bars` values are returned.
pub fn parse_visualizer_frame(line: &str, bars: usize) -> Vec<f64> {
    line.split([';', ',', ' ', '\t'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .map(|value| (value / 100.0).clamp(0.0, 1.0))
        .take(bars.min(VISUALIZER_MAX_BARS))
        .collect()
}

/// The cava configuration the visualizer owns while its card is alive.
///
/// It writes raw ASCII to stdout, which is the one mode a subprocess can read
/// without an audio server on whelk's side or a second decoder.  The card never
/// lets cava open its own terminal, and the `[input]` section is cava's own
/// auto-detection, so PulseAudio, PipeWire and ALSA all work as they do for
/// every other cava user.
pub fn visualizer_config(bars: usize, framerate: u32) -> String {
    let bars = bars.clamp(1, VISUALIZER_MAX_BARS);
    let framerate = framerate.clamp(1, 120);

    format!(
        "[general]\nbars = {bars}\nframerate = {framerate}\n\n\
         [input]\nmethod = pulse\nsource = auto\n\n\
         [output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = ascii\n\
         ascii_max_range = 100\nbar_delimiter = 59\nframe_delimiter = 10\n"
    )
}

// ---------------------------------------------------------------------------
// Photos
// ---------------------------------------------------------------------------

/// The directories a photo card reads, one per comma-separated entry.
pub fn photo_paths(input: &str) -> Vec<PathBuf> {
    input
        .split([',', '\n'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Every still picture under `roots`, sorted by path.
///
/// The walk is bounded and skips hidden entries, exactly as the wallpaper
/// collection does: a photo frame pointed at a home directory must not spend a
/// timer walking a cache.  A root that names a file is taken as it is.
pub fn photo_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 4;

    fn is_photo(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                PHOTO_EXTENSIONS
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
            })
    }

    fn walk(path: &Path, depth: usize, into: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH {
            return;
        }

        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return;
        };

        if metadata.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                return;
            };

            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }

                walk(&entry.path(), depth + 1, into);
            }

            return;
        }

        if metadata.is_file() && is_photo(path) {
            into.push(path.to_owned());
        }
    }

    let mut files = Vec::new();

    for root in roots {
        walk(root, 0, &mut files);
    }

    files.sort_by(|left, right| left.as_os_str().cmp(right.as_os_str()));
    files.dedup();

    files
}

/// The index a photo card shows after moving by `by`, wrapping at both ends.
pub fn photo_step(index: usize, len: usize, by: i32) -> usize {
    if len == 0 {
        return 0;
    }

    let len = len as i32;
    let next = (index as i32 + by).rem_euclid(len);

    next as usize
}

/// A photo's caption: its file stem with underscores and hyphens as spaces.
pub fn photo_caption(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let caption = stem.replace(['_', '-'], " ");
    let caption = caption.split_whitespace().collect::<Vec<_>>().join(" ");

    if caption.is_empty() {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        caption
    }
}

// ---------------------------------------------------------------------------
// Recent files
// ---------------------------------------------------------------------------

/// One row of the Files card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentFile {
    pub title: String,
    pub uri: String,
    /// `Local`, `Web` or `Recent`: what the row is, not where it lives.
    pub kind: &'static str,
}

/// `$XDG_DATA_HOME/recently-used.xbel`, or the home-relative default.
pub fn recent_xbel_path(data_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    match data_home.filter(|path| !path.as_os_str().is_empty()) {
        Some(path) => Some(path.join("recently-used.xbel")),
        None => home
            .filter(|path| !path.as_os_str().is_empty())
            .map(|home| home.join(".local/share/recently-used.xbel")),
    }
}

/// Read the recent-documents list the desktop already keeps.
///
/// The file is XBEL, a small XML dialect.  This reader takes the one shape the
/// desktops write -- a `<bookmark href=...>` with an optional `<title>` -- and
/// ignores every element it does not know, so a record it cannot read costs one
/// row rather than the whole list.
pub fn parse_recent(xbel: &str, limit: usize) -> Vec<RecentFile> {
    let mut files = Vec::new();
    let mut rest = xbel;

    while let Some(start) = rest.find("<bookmark") {
        rest = &rest[start..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..tag_end];
        let href = attribute(tag, "href").filter(|href| !href.is_empty());
        let body_start = tag_end + 1;
        let body_end = rest[body_start..]
            .find("</bookmark>")
            .map(|offset| body_start + offset)
            .unwrap_or(rest.len());
        let body = &rest[body_start..body_end];
        let title = element_text(body, "title").filter(|title| !title.is_empty());

        if let Some(href) = href {
            let title = title.unwrap_or_else(|| uri_display_name(&href));
            let kind = if href.starts_with("file://") {
                "Local"
            } else if href.starts_with("http://") || href.starts_with("https://") {
                "Web"
            } else {
                "Recent"
            };

            if !files
                .iter()
                .any(|file: &RecentFile| file.uri == href && file.title == title)
            {
                files.push(RecentFile {
                    title,
                    uri: href,
                    kind,
                });
            }
        }

        rest = &rest[body_end..];

        if rest.starts_with("</bookmark>") {
            rest = &rest["</bookmark>".len()..];
        }
    }

    files.truncate(limit);
    files
}

/// The name a URI is shown under when its record has no title.
pub fn uri_display_name(uri: &str) -> String {
    let path = uri
        .strip_prefix("file://")
        .or_else(|| uri.split_once("://").map(|(_, rest)| rest))
        .unwrap_or(uri);
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let name = path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path);
    let decoded = percent_decode(name);

    if decoded.is_empty() {
        uri.to_owned()
    } else {
        decoded
    }
}

/// `%20` and friends, as the file name they spell.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        // Only three ASCII bytes can spell `%XX`; checking them before slicing
        // keeps a `%` followed by a multi-byte character from splitting it.
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
            && let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            out.push(byte);
            index += 3;
            continue;
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// One attribute out of an XML start tag; the value may use double or single
/// quotes.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag.find(&needle)? + needle.len();
    let quote = tag[start..].chars().next()?;

    if quote != '"' && quote != '\'' {
        return None;
    }

    let value = &tag[start + 1..];
    let end = value.find(quote)?;

    Some(xml_decode(&value[..end]))
}

/// The text of the first `name` element in `body`, with CDATA unwrapped and
/// entities decoded.
fn element_text(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    let raw = &body[start..end];
    let raw = raw.strip_prefix("<![CDATA[").unwrap_or(raw);
    let raw = raw.strip_suffix("]]>").unwrap_or(raw);

    Some(xml_decode(&strip_tags(raw)))
}

/// The text of an element and its attributes, for an Atom `link`.
fn link_text(block: &str) -> Option<String> {
    if let Some(text) = element_text(block, "link").filter(|text| !text.trim().is_empty()) {
        return Some(text.trim().to_owned());
    }

    let tag = block.find("<link")?;
    let end = block[tag..].find('>')? + tag;

    attribute(&block[tag..end], "href")
}

/// Remove the tags an RSS title is allowed to carry.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;

    for character in text.chars() {
        match character {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(character),
            _ => {}
        }
    }

    out
}

/// The five named entities XML uses, plus a numeric form.
fn xml_decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };

        match decoded {
            Some(character) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }

    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// News
// ---------------------------------------------------------------------------

/// One headline from a feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub source: String,
    pub published: String,
}

/// The feeds a card reads: trimmed, deduplicated and bounded.
pub fn feed_urls(input: &str) -> Vec<String> {
    let mut urls = Vec::new();

    for entry in input.split([',', '\n']).map(str::trim) {
        if entry.is_empty() || !(entry.starts_with("http://") || entry.starts_with("https://")) {
            continue;
        }

        if !urls.iter().any(|url| url == entry) {
            urls.push(entry.to_owned());
        }

        if urls.len() == MAX_FEEDS {
            break;
        }
    }

    urls
}

/// Read the headlines of an RSS 2.0 or Atom feed.
///
/// This is a deliberately small reader: a feed is XML with a great deal of it
/// optional, and a card needs a title, a link and a date.  A feed that answers
/// in a shape this reader does not know contributes no headlines rather than
/// invented ones.
pub fn parse_feed(text: &str, source: &str, limit: usize) -> Vec<NewsItem> {
    let mut items = Vec::new();
    let mut rest = text;

    while let Some(start) = rest
        .find("<item")
        .into_iter()
        .chain(rest.find("<entry"))
        .min()
    {
        rest = &rest[start..];
        let Some(item_open) = rest.find('>') else {
            break;
        };
        let block_start = item_open + 1;
        let close = rest[block_start..]
            .find("</item>")
            .or_else(|| rest[block_start..].find("</entry>"));
        let Some(close) = close else {
            break;
        };
        let block = &rest[block_start..block_start + close];
        let title = element_text(block, "title")
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty());
        let link = link_text(block).filter(|link| !link.is_empty());
        let published = element_text(block, "pubDate")
            .or_else(|| element_text(block, "updated"))
            .or_else(|| element_text(block, "published"))
            .map(|date| date.trim().to_owned())
            .unwrap_or_default();

        if let Some(title) = title {
            let link = link.unwrap_or_default();

            if !items
                .iter()
                .any(|item: &NewsItem| item.title == title && item.link == link)
            {
                items.push(NewsItem {
                    title,
                    link,
                    source: source.to_owned(),
                    published,
                });
            }
        }

        rest = &rest[block_start + close..];

        if items.len() >= limit {
            break;
        }
    }

    items
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// The three load averages a System card draws.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// `/proc/loadavg`'s first three fields.
pub fn parse_loadavg(text: &str) -> Option<LoadAverage> {
    let mut fields = text.split_whitespace();

    Some(LoadAverage {
        one: fields.next()?.parse().ok()?,
        five: fields.next()?.parse().ok()?,
        fifteen: fields.next()?.parse().ok()?,
    })
}

/// `/proc/uptime`'s seconds, as a whole number.
pub fn parse_uptime(text: &str) -> Option<u64> {
    let seconds = text.split_whitespace().next()?.parse::<f64>().ok()?;

    seconds.is_finite().then(|| seconds.max(0.0).round() as u64)
}

/// The uptime in the words a card has room for: days and hours, hours and
/// minutes, or minutes.
pub fn uptime_text(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let day_word = if days == 1 { "day" } else { "days" };

    match (days, hours, minutes) {
        (0, 0, minutes) => format!("{minutes} min"),
        (0, hours, 0) => format!("{hours} h"),
        (0, hours, minutes) => format!("{hours} h {minutes} min"),
        (days, 0, _) => format!("{days} {day_word}"),
        (days, hours, _) => format!("{days} {day_word}, {hours} h"),
    }
}

/// One decimal for a load average, which is how the kernel's own tools print
/// it.
pub fn load_text(load: LoadAverage) -> String {
    format!("{:.2}  {:.2}  {:.2}", load.one, load.five, load.fifteen)
}

/// The memory line: used of total, in the same binary units the bar uses.
pub fn memory_text(used_kb: u64, total_kb: u64) -> String {
    format!(
        "{} of {}",
        format::str_gib(used_kb),
        format::str_gib(total_kb)
    )
}

// ---------------------------------------------------------------------------
// Scheme values
// ---------------------------------------------------------------------------

fn value_text(value: &SchemeValue) -> Option<String> {
    match value {
        SchemeValue::Text(text) | SchemeValue::Symbol(text) => Some(text.clone()),
        _ => None,
    }
}

fn value_int(value: &SchemeValue) -> Option<i32> {
    match value {
        SchemeValue::Integer(number) => i32::try_from(*number).ok(),
        SchemeValue::Number(number) if number.fract() == 0.0 => Some(*number as i32),
        _ => None,
    }
}

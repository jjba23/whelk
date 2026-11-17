// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The desktop widgets' data, ported from the display-free half of
//! `src/widgets.c` and the `(whelk state)` records behind it.
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

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::json::Json;
use crate::settings::SchemeValue;

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

/// The note a Notes card shows when it was given nothing else.
pub const DEFAULT_NOTE: &str = "No note yet.\nSet #:text or #:file on this widget.";

/// The forecast request's fixed half.
pub const WEATHER_URL: &str = "https://api.open-meteo.com/v1/forecast";

/// The quote request's fixed half.
pub const STOCKS_URL: &str = "https://query1.finance.yahoo.com/v8/finance/spark";

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
            Self::Custom => "custom",
        }
    }

    /// The size Apple's widget gallery gives this card, from
    /// `%widget-sizes` in `(whelk state)`.  A kind of the configuration's own
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
            Self::Stocks => (300, 260),
            Self::Custom => (320, 220),
        }
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
        }
    }

    /// Read one `shell-widgets->alists` row.
    pub fn from_alist(entries: &[(String, SchemeValue)]) -> Option<Self> {
        let mut kind = None;
        let mut x = DEFAULT_X;
        let mut y = DEFAULT_Y;
        let mut width = None;
        let mut height = None;
        let mut monitor = "default".to_owned();
        let mut options = std::collections::BTreeMap::new();

        for (key, value) in entries {
            match key.as_str() {
                "kind" => kind = value_text(value),
                "x" => x = value_int(value).unwrap_or(x),
                "y" => y = value_int(value).unwrap_or(y),
                "width" => width = value_int(value),
                "height" => height = value_int(value),
                "monitor" => monitor = value_text(value).unwrap_or(monitor),
                "options" => {
                    if let SchemeValue::Alist(values) = value {
                        options = values.iter().cloned().collect();
                    }
                }
                _ => {}
            }
        }

        Some(Self::new(
            ModuleSpec {
                kind: kind?,
                options,
            },
            x,
            y,
            width,
            height,
            &monitor,
        ))
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
        match saved {
            Some((x, y)) => clamp_into(x, y, self.width, self.height, area),
            None => (self.x, self.y),
        }
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

/// A city's clock at `now`, or `None` when this machine does not know the
/// zone.  The caller falls back to UTC, as the C card does.
pub fn city_time(zone: &str) -> Option<(String, String)> {
    // `glib::TimeZone::new` is `g_time_zone_new`, which falls back to UTC for
    // a zone this machine does not know.  The C card calls
    // `g_time_zone_new_identifier` first so it can say so on stderr; that
    // function is not bound here, so an unknown zone silently reads as UTC.
    let timezone = gtk::glib::TimeZone::new(Some(zone));
    let now = gtk::glib::DateTime::now(&timezone).ok()?;

    Some((
        now.format("%H:%M").ok()?.to_string(),
        now.format("%a").ok()?.to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Weather
// ---------------------------------------------------------------------------

/// One Open-Meteo answer, formatted the way the card reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherReading {
    pub glyph: &'static str,
    pub temperature: String,
    pub condition: &'static str,
    pub range: String,
    pub details: String,
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
    })
}

/// The forecast request for one place.
pub fn weather_url(latitude: &str, longitude: &str, imperial: bool) -> String {
    let unit = if imperial {
        "&temperature_unit=fahrenheit"
    } else {
        ""
    };

    format!(
        "{WEATHER_URL}?latitude={latitude}&longitude={longitude}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,\
         weather_code&daily=temperature_2m_max,temperature_2m_min\
         &forecast_days=1&timezone=auto{unit}"
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

            Some(StockQuote {
                symbol: symbol.clone(),
                price,
                change_percent: change,
            })
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

    format!("{STOCKS_URL}?symbols={encoded}&range=1d&interval=1d")
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

    MonthGrid {
        heading: format!("{} {year}", MONTHS[month - 1]),
        leading,
        days,
        today,
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

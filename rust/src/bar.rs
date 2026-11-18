// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The plain bar configuration, ported from `bar_from_alist` in the C bridge.
//!
//! The Scheme side hands the shell an alist of plain values; this is
//! the same shape as Rust types.  A missing field takes the C `WhelkBarConfig`
//! default, and every conversion stays testable without Guile or a display.

use std::collections::BTreeMap;

use crate::settings::SchemeValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    fn parse(text: &str) -> Self {
        match text {
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            "right" => Self::Right,
            _ => Self::Top,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl Layer {
    fn parse(text: &str) -> Self {
        match text {
            "background" => Self::Background,
            "bottom" => Self::Bottom,
            "overlay" => Self::Overlay,
            _ => Self::Top,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Always,
    Single,
    Multi,
}

/// The half of a bar a module was configured in.  The C factory walks left,
/// centre, then right; naming the half lets the Rust shell put the few
/// implemented modules where the configuration asked for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Left,
    Center,
    Right,
}

impl When {
    fn parse(text: &str) -> Self {
        match text {
            "single" => Self::Single,
            "multi" => Self::Multi,
            _ => Self::Always,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Single => "single",
            Self::Multi => "multi",
        }
    }
}

/// One configured module.  The options keep their Scheme types so a later
/// module factory can read booleans, symbols and integers without re-parsing
/// the rendered text.
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct BarSettings {
    pub name: String,
    pub theme: Option<String>,
    pub edge: Edge,
    pub layer: Layer,
    pub monitor: String,
    pub monitors: Vec<String>,
    pub when: When,
    /// Whether a visible `wifi` module already speaks for the network.  The
    /// tray reads this after the bar is parsed so an `nm-applet` icon is not a
    /// second control for the same radio.
    pub native_network: bool,
    /// The same rule for a visible `bluetooth` module.
    pub native_bluetooth: bool,
    pub height: i32,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub exclusive: bool,
    /// Hide the bar and give its space back, revealing it on a hot edge or a
    /// keybinding.  A hidden layer surface keeps no exclusive zone, so a
    /// maximised window really does use the pixels.
    pub auto_hide: bool,
    /// Ask the compositor for on-demand keyboard focus and make the modules
    /// traversable: Tab and the arrows between readings, Return to run one.
    pub keyboard: bool,
    /// How long the bar waits after the pointer leaves before it hides again.
    pub auto_hide_delay_ms: u32,
    pub spacing: i32,
    pub padding: i32,
    pub font: Option<String>,
    pub font_size: Option<String>,
    pub icon_font: Option<String>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub accent: Option<String>,
    pub border_color: Option<String>,
    pub radius: i32,
    pub opacity: f64,
    pub blur: i32,
    pub shine: f64,
    pub reflection: f64,
    pub flare: f64,
    pub css: Option<String>,
    pub launcher_width: i32,
    pub launcher_height: i32,
    pub launcher_icon_size: i32,
    pub launcher_font_scale: i32,
    pub left: Vec<ModuleSpec>,
    pub center: Vec<ModuleSpec>,
    pub right: Vec<ModuleSpec>,
    pub overflow: Vec<ModuleSpec>,
    pub overflow_expanded: bool,
}

/// How long an auto-hiding bar waits after the pointer leaves it, in
/// milliseconds.  Long enough to cross a gap to the next surface, short enough
/// that the pixels are worth reclaiming.
pub const DEFAULT_AUTO_HIDE_DELAY_MS: u32 = 900;
/// A configured delay is clamped to this: a bar that never comes back is worse
/// than one that never left.
pub const MAX_AUTO_HIDE_DELAY_MS: u32 = 60_000;

/// The room kept for the fold's `+N` label; the label can reach two digits, so
/// it is a little wider than the chevron it replaced.
pub const FOLD_BUTTON_WIDTH: i32 = 34;

/// How much a reading wants to stay on the row.
///
/// A configuration says this per module: `#:keep? #t` is the loudest claim
/// there is, `#:priority` is a number to tune (a clock at 100 beats a load
/// average at 10), and a reading that says nothing keeps the positional default
/// every bar had before there was a priority at all.
pub const DEFAULT_PRIORITY: i32 = 0;
pub const KEEP_PRIORITY: i32 = 1_000;

/// The priority a module declares, clamped to the range the fold understands.
pub fn module_priority(spec: &ModuleSpec) -> i32 {
    if spec.boolean("keep?").unwrap_or(false) {
        return KEEP_PRIORITY;
    }

    spec.integer("priority")
        .map(|priority| priority.clamp(-i64::from(KEEP_PRIORITY), i64::from(KEEP_PRIORITY)) as i32)
        .unwrap_or(DEFAULT_PRIORITY)
}

/// One reading on the row: how wide it is and how much it wants to stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub width: i32,
    pub priority: i32,
}

/// The readings that stay on the row, as indexes into `readings` in row order.
///
/// The C bar folded positionally -- everything past the first that did not fit
/// -- which is how a clock can disappear while a load average stays.  Here the
/// claim decides: readings are taken in priority order (position breaks a tie,
/// so a row of equals folds from the right exactly as it always did) while the
/// row's width allows, and the answer is returned in row order so the row keeps
/// the order the configuration wrote.
///
/// A row too narrow for even its best reading still keeps one: an empty right
/// row is worse than a clipped clock.
pub fn fold_visible(readings: &[Reading], available: i32, spacing: i32) -> Vec<usize> {
    if readings.is_empty() {
        return Vec::new();
    }

    let available = available.max(0);
    let spacing = spacing.max(0);
    let mut order: Vec<usize> = (0..readings.len()).collect();

    order.sort_by(|left, right| {
        readings[*right]
            .priority
            .cmp(&readings[*left].priority)
            .then(left.cmp(right))
    });

    // Everything fits: no fold, and no room has to be kept for its button.
    let total: i32 = readings
        .iter()
        .map(|reading| reading.width.max(0))
        .sum::<i32>()
        + spacing * (readings.len() as i32 - 1);

    if total <= available {
        return (0..readings.len()).collect();
    }

    let mut kept = Vec::new();
    let mut used = 0;

    for index in order {
        let width = readings[index].width.max(0);
        let gap = if kept.is_empty() { 0 } else { spacing };
        // Anything left out of the row has to be reachable: the `+N` button is
        // charged to every reading that is not the last one taken.
        let reserve = FOLD_BUTTON_WIDTH + spacing;

        if used + gap + width + reserve > available && !kept.is_empty() {
            continue;
        }

        used += gap + width;
        kept.push(index);
    }

    kept.sort_unstable();
    kept
}

impl Default for BarSettings {
    fn default() -> Self {
        Self {
            name: "main".to_owned(),
            theme: None,
            edge: Edge::Top,
            layer: Layer::Top,
            monitor: "all".to_owned(),
            monitors: Vec::new(),
            when: When::Always,
            native_network: false,
            native_bluetooth: false,
            height: 32,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            exclusive: true,
            auto_hide: false,
            keyboard: false,
            auto_hide_delay_ms: DEFAULT_AUTO_HIDE_DELAY_MS,
            spacing: 6,
            padding: 12,
            font: None,
            font_size: None,
            icon_font: None,
            foreground: None,
            background: None,
            accent: None,
            border_color: None,
            radius: 0,
            opacity: 1.0,
            blur: 0,
            shine: 0.0,
            reflection: 0.0,
            flare: 0.0,
            css: None,
            launcher_width: 660,
            launcher_height: 640,
            launcher_icon_size: 24,
            launcher_font_scale: 100,
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            overflow: Vec::new(),
            overflow_expanded: false,
        }
    }
}

impl BarSettings {
    pub fn from_alist(alist: &[(String, SchemeValue)]) -> Self {
        let mut bar = Self::default();

        if alist.is_empty() {
            return bar;
        }

        bar.name = non_empty_text(alist, "name").unwrap_or_else(|| "main".to_owned());
        bar.theme = optional_text(alist, "theme");
        bar.edge = Edge::parse(&text(alist, "edge").unwrap_or_else(|| "top".to_owned()));
        bar.layer = Layer::parse(&text(alist, "layer").unwrap_or_else(|| "top".to_owned()));
        bar.monitor = non_empty_text(alist, "monitor").unwrap_or_else(|| "all".to_owned());
        bar.when = When::parse(&text(alist, "when").unwrap_or_else(|| "always".to_owned()));
        bar.height = integer(alist, "height").unwrap_or(32) as i32;
        bar.margin_top = integer(alist, "margin-top").unwrap_or(0) as i32;
        bar.margin_right = integer(alist, "margin-right").unwrap_or(0) as i32;
        bar.margin_bottom = integer(alist, "margin-bottom").unwrap_or(0) as i32;
        bar.margin_left = integer(alist, "margin-left").unwrap_or(0) as i32;
        bar.exclusive = boolean(alist, "exclusive?").unwrap_or(true);
        bar.auto_hide = boolean(alist, "auto-hide?").unwrap_or(false);
        bar.keyboard = boolean(alist, "keyboard?").unwrap_or(false);
        bar.auto_hide_delay_ms = integer(alist, "auto-hide-delay")
            .map(|delay| delay.clamp(0, i64::from(MAX_AUTO_HIDE_DELAY_MS)) as u32)
            .unwrap_or(DEFAULT_AUTO_HIDE_DELAY_MS);
        bar.spacing = integer(alist, "spacing").unwrap_or(6) as i32;
        bar.padding = integer(alist, "padding").unwrap_or(12) as i32;
        bar.font = optional_text(alist, "font");
        bar.font_size = optional_text(alist, "font-size");
        bar.icon_font = optional_text(alist, "icon-font");
        bar.foreground = optional_text(alist, "foreground");
        bar.background = optional_text(alist, "background");
        bar.accent = optional_text(alist, "accent");
        bar.border_color = optional_text(alist, "border-color");
        bar.radius = integer(alist, "radius").unwrap_or(0) as i32;
        bar.opacity = number(alist, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
        bar.blur = integer(alist, "blur").unwrap_or(0).clamp(0, 64) as i32;
        bar.shine = number(alist, "shine").unwrap_or(0.0).clamp(0.0, 1.0);
        bar.reflection = number(alist, "reflection").unwrap_or(0.0).clamp(0.0, 1.0);
        bar.flare = number(alist, "flare").unwrap_or(0.0).clamp(0.0, 1.0);
        bar.css = optional_text(alist, "css");
        bar.launcher_width = integer(alist, "launcher-width").unwrap_or(660) as i32;
        bar.launcher_height = integer(alist, "launcher-height").unwrap_or(640) as i32;
        bar.launcher_icon_size = integer(alist, "launcher-icon-size").unwrap_or(24) as i32;
        bar.launcher_font_scale = integer(alist, "launcher-font-scale").unwrap_or(100) as i32;
        bar.overflow_expanded = boolean(alist, "overflow-expanded?").unwrap_or(false);

        bar.monitors = monitors(alist);
        if bar.monitors.iter().any(|monitor| monitor == "all") {
            bar.monitors.clear();
            bar.monitor = "all".to_owned();
        }

        bar.left = modules(alist, "modules-left");
        bar.center = modules(alist, "modules-center");
        bar.right = modules(alist, "modules-right");
        bar.overflow = modules(alist, "overflow");
        bar.native_network = native_module(&bar, "wifi");
        bar.native_bluetooth = native_module(&bar, "bluetooth");

        bar
    }

    pub fn summary(&self) -> String {
        format!(
            "bar: {} · {} left · {} center · {} right · {}",
            self.name,
            self.left.len(),
            self.center.len(),
            self.right.len(),
            self.edge.name()
        )
    }

    pub fn clock_format(&self) -> Option<String> {
        self.right
            .iter()
            .chain(self.center.iter())
            .chain(self.left.iter())
            .find(|module| module.kind == "clock")
            .and_then(|module| module.text("format"))
    }

    pub fn module(&self, kind: &str) -> Option<&ModuleSpec> {
        self.left
            .iter()
            .chain(self.center.iter())
            .chain(self.right.iter())
            .find(|module| module.kind == kind)
    }

    /// The first half that configures `kind`, in the C factory's order.
    pub fn section(&self, kind: &str) -> Option<Section> {
        if self.left.iter().any(|module| module.kind == kind) {
            Some(Section::Left)
        } else if self.center.iter().any(|module| module.kind == kind) {
            Some(Section::Center)
        } else if self.right.iter().any(|module| module.kind == kind) {
            Some(Section::Right)
        } else {
            None
        }
    }
}

fn native_module(bar: &BarSettings, kind: &str) -> bool {
    bar.left
        .iter()
        .chain(bar.center.iter())
        .chain(bar.right.iter())
        .any(|module| module.kind == kind && !module.boolean("hidden?").unwrap_or(false))
}

fn value_at<'a>(alist: &'a [(String, SchemeValue)], key: &str) -> Option<&'a SchemeValue> {
    alist
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

fn text(alist: &[(String, SchemeValue)], key: &str) -> Option<String> {
    match value_at(alist, key) {
        Some(SchemeValue::Text(value) | SchemeValue::Symbol(value)) => Some(value.clone()),
        _ => None,
    }
}

fn non_empty_text(alist: &[(String, SchemeValue)], key: &str) -> Option<String> {
    text(alist, key).filter(|value| !value.is_empty())
}

fn optional_text(alist: &[(String, SchemeValue)], key: &str) -> Option<String> {
    non_empty_text(alist, key)
}

fn integer(alist: &[(String, SchemeValue)], key: &str) -> Option<i64> {
    match value_at(alist, key) {
        Some(SchemeValue::Integer(value)) => Some(*value),
        Some(SchemeValue::Number(value)) if value.is_finite() && value.fract() == 0.0 => {
            Some(*value as i64)
        }
        _ => None,
    }
}

fn number(alist: &[(String, SchemeValue)], key: &str) -> Option<f64> {
    match value_at(alist, key) {
        Some(SchemeValue::Integer(value)) => Some(*value as f64),
        Some(SchemeValue::Number(value)) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn boolean(alist: &[(String, SchemeValue)], key: &str) -> Option<bool> {
    match value_at(alist, key) {
        Some(SchemeValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn monitors(alist: &[(String, SchemeValue)]) -> Vec<String> {
    match value_at(alist, "monitors") {
        Some(SchemeValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                SchemeValue::Text(value) | SchemeValue::Symbol(value) => Some(value.clone()),
                _ => None,
            })
            .filter(|value| !value.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn modules(alist: &[(String, SchemeValue)], key: &str) -> Vec<ModuleSpec> {
    let Some(SchemeValue::List(values)) = value_at(alist, key) else {
        return Vec::new();
    };

    values
        .iter()
        .filter_map(|value| match value {
            SchemeValue::Alist(entries) => module_from_alist(entries),
            _ => None,
        })
        .collect()
}

fn module_from_alist(alist: &[(String, SchemeValue)]) -> Option<ModuleSpec> {
    let kind = text(alist, "kind")?;

    if kind.is_empty() {
        return None;
    }

    let mut options = BTreeMap::new();

    if let Some(SchemeValue::Alist(entries)) = value_at(alist, "options") {
        for (key, value) in entries {
            let empty = matches!(value, SchemeValue::Text(text) if text.is_empty());

            if !key.is_empty() && !empty && !matches!(value, SchemeValue::Other) {
                options.insert(key.clone(), value.clone());
            }
        }
    }

    Some(ModuleSpec { kind, options })
}

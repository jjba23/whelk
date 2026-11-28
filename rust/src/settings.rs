// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper and clipboard policies as the Scheme bridge presents them.
//!
//! The C shell and this crate read the same alists from
//! `(shell-wallpaper->alist)` and `(shell-clipboard->alist)`.  Keeping the
//! conversion here, rather than in the FFI layer, makes every default and
//! clamp testable without starting Guile.

use std::path::{Path, PathBuf};

use crate::wallpaper::{Animation, Fit};

pub use crate::bar::SchemeValue;

/// The clipboard policy from `(clipboard-settings)`.  Zero limits mean "keep
/// the module's own default", matching the C bridge's `0`-is-unset rule.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClipboardSettings {
    pub configured: bool,
    pub persist: Option<String>,
    pub persist_only_pinned: bool,
    pub clear_on_lock: bool,
    pub max_entries: u32,
    pub max_entry_bytes: u32,
    pub max_total_bytes: u32,
    pub ttl_seconds: u32,
}

impl ClipboardSettings {
    pub fn from_alist(alist: &[(String, SchemeValue)]) -> Self {
        let mut settings = Self::default();

        if alist.is_empty() {
            return settings;
        }

        settings.configured = bool_at(alist, "configured?").unwrap_or(true);
        settings.persist = match value_at(alist, "persist") {
            Some(SchemeValue::Bool(false)) => None,
            Some(SchemeValue::Bool(true)) => Some("auto".to_owned()),
            Some(SchemeValue::Text(value) | SchemeValue::Symbol(value))
                if !value.is_empty() && value != "#f" =>
            {
                Some(value.clone())
            }
            _ => None,
        };
        settings.persist_only_pinned = bool_at(alist, "persist-only-pinned?").unwrap_or(false);
        settings.clear_on_lock = bool_at(alist, "clear-on-lock?").unwrap_or(false);
        settings.max_entries = positive_integer(integer_at(alist, "max-entries"));
        settings.max_entry_bytes = positive_integer(integer_at(alist, "max-entry-bytes"));
        settings.max_total_bytes = positive_integer(integer_at(alist, "max-total-bytes"));
        settings.ttl_seconds = positive_integer(integer_at(alist, "ttl-seconds"));

        settings
    }

    /// Resolve the configured persistence spelling the way the C shell does:
    /// a value containing `/` is a path, `auto`/`1`/`#t` (or any other word)
    /// selects the state-directory file, and no value means memory only.
    ///
    /// `state_home` and `home` are arguments so the rule is testable without
    /// moving the process's environment.
    pub fn persistence_path(
        &self,
        state_home: Option<&str>,
        home: Option<&str>,
    ) -> Option<PathBuf> {
        let persist = self
            .persist
            .as_deref()
            .filter(|value| !value.is_empty() && *value != "#f" && *value != "0");
        let persist = persist?;

        if persist.contains('/') {
            return Some(PathBuf::from(persist));
        }

        let base = state_home
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.map(|value| Path::new(value).join(".local/state")))
            .unwrap_or_else(|| PathBuf::from(".local/state"));

        Some(base.join("whelk").join("clipboard.variant"))
    }

    pub fn summary(&self) -> String {
        let policy = if self.persist.is_some() {
            "persisted"
        } else {
            "memory only"
        };
        let limits = format!(
            "{} entries · {} MiB total · ttl {}s",
            if self.max_entries == 0 {
                50
            } else {
                self.max_entries
            },
            if self.max_total_bytes == 0 {
                128
            } else {
                self.max_total_bytes / (1024 * 1024)
            },
            self.ttl_seconds
        );

        format!("clipboard: {policy} · {limits}")
    }
}

fn positive_integer(value: Option<i64>) -> u32 {
    value
        .filter(|value| *value > 0)
        .map(|value| value.min(i64::from(u32::MAX)) as u32)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperSettings {
    pub configured: bool,
    pub enabled: bool,
    pub paths: Vec<PathBuf>,
    pub interval_seconds: u32,
    pub shuffle: bool,
    pub animation: Animation,
    pub animation_ms: u32,
    pub fit: Fit,
    pub ken_burns: bool,
    pub video: bool,
    pub mute_video: bool,
    pub current: Option<PathBuf>,
    pub picker_width: u32,
    pub picker_height: u32,
    pub picker_columns: u32,
}

impl Default for WallpaperSettings {
    fn default() -> Self {
        Self {
            configured: false,
            enabled: false,
            paths: Vec::new(),
            interval_seconds: 900,
            shuffle: true,
            animation: Animation::Crossfade,
            animation_ms: 1200,
            fit: Fit::Cover,
            ken_burns: false,
            video: true,
            mute_video: true,
            current: None,
            picker_width: 960,
            picker_height: 640,
            picker_columns: 4,
        }
    }
}

impl WallpaperSettings {
    pub fn from_alist(alist: &[(String, SchemeValue)]) -> Self {
        Self::with_home(alist, std::env::var("HOME").ok().as_deref())
    }

    /// The testable constructor: HOME is an argument, not an ambient read.
    pub fn with_home(alist: &[(String, SchemeValue)], home: Option<&str>) -> Self {
        let mut settings = Self::default();

        if alist.is_empty() {
            return settings;
        }

        settings.configured = bool_at(alist, "configured?").unwrap_or(true);
        settings.enabled = bool_at(alist, "enabled?").unwrap_or(false);
        settings.paths = paths_at(alist, home);
        settings.interval_seconds =
            normalize_interval(integer_at(alist, "interval").unwrap_or(900));
        settings.shuffle = bool_at(alist, "shuffle?").unwrap_or(true);
        settings.animation = text_at(alist, "animation")
            .and_then(|value| Animation::parse(&value))
            .unwrap_or(Animation::Crossfade);
        settings.animation_ms = clamp_integer(
            integer_at(alist, "animation-duration").unwrap_or(1200),
            0,
            10_000,
        );
        settings.fit = text_at(alist, "fit")
            .and_then(|value| Fit::parse(&value))
            .unwrap_or(Fit::Cover);
        settings.ken_burns = bool_at(alist, "ken-burns?").unwrap_or(false);
        settings.video = bool_at(alist, "video?").unwrap_or(true);
        settings.mute_video = bool_at(alist, "mute-video?").unwrap_or(true);
        settings.current = text_at(alist, "current")
            .filter(|value| !value.is_empty())
            .map(|value| expand_home(&value, home));
        settings.picker_width =
            clamp_integer(integer_at(alist, "picker-width").unwrap_or(960), 320, 3840);
        settings.picker_height =
            clamp_integer(integer_at(alist, "picker-height").unwrap_or(640), 240, 2160);
        settings.picker_columns =
            clamp_integer(integer_at(alist, "picker-columns").unwrap_or(4), 1, 12);

        settings
    }

    pub fn summary(&self) -> String {
        if !self.enabled {
            return "wallpaper: off".to_owned();
        }

        let paths = if self.paths.len() == 1 {
            "1 path".to_owned()
        } else {
            format!("{} paths", self.paths.len())
        };

        format!(
            "wallpaper: enabled · {}s · {} · {} · {}",
            self.interval_seconds,
            self.animation.name(),
            self.fit.name(),
            paths
        )
    }
}

fn value_at<'a>(alist: &'a [(String, SchemeValue)], key: &str) -> Option<&'a SchemeValue> {
    alist
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

fn bool_at(alist: &[(String, SchemeValue)], key: &str) -> Option<bool> {
    match value_at(alist, key) {
        Some(SchemeValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn integer_at(alist: &[(String, SchemeValue)], key: &str) -> Option<i64> {
    match value_at(alist, key) {
        Some(SchemeValue::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn text_at(alist: &[(String, SchemeValue)], key: &str) -> Option<String> {
    match value_at(alist, key) {
        Some(SchemeValue::Text(value) | SchemeValue::Symbol(value)) => Some(value.clone()),
        _ => None,
    }
}

fn paths_at(alist: &[(String, SchemeValue)], home: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    match value_at(alist, "paths") {
        Some(SchemeValue::List(values)) => {
            for value in values {
                if let SchemeValue::Text(path) | SchemeValue::Symbol(path) = value {
                    push_path(&mut paths, path, home);
                }
            }
        }
        Some(SchemeValue::Text(path) | SchemeValue::Symbol(path)) => {
            push_path(&mut paths, path, home);
        }
        _ => {}
    }

    paths
}

fn push_path(paths: &mut Vec<PathBuf>, path: &str, home: Option<&str>) {
    if !path.is_empty() {
        paths.push(expand_home(path, home));
    }
}

fn normalize_interval(value: i64) -> u32 {
    if value < 0 {
        0
    } else if value > 0 {
        clamp_integer(value.max(5), 5, 7 * 24 * 60 * 60)
    } else {
        0
    }
}

fn clamp_integer(value: i64, minimum: i64, maximum: i64) -> u32 {
    value.clamp(minimum, maximum) as u32
}

/// Expand a leading `~/` exactly as the Scheme constructor does.  Other
/// spellings, including `~user`, are paths of their own.
pub fn expand_home(path: &str, home: Option<&str>) -> PathBuf {
    match home {
        Some(home) if !home.is_empty() && path.starts_with("~/") => {
            PathBuf::from(format!("{home}{}", &path[1..]))
        }
        _ => PathBuf::from(path),
    }
}

pub fn default_config_path() -> PathBuf {
    config_path_from(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Match `g_get_user_config_dir()`: XDG_CONFIG_HOME wins only when absolute;
/// otherwise the home directory's `.config` is used.
pub fn config_path_from(xdg_config_home: Option<&str>, home: Option<&str>) -> PathBuf {
    let base = xdg_config_home
        .filter(|value| Path::new(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| home.map(|value| Path::new(value).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));

    base.join("whelk").join("settings.scm")
}

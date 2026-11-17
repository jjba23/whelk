// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The configuration snapshot and the loader that can refresh it.
//!
//! Guile is process-global, so the handle lives with the shell instead of being
//! created for one startup read and dropped.  A missing settings file is not an
//! error: it means the defaults, and a later reload can pick up a file that was
//! created while the shell was running.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::bar::BarSettings;
use crate::guile::Guile;
use crate::menuapps::MenuSources;
use crate::preferences::PreferenceValue;
use crate::settings::{ClipboardSettings, WallpaperSettings};
use crate::shortcutmaps::ShortcutMaps;
use crate::startmenu::MenuEntry;
use crate::widgets::WidgetConfig;

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub wallpaper: WallpaperSettings,
    pub bars: Vec<BarSettings>,
    pub idle_lock_after: i32,
    pub clipboard: ClipboardSettings,
    pub menu: Vec<MenuEntry>,
    /// The curated dictionary and Orbit corpus, loaded once per configuration.
    /// Every bar shares the same immutable copy.
    pub menu_sources: Rc<MenuSources>,
    /// The desktop widgets the settings file asks for, in placement order.
    pub widgets: Vec<WidgetConfig>,
    /// The night light the display server should carry, overrides included.
    pub night_light: crate::nightlight::Config,
}

impl LoadedConfig {
    /// The C shell's no-configuration path: the built-in bar and defaults.
    pub fn fallback() -> Self {
        Self {
            wallpaper: WallpaperSettings::default(),
            bars: vec![BarSettings::default()],
            idle_lock_after: 0,
            clipboard: ClipboardSettings::default(),
            menu: MenuEntry::default_menu(),
            menu_sources: Rc::new(MenuSources::default()),
            widgets: Vec::new(),
            night_light: crate::nightlight::Config::default(),
        }
    }
}

pub struct ConfigLoader {
    path: PathBuf,
    scheme_paths: Vec<PathBuf>,
    guile: Option<Guile>,
}

impl ConfigLoader {
    pub fn new(path: PathBuf, scheme_paths: Vec<PathBuf>) -> Self {
        Self {
            path,
            scheme_paths,
            guile: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load a complete snapshot.  The Guile handle is initialized on the first
    /// file-backed load and reused for every later reload.
    pub fn load(&mut self) -> Result<LoadedConfig, String> {
        if !self.path.is_file() {
            return Ok(LoadedConfig::fallback());
        }

        if self.guile.is_none() {
            let guile = Guile::init(&self.scheme_paths)
                .map_err(|error| format!("shell settings could not be loaded: {error}"))?;

            self.guile = Some(guile);
        }

        let guile = self.guile.as_ref().expect("Guile was initialized");

        let wallpaper = guile
            .load_wallpaper(&self.path)
            .map_err(|error| format!("wallpaper settings could not be loaded: {error}"))?;
        let mut bars = guile
            .load_bars(&self.path)
            .map_err(|error| format!("bar settings could not be loaded: {error}"))?;

        if bars.is_empty() {
            bars.push(BarSettings::default());
        }

        let idle_lock_after = guile
            .load_idle_lock_after(&self.path)
            .map_err(|error| format!("idle settings could not be loaded: {error}"))?;
        let clipboard = guile
            .load_clipboard(&self.path)
            .map_err(|error| format!("clipboard settings could not be loaded: {error}"))?;
        // A menu is configuration, not a reason to refuse a valid shell.  The
        // C bridge returns its short fallback when the Scheme side cannot
        // answer; this does the same, with the full default menu instead.
        let menu = guile.load_shell_menu(&self.path).unwrap_or_else(|error| {
            eprintln!("whelk: shell menu could not be loaded: {error}; using the default menu");

            MenuEntry::default_menu()
        });
        // The dictionary and Orbit corpus are fallbacks, not a reason to
        // refuse an otherwise valid configuration.  A missing or broken
        // module leaves the shared Edit/View and window verbs in place.
        let mut menu_sources = guile.load_menu_sources(&self.path).unwrap_or_else(|error| {
            eprintln!(
                "whelk: application menu sources could not be loaded: {error}; using shared menus only"
            );

            MenuSources::default()
        });

        // This is independent of Guile: a user can install an Orbit JSON map
        // even when the Scheme modules are unavailable.
        menu_sources.shortcut_maps = ShortcutMaps::load();

        // A desktop widget that cannot be read is not a reason to refuse the
        // shell: the bars it was placed around are still valid.
        let widgets = guile.load_widgets(&self.path).unwrap_or_else(|error| {
            eprintln!("whelk: desktop widgets could not be loaded: {error}; drawing none");

            Vec::new()
        });

        // A night light that cannot be read leaves the shell usable and the
        // filter off, the same way a broken widget list draws no cards.
        let night_light = guile.load_night_light(&self.path).unwrap_or_else(|error| {
            eprintln!("whelk: night light settings could not be loaded: {error}; leaving it off");

            crate::nightlight::Config::default()
        });

        Ok(LoadedConfig {
            wallpaper,
            bars,
            idle_lock_after,
            clipboard,
            menu,
            menu_sources: Rc::new(menu_sources),
            widgets,
            night_light,
        })
    }

    /// The label the bar's name widget wears: the real path, or the deprecated
    /// but useful note that this process is running from defaults.
    /// Persist a wallpaper choice through the same `(whelk prefs)` override the
    /// settings window writes, so the shell and the settings file never
    /// disagree about which picture is current.
    pub fn remember_wallpaper(&self, wallpaper: &Path) -> Result<(), String> {
        let guile = self
            .guile
            .as_ref()
            .ok_or_else(|| "the configuration has not been loaded yet".to_owned())?;

        guile
            .remember_wallpaper(&self.path, wallpaper)
            .map_err(|error| error.to_string())
    }

    /// A Guile handle, initialized on first use so a configuration that was
    /// missing at startup can still be given GUI overrides.
    fn guile_mut(&mut self) -> Result<&Guile, String> {
        if self.guile.is_none() {
            let guile = Guile::init(&self.scheme_paths)
                .map_err(|error| format!("Guile could not be initialized: {error}"))?;

            self.guile = Some(guile);
        }

        Ok(self.guile.as_ref().expect("Guile was initialized"))
    }

    /// Save one settings-window change as a GUI override.
    pub fn save_preference(&mut self, key: &str, value: &PreferenceValue) -> Result<(), String> {
        let path = self.path.clone();
        let guile = self.guile_mut()?;
        let scheme = value.to_scheme();

        guile
            .save_preference(&path, key, &scheme)
            .map_err(|error| error.to_string())
    }

    /// Delete the GUI overrides; the settings file is not touched.
    pub fn reset_preferences(&mut self) -> Result<bool, String> {
        let path = self.path.clone();
        let guile = self.guile_mut()?;

        guile
            .reset_preferences(&path)
            .map_err(|error| error.to_string())
    }

    /// One saved override as text, for the fields whose value is not on the bar.
    pub fn preference_text(&mut self, key: &str) -> Option<String> {
        let path = self.path.clone();

        match self.guile_mut() {
            Ok(guile) => match guile.read_preference(&path, key) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("whelk: could not read '{key}': {error}");

                    None
                }
            },
            Err(error) => {
                eprintln!("whelk: could not read '{key}': {error}");

                None
            }
        }
    }

    /// The theme names the window offers, empty when Scheme cannot answer.
    pub fn theme_names(&mut self) -> Vec<String> {
        match self.guile_mut() {
            Ok(guile) => guile.theme_names().unwrap_or_else(|error| {
                eprintln!("whelk: could not list the themes: {error}");

                Vec::new()
            }),
            Err(error) => {
                eprintln!("whelk: could not list the themes: {error}");

                Vec::new()
            }
        }
    }

    /// The night light the window edits, defaults included.
    pub fn night_light(&mut self) -> Option<crate::nightlight::Config> {
        let path = self.path.clone();
        let guile = self.guile_mut().ok()?;

        match guile.load_night_light(&path) {
            Ok(config) => Some(config),
            Err(error) => {
                eprintln!("whelk: could not read the night light: {error}");

                None
            }
        }
    }

    pub fn label(&self) -> String {
        if self.path.is_file() {
            self.path.display().to_string()
        } else {
            format!("{} (missing; using defaults)", self.path.display())
        }
    }
}

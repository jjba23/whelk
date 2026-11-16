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

use crate::bar::BarSettings;
use crate::guile::Guile;
use crate::settings::{ClipboardSettings, WallpaperSettings};
use crate::startmenu::MenuEntry;

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub wallpaper: WallpaperSettings,
    pub bars: Vec<BarSettings>,
    pub idle_lock_after: i32,
    pub clipboard: ClipboardSettings,
    pub menu: Vec<MenuEntry>,
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
            eprintln!("whelk-rs: shell menu could not be loaded: {error}; using the default menu");

            MenuEntry::default_menu()
        });

        Ok(LoadedConfig {
            wallpaper,
            bars,
            idle_lock_after,
            clipboard,
            menu,
        })
    }

    /// The label the bar's name widget wears: the real path, or the deprecated
    /// but useful note that this process is running from defaults.
    pub fn label(&self) -> String {
        if self.path.is_file() {
            self.path.display().to_string()
        } else {
            format!("{} (missing; using defaults)", self.path.display())
        }
    }
}

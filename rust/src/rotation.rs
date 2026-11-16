// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper engine's clock, without the clock.
//!
//! `resolve_selected()` and `rotate_once()` in `src/wallpaper.c` decide which
//! file is current; GTK only draws it.  This module owns that decision so a
//! timer can be tested with a seed instead of a compositor.

use std::path::{Path, PathBuf};

use crate::collection::{WallpaperItem, is_usable};
use crate::wallpaper::next_index;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    items: Vec<WallpaperItem>,
    selected: Option<PathBuf>,
    include_video: bool,
    shuffle_state: u64,
}

impl Playlist {
    pub fn new(items: Vec<WallpaperItem>, include_video: bool, shuffle_state: u64) -> Self {
        Self {
            items,
            selected: None,
            include_video,
            shuffle_state,
        }
    }

    pub fn items(&self) -> &[WallpaperItem] {
        &self.items
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    /// Choose a usable file explicitly, as the picker does.  A file outside the
    /// collection is allowed, matching `whelk_wallpaper_engine_apply()`.
    pub fn select(&mut self, path: &Path) -> bool {
        if !is_usable(path, self.include_video) {
            return false;
        }

        self.selected = Some(path.to_owned());
        true
    }

    /// Keep a usable current file; otherwise prefer the configured start and
    /// finally the first collected item.
    pub fn resolve(&mut self, preferred: Option<&Path>) {
        if self
            .selected
            .as_deref()
            .is_some_and(|path| is_usable(path, self.include_video))
        {
            return;
        }

        self.selected = None;

        if let Some(preferred) = preferred
            && is_usable(preferred, self.include_video)
        {
            self.selected = Some(preferred.to_owned());
            return;
        }

        self.selected = self.items.first().map(|item| item.path.clone());
    }

    /// Move to the next item and return it.  If the selected file is outside
    /// the collection, C starts the lookup at index zero and so does this.
    pub fn advance(&mut self, shuffle: bool) -> Option<&Path> {
        if self.items.is_empty() || self.selected.is_none() {
            return None;
        }

        let current = self
            .selected
            .as_deref()
            .and_then(|selected| {
                self.items
                    .iter()
                    .position(|item| item.path.as_path() == selected)
            })
            .unwrap_or(0);

        let next = next_index(self.items.len(), current, shuffle, &mut self.shuffle_state);

        self.selected = Some(self.items[next].path.clone());
        self.items.get(next).map(|item| item.path.as_path())
    }

    /// The C engine's `restart_rotation()` guard: an enabled engine, a positive
    /// interval, at least two files and a current selection.
    pub fn should_rotate(&self, enabled: bool, interval_seconds: u32) -> bool {
        enabled
            && interval_seconds > 0
            && self.items.len() >= 2
            && self
                .selected
                .as_deref()
                .is_some_and(|path| is_usable(path, self.include_video))
    }
}

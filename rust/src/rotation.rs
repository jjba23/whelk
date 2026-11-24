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

        self.selected = preferred
            .filter(|path| is_usable(path, self.include_video))
            .map(Path::to_owned)
            .or_else(|| self.items.first().map(|item| item.path.clone()));
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
        let path = self.items.get(next)?.path.clone();

        self.selected = Some(path);

        self.selected.as_deref()
    }

    /// The C engine's `restart_rotation()` guard: an enabled engine, a positive
    /// interval, at least two files and a current selection.  The selected file
    /// does not have to still exist: a deletion must not stop the clock, because
    /// the next tick advances to the next collected item.
    pub fn should_rotate(&self, enabled: bool, interval_seconds: u32) -> bool {
        enabled && interval_seconds > 0 && self.items.len() >= 2 && self.selected.is_some()
    }
}

/// The picture one step from `current`, without a playlist to keep.
///
/// This is what a context menu's *Next* and *Previous* rows use: the collection
/// is re-read from the configuration, the current picture is where the step
/// starts, and `by` is signed.  A shuffle picks an item that is not the one
/// already on screen.  A current picture outside the collection starts the
/// walk at the first item, exactly as [`Playlist::advance`] does.
pub fn step_path(
    items: &[WallpaperItem],
    current: Option<&Path>,
    by: i32,
    shuffle: bool,
    shuffle_state: u64,
) -> Option<PathBuf> {
    if items.is_empty() {
        return None;
    }

    let index = current
        .and_then(|selected| {
            items
                .iter()
                .position(|item| item.path.as_path() == selected)
        })
        .unwrap_or(0);
    let next = if shuffle {
        let mut state = shuffle_state;

        next_index(items.len(), index, true, &mut state)
    } else {
        (index as i32 + by).rem_euclid(items.len() as i32) as usize
    };

    items.get(next).map(|item| item.path.clone())
}

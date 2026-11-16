// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper collection, independent of GTK and the display server.
//!
//! This mirrors `rescan()` in `src/wallpaper.c`: configured paths are walked to
//! the same depth, hidden directory entries are skipped, directory symlinks are
//! not followed (a self-referential link must not loop), file symlinks are
//! accepted when they resolve to a regular file, videos obey the `video?`
//! switch, and the explicit current file may live outside every root.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::wallpaper::{Kind, kind_for_path};

const MAX_DEPTH: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperItem {
    pub path: PathBuf,
    pub kind: Kind,
}

/// Collect the configured roots plus the optional current file.
///
/// Paths are expected to be expanded already (the Scheme constructor does
/// that).  The returned items are deduplicated by path and sorted as C's
/// `g_strcmp0` sorts them.
pub fn collect(
    roots: &[PathBuf],
    current: Option<&Path>,
    include_video: bool,
) -> Vec<WallpaperItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        scan(root, 0, include_video, &mut seen, &mut items);
    }

    if let Some(current) = current {
        add_item(current, include_video, &mut seen, &mut items);
    }

    items.sort_by(|left, right| left.path.as_os_str().cmp(right.path.as_os_str()));
    items
}

/// Whether a selected path can be shown under the current video policy.
pub fn is_usable(path: &Path, include_video: bool) -> bool {
    let kind = kind_for_path(path);

    if kind == Kind::Unknown || (kind == Kind::Video && !include_video) {
        return false;
    }

    path.is_file()
}

fn scan(
    path: &Path,
    depth: usize,
    include_video: bool,
    seen: &mut HashSet<PathBuf>,
    items: &mut Vec<WallpaperItem>,
) {
    if depth > MAX_DEPTH {
        return;
    }

    // symlink_metadata() does not follow the link.  A real directory is walked;
    // a directory symlink falls through to add_item(), where is_file() is false,
    // so it is ignored rather than followed.
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };

    if metadata.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };

        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }

            scan(&entry.path(), depth + 1, include_video, seen, items);
        }

        return;
    }

    add_item(path, include_video, seen, items);
}

fn add_item(
    path: &Path,
    include_video: bool,
    seen: &mut HashSet<PathBuf>,
    items: &mut Vec<WallpaperItem>,
) {
    if !is_usable(path, include_video) || seen.contains(path) {
        return;
    }

    seen.insert(path.to_owned());
    items.push(WallpaperItem {
        path: path.to_owned(),
        kind: kind_for_path(path),
    });
}

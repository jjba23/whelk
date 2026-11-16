// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A tray item's own icon file, ported from `util/trayicon.c`.
//!
//! StatusNotifierItem apps may ship their own icon theme beside the running
//! binary and publish its directory as IconThemePath.  GTK does not search that
//! directory by default, so an icon such as JetBrains Toolbox's
//! `toolbox-tray-color` can fall back to a monochrome desktop-theme icon.

use std::path::Path;

/// The app-provided file, preferring an exact name, then PNG, SVG and XPM, or
/// `None` when the path is relative, the name would escape it, or no such file
/// exists.
pub fn tray_icon_file(theme_path: &str, icon_name: &str) -> Option<String> {
    if theme_path.is_empty() || !Path::new(theme_path).is_absolute() {
        return None;
    }

    if icon_name.is_empty() || icon_name.contains(['/', '\\']) {
        return None;
    }

    let directory = Path::new(theme_path);

    for suffix in ["", ".png", ".svg", ".xpm"] {
        let candidate = directory.join(format!("{icon_name}{suffix}"));

        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }

    None
}

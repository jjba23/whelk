// whelk-launcher -- a launcher overlay for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher split out of [whelk](https://github.com/jjba23/whelk).
//!
//! The project owns the launcher overlay, its modes and key map, application,
//! command, SSH, window, file and menu data, fuzzy ranking, the calculator and
//! the card geometry.  A host supplies clipboard history, the focused
//! application's menus, a stylesheet and its theme; without those the launcher
//! still starts and shows its own empty states.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod calc;
pub mod command;
pub mod filesearch;
pub mod format;
pub mod fuzzy;
pub mod host;
pub mod json;
pub mod launcher;
pub mod launcherdata;
pub mod launcherkeys;
pub mod launcherpanel;
pub mod power;
pub mod sources;
pub mod style;

pub use host::{ClipboardPort, ClipboardRow, ClipboardStatus, LauncherHost, MenuRow, MenuSnapshot};
pub use launcherpanel::Launcher;
pub use style::{LauncherSettings, LauncherStyle};

/// The version of this crate, for a host's about page.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Compatibility re-export of the separate launcher project.
//!
//! WheLK installs a [`crate::launcherhost::WhelkLauncherHost`] so the launcher
//! can read this shell's clipboard and focused menus.

pub use whelk_launcher::launcherpanel::*;

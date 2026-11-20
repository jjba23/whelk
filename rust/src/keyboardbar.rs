// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Caps Lock, ported from `modules/keyboard.c`.
//!
//! No Wayland protocol gives a client the seat's modifier state, but every
//! keyboard's Caps Lock LED is readable in sysfs.  The reading is empty while
//! unlocked, so the module collapses instead of spending bar width saying
//! "nothing is locked".
//!
//! A machine whose keyboards have no Caps Lock LED at all is told apart from
//! one where the lock is simply off: the module steps out of the row on the
//! first and stays ready on the second, so a desktop without the LED does not
//! look like a lock that never engages.

use std::fs;
use std::path::{Path, PathBuf};

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
pub const DEFAULT_TOOLTIP: &str = "Caps Lock is on";
const DEFAULT_FORMAT: &str = "⇪";
const LEDS_ROOT: &str = "/sys/class/leds";

/// Every Caps Lock LED under `root`, in a stable order.
pub fn capslock_leds_at(root: &Path) -> Vec<PathBuf> {
    let mut entries = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("::capslock"))
            })
            .collect::<Vec<PathBuf>>(),
        Err(_) => return Vec::new(),
    };

    entries.sort();
    entries
}

/// Whether this machine reports a Caps Lock LED at all.  A desktop whose
/// keyboards have none can never show the reading, so the module hides.
pub fn has_capslock_led_at(root: &Path) -> bool {
    !capslock_leds_at(root).is_empty()
}

pub fn capslock_active_at(root: &Path) -> bool {
    capslock_leds_at(root).into_iter().any(|path| {
        fs::read_to_string(path.join("brightness"))
            .ok()
            .map(|text| text.trim() != "0")
            .unwrap_or(false)
    })
}

pub fn has_capslock_led() -> bool {
    has_capslock_led_at(Path::new(LEDS_ROOT))
}

pub fn capslock_active() -> bool {
    capslock_active_at(Path::new(LEDS_ROOT))
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(update);

    Some(root)
}

fn update(host: &ModuleHost) {
    // No LED to read: this machine has nothing to say, so it says it once and
    // leaves the row its width back.
    if !has_capslock_led() {
        host.set_visible(false);
        host.set_text("");
        host.remove_class("locked");

        return;
    }

    if !capslock_active() {
        host.set_visible(false);
        host.set_text("");
        host.remove_class("locked");

        return;
    }

    let icon = host.settings().icon.clone().unwrap_or_default();
    let format = host
        .settings()
        .format
        .clone()
        .unwrap_or_else(|| DEFAULT_FORMAT.to_owned());
    let values = [("state", "on")];

    host.set_visible(true);
    host.add_class("locked");
    host.present_with(&format, &icon, Some(DEFAULT_TOOLTIP), &values);
}

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

use std::fs;
use std::path::{Path, PathBuf};

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
const LEDS_ROOT: &str = "/sys/class/leds";

pub fn capslock_active_at(root: &Path) -> bool {
    let mut entries = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect::<Vec<PathBuf>>(),
        Err(_) => return false,
    };

    entries.sort();

    entries.into_iter().any(|path| {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };

        if !name.ends_with("::capslock") {
            return false;
        }

        fs::read_to_string(path.join("brightness"))
            .ok()
            .map(|text| text.trim() != "0")
            .unwrap_or(false)
    })
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
    if !capslock_active() {
        host.set_visible(false);
        host.set_text("");

        return;
    }

    host.set_visible(true);
    host.add_class("locked");
    host.present("⇪", &[("state", "on")]);
}

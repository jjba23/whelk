// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The active power-profiles-daemon profile, ported from
//! `modules/power_profile.c`.
//!
//! A machine without the daemon answers nothing and the module hides itself:
//! an indicator for a profile that cannot be chosen is decoration.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::format;

pub const COMMAND: &str = "powerprofilesctl get";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;

pub fn reading(text: &str) -> Option<String> {
    let profile = format::first_line(text).trim();

    (!profile.is_empty()).then(|| profile.to_owned())
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
    let callback_host = host.clone();

    host.run_command(COMMAND, move |text| {
        let Some(profile) = text.as_deref().and_then(reading) else {
            callback_host.set_visible(false);
            callback_host.set_text("");

            return;
        };

        callback_host.set_visible(true);

        let values = [("profile", profile.as_str())];

        callback_host.present("{profile}", &values);
    });
}

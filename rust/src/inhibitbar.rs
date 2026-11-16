// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The inhibitor readout, ported from `modules/inhibit.c` and
//! `util/inhibitors.c`.
//!
//! `systemd-inhibit --list` pads its columns, so a run of two or more spaces is
//! a boundary while a single space belongs to a program name or reason.  This
//! module never takes a lock; it names the process that did.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::inhibitors::{describe, parse};

pub const COMMAND: &str = "systemd-inhibit --list --no-pager 2>/dev/null";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 10;
const EMPTY_TOOLTIP: &str = "No inhibitor is held: idle policy can blank the screen";

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(move |host| update(&module, host));

    Some(root)
}

fn update(module: &ModuleSpec, host: &ModuleHost) {
    let callback_host = host.clone();
    let module = module.clone();

    host.run_command(COMMAND, move |text| {
        let text = text.unwrap_or_default();
        let inhibitors = parse(&text);
        let show_off = module.boolean("show-off?").unwrap_or(false);

        if inhibitors.is_empty() && !show_off {
            callback_host.set_visible(false);
            callback_host.set_text("");

            return;
        }

        callback_host.set_visible(true);

        let count = inhibitors.len().to_string();
        let summary = describe(&inhibitors);
        let tooltip = summary.as_deref().unwrap_or(EMPTY_TOOLTIP);
        let format = callback_host
            .settings()
            .format
            .as_deref()
            .unwrap_or("{icon} {count}");
        let icon = callback_host.settings().icon.as_deref().unwrap_or("");
        let values = [("count", count.as_str())];

        callback_host.present_with(format, icon, Some(tooltip), &values);
    });
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The custom-command reading, ported from `modules/custom.c`.
//!
//! The reading is the command's first line.  `#:interval 0` makes it a static
//! label for a glyph that only exists to be clicked.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::format;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;

/// The first line, shortened to `max_length` characters; zero means no limit.
pub fn reading(text: &str, max_length: i32) -> String {
    format::truncate_utf8(format::first_line(text), max_length)
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let command = module.text("exec").filter(|value| !value.is_empty());
    let max_length = module
        .integer("max-length")
        .unwrap_or(0)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    if let Some(label) = host.add_label()
        && max_length > 0
    {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    }

    host.start(move |host| update(command.as_deref(), max_length, host));

    Some(root)
}

fn update(command: Option<&str>, max_length: i32, host: &ModuleHost) {
    let Some(command) = command else {
        host.set_text("");

        return;
    };
    let callback_host = host.clone();

    host.run_command(command, move |text| {
        let Some(text) = text else {
            return;
        };
        let text = reading(&text, max_length);
        let values = [("text", text.as_str())];

        callback_host.present("{text}", &values);
    });
}

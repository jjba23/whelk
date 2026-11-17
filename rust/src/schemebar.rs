// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `scheme` bar module, ported from `modules/scheme.c`.
//!
//! The module that is a procedure.  `(scheme-module #:update PROC)` stores PROC
//! in the Scheme-side registry and hands the module a token; on every refresh
//! [`crate::guile::widget::call_module_widget`] calls PROC and gives back the
//! widget it built.  The procedure runs on the GTK thread, so it can touch GTK
//! through the `whelk-widget-*` primitives, but it can also read a file, a
//! socket or an API -- from the bar's point of view those are all just a
//! reading that returns a widget.
//!
//! A failed or empty reading leaves the previous widget alone.  That is the
//! same contract a command-backed module has: a slow program does not erase the
//! last value it printed.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::guile::widget::MISSING_UPDATE;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;

/// The token `#:update` carries, when the module has one.  A missing or empty
/// token is a configuration mistake the module reports instead of polling.
pub fn update_token(module: &ModuleSpec) -> Option<String> {
    module.text("update").filter(|token| !token.is_empty())
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    root.add_css_class("whelk-scheme");

    let Some(token) = update_token(module) else {
        eprintln!(
            "whelk-rs: scheme-module has no #:update procedure: it shows {}",
            MISSING_UPDATE.trim_start_matches("scheme: ")
        );

        if let Some(label) = host.add_label() {
            label.set_text(MISSING_UPDATE);
        }

        return Some(root);
    };

    // The slot holds the current widget.  Swapping its child is what keeps the
    // module's own root (and its CSS, padding and clicks) in place while the
    // reading changes.
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    slot.set_valign(gtk::Align::Center);
    root.append(&slot);

    host.start(move |host| update(host, &token, &slot));

    Some(root)
}

fn update(host: &ModuleHost, token: &str, slot: &gtk::Box) {
    if !host.is_alive() {
        return;
    }

    let Some(widget) = crate::guile::widget::call_module_widget(token) else {
        return;
    };

    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }

    widget.set_valign(gtk::Align::Center);
    widget.add_css_class("whelk-scheme-widget");
    slot.append(&widget);
}

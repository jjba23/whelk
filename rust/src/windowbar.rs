// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The focused-window reading, ported from `modules/window.c`.
//!
//! The session parser supplies the title and application id; the format
//! chooses which one the label says, and `#:max-length` keeps a long title from
//! eating the row.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::format;
use crate::riverstatus;
use crate::session::{FocusedWindow, Session};

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
pub const DEFAULT_MAX_LENGTH: i32 = 50;

pub fn focused_reading(session: Session, text: &str, max_length: i32) -> Option<FocusedWindow> {
    let window = session.parse_focused(text)?;

    Some(FocusedWindow {
        title: format::truncate_utf8(&window.title, max_length),
        app_id: window.app_id,
    })
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let session = context.session;
    let max_length = module
        .integer("max-length")
        .unwrap_or(i64::from(DEFAULT_MAX_LENGTH))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    if let Some(label) = host.add_label() {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_xalign(0.0);
    }

    if session == Session::River {
        start_river_push(&root, &host, max_length);
    }

    host.start(move |host| update(session, max_length, host));

    Some(root)
}

fn update(session: Session, max_length: i32, host: &ModuleHost) {
    // River has no focused-window command; its title arrives on the status
    // socket started beside this module.
    if session == Session::River {
        return;
    }

    let Some(command_line) = session.focused_window_command() else {
        host.set_text("");

        return;
    };
    let callback_host = host.clone();

    host.run_command(&command_line, move |text| {
        let Some(text) = text else {
            return;
        };
        let Some(window) = focused_reading(session, &text, max_length) else {
            callback_host.set_text("");

            return;
        };

        present_focused(&callback_host, &window);
    });
}

fn start_river_push(root: &gtk::Box, host: &ModuleHost, max_length: i32) {
    let weak = host.downgrade();
    let Some(watch) = riverstatus::Watch::start(move |title| {
        let Some(host) = weak.upgrade() else {
            return;
        };
        let Some(window) = focused_reading(Session::River, title, max_length) else {
            host.set_text("");

            return;
        };

        present_focused(&host, &window);
    }) else {
        // A compositor without river-status leaves the module empty, exactly
        // like a session with no focused-window IPC.
        return;
    };
    // The widget's destroy handler owns the watch: the proxy and the socket
    // watch live exactly as long as the bar it draws into.
    let slot = Rc::new(RefCell::new(Some(watch)));
    let on_destroy = Rc::clone(&slot);

    root.connect_destroy(move |_| {
        on_destroy.borrow_mut().take();
    });
}

fn present_focused(host: &ModuleHost, window: &FocusedWindow) {
    let values = [
        ("title", window.title.as_str()),
        ("app-id", window.app_id.as_str()),
    ];

    host.present("{title}", &values);
}

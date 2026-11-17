// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The UI's termination signals, ported from the C shell's `g_unix_signal_add`
//! handlers.  The handler only records the signal; GTK's main loop turns it
//! into `application.quit()`, so shutdown runs the ordinary path rather than
//! dropping the process in the middle of a paint.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::posix::{TERMINATION_SIGNALS, install_signal_handler};

static PENDING: AtomicI32 = AtomicI32::new(0);

extern "C" fn record(signal: libc::c_int) {
    PENDING.store(signal, Ordering::SeqCst);
}

pub fn install() -> Result<(), String> {
    install_signal_handler(record, &TERMINATION_SIGNALS)
        .map_err(|error| format!("could not install termination handlers: {error}"))
}

fn take() -> Option<libc::c_int> {
    let signal = PENDING.swap(0, Ordering::SeqCst);

    (signal != 0).then_some(signal)
}

/// Poll the handler's slot on GTK's main loop.  `g_unix_signal_add` is not
/// exposed by the Rust GLib bindings; a 100 ms poll is the same ordering
/// guarantee for a shell shutdown and avoids a second event loop.
pub fn quit_source(application: &gtk::Application) -> gtk::glib::SourceId {
    let application = application.clone();

    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        if let Some(signal) = take() {
            eprintln!("whelk: termination signal {signal} received: shutting down");
            application.quit();

            gtk::glib::ControlFlow::Break
        } else {
            gtk::glib::ControlFlow::Continue
        }
    })
}

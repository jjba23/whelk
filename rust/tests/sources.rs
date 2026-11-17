// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// A one-shot main-loop source is gone the moment it fires, and a module that
// cancels its work often holds exactly such an id: the deadline lost the race
// with the command it was watching.  Removing it must be a no-op.  GLib calls
// it a programmer error, the gtk-rs wrapper turns that into a panic, and the
// panic lands inside a GIO or GTK callback, which cannot unwind -- so the whole
// shell aborts instead of one bar's reading going stale.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use whelk_rs::sources;

/// A `GMainContext` has one owner at a time, and the test harness runs tests in
/// parallel, so these tests take turns owning it.
static CONTEXT_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn removing_a_fired_source_is_a_no_op() {
    let _serial = CONTEXT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let context = gtk4::glib::MainContext::default();
    let _guard = context.acquire().expect("the main context");
    let fired = Rc::new(Cell::new(false));
    let id = {
        let fired = Rc::clone(&fired);

        gtk4::glib::timeout_add_local_once(Duration::from_millis(10), move || fired.set(true))
    };

    while !fired.get() {
        context.iteration(true);
    }

    // The iteration that ran the closure also destroyed the one-shot source;
    // this is the stale id a module still holds afterwards.
    assert!(
        context.find_source_by_id(&id).is_none(),
        "a one-shot source should be gone once it has fired"
    );

    sources::remove(id);
}

#[test]
fn removing_a_pending_source_cancels_it() {
    let _serial = CONTEXT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let context = gtk4::glib::MainContext::default();
    let _guard = context.acquire().expect("the main context");
    let fired = Rc::new(Cell::new(false));
    let id = {
        let fired = Rc::clone(&fired);

        gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || fired.set(true))
    };

    sources::remove(id);

    let deadline = Instant::now() + Duration::from_millis(300);

    while Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(!fired.get(), "a source that was removed must not run");
}

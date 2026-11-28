// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Removing main-loop sources without aborting the shell.
//!
//! [`SourceId::remove`] is not a request that can fail: GLib treats the id of a
//! source that no longer exists as a programmer error, prints a critical and
//! returns `FALSE`, and the gtk-rs wrapper unwraps that into a panic.  A
//! one-shot source is gone the moment it fires, so every stored id is a
//! potential stale one -- and the cancellation that removes it usually runs
//! inside a GIO or GTK callback, which cannot unwind.  The panic therefore
//! aborts the whole shell instead of ending one bar's work.
//!
//! [`SourceId::remove`]: gtk::glib::SourceId::remove

use gtk4 as gtk;

/// Removes `source` if it is still attached to the default main loop.
///
/// A source that already fired, or that was already removed, has nothing left
/// to cancel; that is the normal case for a one-shot deadline that lost the
/// race with the work it was watching.
pub fn remove(source: gtk::glib::SourceId) {
    if let Some(source) = gtk::glib::MainContext::default().find_source_by_id(&source) {
        source.destroy();
    }
}

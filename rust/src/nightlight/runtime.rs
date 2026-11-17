// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The clock that drives the night light.
//!
//! A GLib timer wakes the engine at the schedule's next edge (five seconds
//! during a fade, a minute at the window's edge, never while the filter is off
//! or always-on).  A settings reload applies the new policy without reopening
//! the display, and shutting the shell down hands every output back.

use std::cell::RefCell;

use gtk::glib;
use gtk4 as gtk;

use super::backend::{self, Platform};
use super::{Config, Engine};

struct Runtime {
    engine: Engine<Platform>,
    timer: Option<glib::SourceId>,
}

thread_local! {
    /// One engine for the shell's lifetime; a second `start` closes the first.
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Open the backend the session offers and apply CONFIG now.  A session whose
/// display server cannot carry a gamma table keeps the setting and logs why.
pub fn start(config: Config) {
    stop();

    let Some(platform) = backend::open() else {
        return;
    };
    let mut runtime = Runtime {
        engine: Engine::new(config, platform),
        timer: None,
    };

    tick(&mut runtime);

    RUNTIME.with(|slot| *slot.borrow_mut() = Some(runtime));
}

/// Re-read the settings and apply them without reopening the display.
pub fn update(config: Config) {
    RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();

        let Some(runtime) = slot.as_mut() else {
            return;
        };

        runtime.engine.set_config(config);
        tick(runtime);
    });
}

/// Hand every output back and forget the timer.
pub fn stop() {
    RUNTIME.with(|slot| {
        let Some(mut runtime) = slot.borrow_mut().take() else {
            return;
        };

        if let Some(timer) = runtime.timer.take() {
            timer.remove();
        }

        runtime.engine.restore();
    });
}

fn tick(runtime: &mut Runtime) {
    if let Some(timer) = runtime.timer.take() {
        timer.remove();
    }

    let Some(seconds) = runtime.engine.tick(now_minutes()) else {
        return;
    };

    let source = glib::timeout_add_seconds_local(seconds, move || {
        RUNTIME.with(|slot| {
            if let Some(runtime) = slot.borrow_mut().as_mut() {
                runtime.timer = None;
                tick(runtime);
            }
        });

        glib::ControlFlow::Break
    });

    runtime.timer = Some(source);
}

/// Local time in minutes, with seconds as a fraction, as C's `now_minutes`.
fn now_minutes() -> f64 {
    let Ok(now) = glib::DateTime::now_local() else {
        return 0.0;
    };

    f64::from(now.hour() * 60 + now.minute()) + f64::from(now.second()) / 60.0
}

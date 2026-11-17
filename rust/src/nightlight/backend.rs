// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Which display server's gamma the night light writes to.
//!
//! C picks the backend from the session: a Wayland display means the
//! compositor's `wlr-gamma-control` (and nothing else — a failing compositor
//! does not fall through to X11 in the same session), otherwise X11's RandR.

use super::wayland::WaylandBackend;
use super::x11::X11Backend;
use super::{Backend, Rgb};

/// The backend the environment calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Wayland,
    X11,
    /// A session with neither variable: there is no screen to warm.
    Unavailable,
}

pub fn choice(wayland_display: Option<&str>, x11_display: Option<&str>) -> Choice {
    if wayland_display.is_some_and(|display| !display.is_empty()) {
        Choice::Wayland
    } else if x11_display.is_some_and(|display| !display.is_empty()) {
        Choice::X11
    } else {
        Choice::Unavailable
    }
}

/// One open backend, whichever the session has.
pub enum Platform {
    Wayland(WaylandBackend),
    /// The X11 connection is much larger than the Wayland one; box it so the
    /// engine's backend does not carry the larger size everywhere.
    X11(Box<X11Backend>),
}

impl Backend for Platform {
    fn set_color(&mut self, rgb: Rgb) -> bool {
        match self {
            Self::Wayland(backend) => backend.set_color(rgb),
            Self::X11(backend) => backend.set_color(rgb),
        }
    }

    fn restore(&mut self) {
        match self {
            Self::Wayland(backend) => backend.restore(),
            Self::X11(backend) => backend.restore(),
        }
    }
}

/// Open the backend the environment selects, with the same warning C logs
/// when the display server refuses to carry a gamma table.
pub fn open() -> Option<Platform> {
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let x11 = std::env::var("DISPLAY").ok();

    match choice(wayland.as_deref(), x11.as_deref()) {
        Choice::Wayland => match WaylandBackend::new() {
            Some(backend) => Some(Platform::Wayland(backend)),
            None => {
                eprintln!(
                    "whelk-rs: night light: this Wayland compositor does not offer \
                     wlr-gamma-control-unstable-v1, so whelk cannot change the screen's gamma; \
                     the setting is kept for a session that can"
                );

                None
            }
        },
        Choice::X11 => match X11Backend::new() {
            Some(backend) => Some(Platform::X11(Box::new(backend))),
            None => {
                eprintln!(
                    "whelk-rs: night light: this X display has no RandR gamma table whelk can \
                     use; the setting is kept for a session that has one"
                );

                None
            }
        },
        Choice::Unavailable => {
            eprintln!("whelk-rs: night light: no Wayland or X display to warm");

            None
        }
    }
}

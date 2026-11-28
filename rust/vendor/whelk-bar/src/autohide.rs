// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A bar that gets out of the way.
//!
//! `#:auto-hide? #t` keeps the bar off the screen until it is wanted, and the
//! pixels are really given back: a layer surface that is not mapped reserves no
//! exclusive zone, so a maximised window grows into the space instead of leaving
//! a strip of desktop behind.  Two things ask for the bar:
//!
//! - the *hot edge*: a [`HOT_EDGE`]-thick strip of surface at the output's edge,
//!   which reveals the bar when the pointer reaches it.  It exists only while
//!   the bar is hidden, so it cannot swallow a click meant for the bar itself;
//! - `whelk --reveal-bar`, so a keybinding can do it too.
//!
//! The bar goes away again a configured delay after the pointer leaves it --
//! not while a popover is open, because a bar that vanishes under a menu the
//! reader is using is worse than one that takes a few pixels.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge};
use crate::barmodule;
use crate::sources;
use crate::surface::{edge_for, layer_for, layer_shell_available};

/// How thick the reveal strip at the output's edge is, in pixels.  Thick enough
/// to hit with a pointer thrown at the edge, thin enough not to be a bar.
pub const HOT_EDGE: i32 = 4;

/// The layer namespace of the strip, so a compositor rule can leave it alone.
pub const HOT_EDGE_NAMESPACE: &str = "whelk-rust-hotedge";

struct Entry {
    window: gtk::ApplicationWindow,
    /// The reveal strip; #f on a backend without layer shell.
    strip: Option<gtk::ApplicationWindow>,
    hidden: Cell<bool>,
    timer: RefCell<Option<gtk::glib::SourceId>>,
    delay_ms: u32,
}

type Shared = Rc<Entry>;

thread_local! {
    /// Every auto-hiding bar of the current configuration, in build order.
    static BARS: RefCell<Vec<Shared>> = const { RefCell::new(Vec::new()) };
}

/// Make WINDOW an auto-hiding bar, and give it its reveal strip.
///
/// Nothing happens on a backend without layer shell: there is no exclusive zone
/// to give back and no edge to catch the pointer on, so a bar that hid itself
/// there could not be brought back.
pub fn install(
    window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
) {
    if !bar.auto_hide || !layer_shell_available() {
        return;
    }

    let strip = build_strip(window, bar, monitor);
    let entry = Rc::new(Entry {
        window: window.clone(),
        strip,
        hidden: Cell::new(false),
        timer: RefCell::new(None),
        delay_ms: bar.auto_hide_delay_ms,
    });

    watch_window(&entry);

    if let Some(strip) = entry.strip.as_ref() {
        watch_strip(&entry, strip);
    }

    BARS.with(|bars| bars.borrow_mut().push(Rc::clone(&entry)));

    // A bar that starts on screen would keep its exclusive zone until the
    // pointer first left it; hiding it now is what makes "auto-hide" true at
    // login, when the reader wants the whole screen for a window.
    hide(&entry);
}

/// Reveal every auto-hiding bar.  `whelk --reveal-bar` and the hot edge both
/// come through here.
pub fn reveal_all() {
    for entry in entries() {
        reveal(&entry);
    }
}

/// Forget WINDOW's auto-hide state: its strip goes with it, and so does a
/// pending hide timer.  A reload builds new windows and must not leave the old
/// strip on the output.
pub fn forget(window: &gtk::ApplicationWindow) {
    BARS.with(|bars| {
        bars.borrow_mut().retain(|entry| {
            if &entry.window != window {
                return true;
            }

            cancel(entry);

            if let Some(strip) = entry.strip.as_ref() {
                strip.destroy();
            }

            false
        });
    });
}

/// True while WINDOW is an auto-hiding bar that is currently off screen.
pub fn is_hidden(window: &gtk::ApplicationWindow) -> bool {
    BARS.with(|bars| {
        bars.borrow()
            .iter()
            .any(|entry| &entry.window == window && entry.hidden.get())
    })
}

fn entries() -> Vec<Shared> {
    BARS.with(|bars| bars.borrow().clone())
}

fn watch_window(entry: &Shared) {
    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(entry);

    // The pointer is on the bar: it must not disappear from under it.
    motion.connect_enter(move |_, _, _| {
        if let Some(entry) = weak.upgrade() {
            cancel(&entry);
        }
    });

    let weak = Rc::downgrade(entry);
    motion.connect_leave(move |_| {
        if let Some(entry) = weak.upgrade() {
            schedule_hide(&entry);
        }
    });
    entry.window.add_controller(motion);
}

fn watch_strip(entry: &Shared, strip: &gtk::ApplicationWindow) {
    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(entry);

    motion.connect_enter(move |_, _, _| {
        if let Some(entry) = weak.upgrade() {
            reveal(&entry);
        }
    });
    strip.add_controller(motion);

    // A pointer that is already at the edge when the bar hides would never
    // send an enter event; a click there is the second way in.
    let click = gtk::GestureClick::new();
    let weak = Rc::downgrade(entry);

    click.connect_pressed(move |_, _, _, _| {
        if let Some(entry) = weak.upgrade() {
            reveal(&entry);
        }
    });
    strip.add_controller(click);
}

fn build_strip(
    bar_window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
) -> Option<gtk::ApplicationWindow> {
    let application = bar_window.application()?;
    // The strip is only ever a target, never content: no theme, no widget tree,
    // and an empty box so the surface has a child GTK will size.
    let strip = gtk::ApplicationWindow::builder()
        .application(&application)
        .title("whelk hot edge")
        .build();
    let filler = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    strip.add_css_class("whelk-hot-edge");
    strip.set_decorated(false);
    strip.set_resizable(false);
    strip.set_child(Some(&filler));
    strip.init_layer_shell();
    strip.set_layer(layer_for(bar.layer));
    strip.set_namespace(Some(HOT_EDGE_NAMESPACE));

    let edge = edge_for(bar.edge);

    strip.set_anchor(edge, true);

    let (width, height) = match bar.edge {
        BarEdge::Top | BarEdge::Bottom => {
            strip.set_anchor(Edge::Left, true);
            strip.set_anchor(Edge::Right, true);

            (bar_window.width().max(1), HOT_EDGE)
        }
        BarEdge::Left | BarEdge::Right => {
            strip.set_anchor(Edge::Top, true);
            strip.set_anchor(Edge::Bottom, true);

            (HOT_EDGE, bar_window.height().max(1))
        }
    };

    // Zero, not -1: the strip must not reserve the space the bar just gave
    // back.
    strip.set_exclusive_zone(0);
    strip.set_keyboard_mode(KeyboardMode::None);

    if let Some(monitor) = monitor {
        strip.set_monitor(Some(monitor));
    }

    strip.set_default_size(width, height);
    strip.set_visible(false);

    Some(strip)
}

fn hide(entry: &Shared) {
    if entry.hidden.replace(true) {
        return;
    }

    cancel(entry);
    entry.window.set_visible(false);

    if let Some(strip) = entry.strip.as_ref() {
        strip.set_visible(true);
    }
}

fn reveal(entry: &Shared) {
    cancel(entry);

    if !entry.hidden.replace(false) {
        // Already on screen: the strip may still be mapped from a hide that
        // was cancelled half way.
        if let Some(strip) = entry.strip.as_ref() {
            strip.set_visible(false);
        }

        return;
    }

    if let Some(strip) = entry.strip.as_ref() {
        strip.set_visible(false);
    }

    entry.window.set_visible(true);
}

fn schedule_hide(entry: &Shared) {
    if entry.hidden.get() || entry.delay_ms == 0 {
        hide(entry);

        return;
    }

    cancel(entry);

    let weak: Weak<Entry> = Rc::downgrade(entry);
    let delay = Duration::from_millis(u64::from(entry.delay_ms));
    let source = gtk::glib::timeout_add_local_once(delay, move || {
        let Some(entry) = weak.upgrade() else {
            return;
        };

        *entry.timer.borrow_mut() = None;

        // A menu the reader is in the middle of outlives the pointer leaving
        // the bar; wait for it rather than closing it under them.
        if barmodule::any_popover_open() {
            schedule_hide(&entry);

            return;
        }

        hide(&entry);
    });

    *entry.timer.borrow_mut() = Some(source);
}

fn cancel(entry: &Shared) {
    if let Some(source) = entry.timer.borrow_mut().take() {
        sources::remove(source);
    }
}

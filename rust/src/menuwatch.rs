// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The live menu watches, ported from `util/gtkmenu.c` and `util/dbusmenu.c`.
//!
//! A producer answers once; an application's menu keeps changing.  Both menu
//! protocols publish a signal for that -- `org.gtk.Menus.Changed` and
//! `com.canonical.dbusmenu`'s `LayoutUpdated` and `ItemsPropertiesUpdated` --
//! and the answer to either is one fresh tree in place of the one on screen.
//!
//! The tree is compared before it is adopted: an application re-publishes its
//! menu for reasons that are not the user's (a hover sends
//! `ItemsPropertiesUpdated`, a `Changed` arrives for a menu nobody is looking
//! at), and rebuilding for a tree that says the same thing takes the menu out
//! of somebody's hands to put the same rows back.
//!
//! This module is the display-free half: which signal belongs to which
//! protocol, the debounce a burst is folded into, the single follow-up read a
//! change during a read owes, and the rule that decides whether a fresh tree is
//! worth rebuilding for.  The subscriptions and the D-Bus calls live in
//! [`crate::globalmenu`].

use crate::menubar::Menu;

/// The C watchers' debounce: one re-read for a burst of changes.
pub const DEBOUNCE_MS: u64 = 120;

/// The protocol a change signal was published by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// `org.gtk.Menus`: the answer to `Changed` is one more `Start`.
    GtkMenus,
    /// `com.canonical.dbusmenu`: the answer is one more `GetLayout`.
    DBusMenu,
}

/// What to do with a change signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Arm the debounce timer.  A window that is already open is restarted.
    Arm,
    /// Fold this signal into the window that is already open.
    Coalesce,
}

/// The debounce state of one watcher.
///
/// The two protocols differ in one place: `org.gtk.Menus.Changed` carries a
/// batch of add/remove tuples, so the C watcher removes its timer and adds it
/// again (a burst is measured from its last signal), while the DBusMenu watcher
/// keeps the window it already opened (a burst is measured from its first).
#[derive(Debug)]
pub struct Refresh {
    protocol: Protocol,
    /// A debounce window is open: a read is scheduled but has not started.
    armed: bool,
    /// A read is on the bus.
    in_flight: bool,
    /// A change arrived while that read was on the bus: one more read is owed,
    /// and no second one is added however many signals follow.
    pending: bool,
}

impl Refresh {
    pub fn new(protocol: Protocol) -> Self {
        Self {
            protocol,
            armed: false,
            in_flight: false,
            pending: false,
        }
    }

    /// A change signal arrived.  [`Step::Arm`] means the caller (re)starts the
    /// debounce timer; [`Step::Coalesce`] means this signal is folded into the
    /// window already open and the caller does nothing.
    pub fn signal(&mut self) -> Step {
        match self.protocol {
            Protocol::GtkMenus => {
                self.armed = true;

                Step::Arm
            }
            Protocol::DBusMenu if self.armed => Step::Coalesce,
            Protocol::DBusMenu => {
                self.armed = true;

                Step::Arm
            }
        }
    }

    /// The debounce timer fired.  `true` means read now; `false` means a read
    /// is already on the bus and this one is owed when it finishes, so a slow
    /// application cannot stack up reads nobody will use.
    pub fn fire(&mut self) -> bool {
        self.armed = false;

        if self.in_flight {
            self.pending = true;

            return false;
        }

        self.in_flight = true;

        true
    }

    /// The read finished, whether or not it answered.  `true` means a change
    /// waited for it: read again now, without another debounce.
    pub fn finish(&mut self) -> bool {
        let owed = std::mem::take(&mut self.pending);

        self.in_flight = owed;

        owed
    }
}

/// The protocol a change signal belongs to, or `None` for a signal that says
/// nothing about the menu's rows.
pub fn protocol_for_member(interface: &str, member: &str) -> Option<Protocol> {
    match (interface, member) {
        ("org.gtk.Menus", "Changed") => Some(Protocol::GtkMenus),
        ("com.canonical.dbusmenu", "LayoutUpdated" | "ItemsPropertiesUpdated") => {
            Some(Protocol::DBusMenu)
        }
        _ => None,
    }
}

/// Whether a fresh tree replaces the one on screen.
///
/// Three rules, all of them the C watchers':
///
/// * an identical tree replaces nothing, so an open menu stays open;
/// * an empty `org.gtk.Menus` answer is an answer -- the application withdrew
///   its menus -- and the bar stops showing them; and
/// * an empty `GetLayout` is a failed read, not a menu without rows, and the
///   tree on screen is kept.
pub fn replaces(protocol: Protocol, current: &[Menu], fresh: &[Menu]) -> bool {
    if protocol == Protocol::DBusMenu && fresh.is_empty() {
        return false;
    }

    current != fresh
}

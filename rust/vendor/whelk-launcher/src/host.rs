// whelk-launcher -- a launcher overlay for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The host boundary.
//!
//! The launcher owns its card, search, ranking and actions.  A host supplies
//! the two services the launcher cannot own: clipboard history and the focused
//! application's live menus.  Both are optional; a host without them gets the
//! same named empty states WheLK shows when a service is absent.

use std::rc::Rc;

/// One clipboard entry, already reduced to what a row paints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRow {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub preview: Option<Vec<u8>>,
    pub pinned: bool,
}

/// The clipboard service's state, without leaking its implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardStatus {
    Ready,
    Paused,
    Unavailable,
}

impl ClipboardStatus {
    pub fn text(self) -> &'static str {
        match self {
            Self::Ready => "Clipboard ready",
            Self::Paused => "Clipboard paused",
            Self::Unavailable => "Clipboard unavailable",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Ready => "Copy something and it will appear here",
            Self::Paused => "Resume clipboard capture to see new entries",
            Self::Unavailable => "This host did not install a clipboard service",
        }
    }
}

/// A host's clipboard history.
pub trait ClipboardPort {
    fn rows(&self) -> Vec<ClipboardRow>;
    fn status(&self) -> ClipboardStatus;
    fn copy(&self, text: &str) -> bool;
    fn restore(&self, id: &str) -> bool;
    fn toggle_pin(&self, id: &str) -> bool;
    fn remove(&self, id: &str) -> bool;
    fn undo_remove(&self) -> bool;

    /// Subscribe to changes.  The returned value is kept alive by the launcher
    /// and dropped to unsubscribe; its concrete type is the host's business.
    fn watch(&self, callback: Rc<dyn Fn()>) -> Box<dyn std::any::Any>;
}

/// One row of the focused application's menu tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRow {
    pub id: usize,
    pub label: String,
    pub path: String,
    pub accel: Option<String>,
}

/// The focused application's searchable menus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuSnapshot {
    pub app_name: String,
    pub rows: Vec<MenuRow>,
}

/// The host services a launcher overlay needs.
pub trait LauncherHost {
    /// The host's generated skin for this launcher and window id.
    fn css(&self, style: &crate::style::LauncherStyle, widget_id: &str) -> String;

    /// Clipboard history, or `None` when the host has no service.
    fn clipboard(&self) -> Option<Rc<dyn ClipboardPort>>;

    /// The focused application's current menu snapshot.
    fn menu(&self) -> MenuSnapshot;

    /// Run the menu row by the id the snapshot assigned.
    fn activate_menu(&self, id: usize);
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The focused application's live menus, flattened for the launcher.
//!
//! The global menu already owns the focused application's menubar, whichever
//! producer answered.  This is the same tree as one row per entry, with its
//! path (`"View > Appearance > Dark"`) and its chord, kept where the launcher's
//! menu-item mode can search it: a menu bar's answer to the Help menu's search
//! field.
//!
//! Publishing owns the actions too.  The application may rebuild its menus
//! while the search is open, and a row that outlives its tree still has to
//! activate what it names, so the action and the bus context are copied rather
//! than looked up again at click time.
//!
//! The flattening is the display-free half and is tested without a bus; the
//! activation is the small half that talks to the session bus.

use std::cell::RefCell;

use gtk::gio;
use gtk::glib::prelude::*;
use gtk4 as gtk;

use crate::atspi;
use crate::command;
use crate::menubar::{Menu, MenuAction, MenuEntry};

/// Where a menu came from: the bus name and application object an
/// `org.gtk.Actions` name is resolved against when the row is chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionContext {
    pub bus_name: Option<String>,
    pub app_path: Option<String>,
}

/// One entry of the focused application's menubar, ready to search and run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchItem {
    /// `"View > Appearance > Dark"`, also searched as text.
    pub path: String,
    pub label: String,
    pub accel: Option<String>,
    pub action: MenuAction,
    pub context: ActionContext,
}

/// The current application's searchable menus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub app_name: String,
    pub items: Vec<SearchItem>,
}

thread_local! {
    /// One snapshot, like the C shell's: the launcher and the menu bar are
    /// both shell-wide.  Re-publishing replaces it whole.
    static CURRENT: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

/// Flatten MENUS the way the bar shows them.  Separators, disabled entries and
/// rows that activate nothing are left out: a search result that cannot be
/// chosen is not a result.
pub fn flatten(app_name: &str, context: &ActionContext, menus: &[Menu]) -> Snapshot {
    let mut snapshot = Snapshot {
        app_name: app_name.to_owned(),
        items: Vec::new(),
    };

    for menu in menus {
        append_entries(&mut snapshot.items, &menu.entries, &menu.label, context);
    }

    snapshot
}

fn append_entries(
    items: &mut Vec<SearchItem>,
    entries: &[MenuEntry],
    parent_path: &str,
    context: &ActionContext,
) {
    for entry in entries {
        let MenuEntry::Item {
            label,
            enabled,
            accel,
            action,
            submenu,
        } = entry
        else {
            continue;
        };

        if !*enabled || label.is_empty() {
            continue;
        }

        if !submenu.is_empty() {
            let path = format!("{parent_path} > {label}");

            append_entries(items, submenu, &path, context);

            continue;
        }

        let Some(action) = action else {
            continue;
        };

        items.push(SearchItem {
            path: format!("{parent_path} > {label}"),
            label: label.clone(),
            accel: accel.clone().filter(|accel| !accel.is_empty()),
            action: action.clone(),
            context: context.clone(),
        });
    }
}

/// Replace the snapshot with the application's current menus.  An empty list
/// is an answer -- the application withdrew its menus -- and clears the rows.
pub fn publish(snapshot: Snapshot) {
    CURRENT.with(|current| *current.borrow_mut() = snapshot);
}

pub fn clear() {
    CURRENT.with(|current| *current.borrow_mut() = Snapshot::default());
}

pub fn snapshot() -> Snapshot {
    CURRENT.with(|current| current.borrow().clone())
}

/// Run the entry the way clicking it in the bar does.  The context captured at
/// publish time is what makes a row still work after the focused window moved
/// on.
pub fn activate(item: &SearchItem) {
    match &item.action {
        MenuAction::Command(command_line) => command::spawn(command_line),
        MenuAction::DBusMenu { service, path, id } => {
            let Some(bus) = session_bus() else {
                return;
            };
            let parameters = (*id, "clicked", 0i32.to_variant(), 0u32).to_variant();

            bus.call(
                Some(service),
                path,
                "com.canonical.dbusmenu",
                "Event",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                None::<&gio::Cancellable>,
                |_| {},
            );
        }
        MenuAction::GtkAction {
            service,
            path,
            name,
            target,
        } => activate_gtk_action(service, path, name, target.as_deref()),
        MenuAction::GtkActionName(action) => {
            let Some(service) = item.context.bus_name.as_deref() else {
                return;
            };
            let Some(app_path) = item.context.app_path.as_deref() else {
                return;
            };
            let (prefix, name) = action
                .split_once('.')
                .filter(|(_, name)| !name.is_empty())
                .unwrap_or((action, action));
            let path = if prefix == "win" {
                format!("{app_path}/windows/0")
            } else {
                app_path.to_owned()
            };

            activate_gtk_action(service, &path, name, None);
        }
        MenuAction::DesktopAction { command, .. } => command::spawn(command),
        MenuAction::Atspi {
            service,
            path,
            action,
        } => atspi::activate(service, path, *action),
    }
}

fn session_bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()
}

fn activate_gtk_action(service: &str, path: &str, name: &str, target: Option<&str>) {
    let Some(bus) = session_bus() else {
        return;
    };
    let targets = target
        .map(|target| vec![target.to_variant()])
        .unwrap_or_default();
    let parameters = (
        name,
        targets,
        std::collections::HashMap::<String, gtk::glib::Variant>::new(),
    )
        .to_variant();

    bus.call(
        Some(service),
        path,
        "org.gtk.Actions",
        "Activate",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |_| {},
    );
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! What the bar says to assistive technology, and how a keyboard reaches it.
//!
//! The bar is a row of gestures, not of buttons: a module root is a box with a
//! click controller, so without help a screen reader sees an anonymous group
//! and a keyboard can only reach whatever the compositor focuses.  Two things
//! fix that here.
//!
//! - Every module root wears a role and a name.  The name is the reading as it
//!   is drawn -- "23:41", "Now playing: Nightcall", a task button's title -- so
//!   what a screen reader announces is what the reader would see.
//! - A bar configured `#:keyboard? #t` asks the compositor for on-demand
//!   keyboard focus and becomes traversable: Tab and the arrows move between
//!   modules, Home and End jump to the ends, and Return or Space runs the
//!   focused module's click command.
//!
//! The pure half -- [`module_name`], [`module_role`] -- is what the tests pin,
//! because the alternative is a screen reader run on every commit.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::barmodule;

/// The human name of a module kind, for a reading that has nothing else to say.
///
/// A reading with no label and no tooltip still has to be announced: "battery",
/// not `battery`.  An unknown kind -- a configuration from a newer build -- is
/// its own name with the hyphens opened up.
pub fn module_name(kind: &str) -> String {
    let name = match kind {
        "audio" => "volume",
        "battery" => "battery",
        "bluetooth" => "bluetooth",
        "clock" => "clock",
        "backlight" => "brightness",
        "clipboard" => "clipboard history",
        "cpu" => "CPU usage",
        "custom" => "command reading",
        "dnd" => "do not disturb",
        "inhibit" => "sleep inhibitor",
        "language" => "keyboard layout",
        "keyboard" => "caps lock",
        "memory" => "memory usage",
        "menu" => "application menus",
        "mpris" => "now playing",
        "notifications" => "notifications",
        "power-profile" => "power profile",
        "scheme" => "Scheme reading",
        "start" => "the shell menu",
        "tasks" => "open windows",
        "tray" => "status icons",
        "wallpaper" => "wallpaper",
        "wifi" => "Wi-Fi",
        "window" => "focused window",
        "workspaces" => "workspaces",
        other => return other.replace('-', " "),
    };

    name.to_owned()
}

/// What a screen reader should say for a reading.
///
/// The drawn text wins, then the tooltip's first line, then the kind's name: a
/// reading that is a number is announced as the number, and a reading that is
/// an icon is announced as what the icon means.
pub fn module_label(kind: &str, text: &str, tooltip: Option<&str>) -> String {
    let text = text.trim();

    if !text.is_empty() {
        return text.to_owned();
    }

    if let Some(line) = tooltip
        .map(|tooltip| tooltip.lines().next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
    {
        return line.to_owned();
    }

    module_name(kind)
}

/// The role a module root wears.
///
/// A reading with a click is a button: that is what a screen reader offers to
/// activate, and what the keyboard path activates.  A reading without one is a
/// status label.
pub fn module_role(clickable: bool) -> gtk::AccessibleRole {
    if clickable {
        gtk::AccessibleRole::Button
    } else {
        gtk::AccessibleRole::Label
    }
}

/// Give `widget` the name and role its module should wear.
pub fn describe(
    widget: &impl IsA<gtk::Accessible>,
    kind: &str,
    text: &str,
    tooltip: Option<&str>,
    clickable: bool,
) {
    let label = module_label(kind, text, tooltip);

    widget.set_accessible_role(module_role(clickable));
    widget.update_property(&[
        gtk::accessible::Property::Label(&label),
        gtk::accessible::Property::Description(&module_name(kind)),
    ]);
}

/// Make the bar's modules reachable from the keyboard.
///
/// Every module of every row becomes focusable, and one key controller on the
/// bar walks the rows in reading order: Tab and the arrows move one module,
/// Home and End jump to the ends, and Return or Space runs the focused module's
/// click command.  The bar's own layer surface asks for on-demand keyboard
/// focus, so this only takes the keyboard when a reader clicks or focusses it.
pub fn install_keyboard_navigation(window: &gtk::ApplicationWindow, rows: &[gtk::Box]) {
    let modules = module_widgets(rows);

    if modules.is_empty() {
        return;
    }

    for widget in &modules {
        widget.set_focusable(true);
    }

    let controller = gtk::EventControllerKey::new();
    let order = modules;

    controller.connect_key_pressed(move |_, key, _, state| {
        let forward = !state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let count = order.len();

        let index = order.iter().position(|widget| widget.has_focus());

        match key {
            gtk::gdk::Key::Tab | gtk::gdk::Key::ISO_Left_Tab => {
                let next = match index {
                    Some(index) if forward => (index + 1) % count,
                    Some(index) => (index + count - 1) % count,
                    None => 0,
                };

                order[next].grab_focus();
            }
            gtk::gdk::Key::Right
            | gtk::gdk::Key::Down
            | gtk::gdk::Key::Left
            | gtk::gdk::Key::Up => {
                let step = matches!(key, gtk::gdk::Key::Right | gtk::gdk::Key::Down);
                let next = match index {
                    Some(index) if step => (index + 1).min(count - 1),
                    Some(index) => index.saturating_sub(1),
                    None => 0,
                };

                order[next].grab_focus();
            }
            gtk::gdk::Key::Home => {
                order[0].grab_focus();
            }
            gtk::gdk::Key::End => {
                order[count - 1].grab_focus();
            }
            gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::space => {
                let Some(widget) = index.map(|index| &order[index]) else {
                    return gtk::glib::Propagation::Proceed;
                };

                if !barmodule::activate(widget) {
                    return gtk::glib::Propagation::Proceed;
                }
            }
            _ => return gtk::glib::Propagation::Proceed,
        }

        gtk::glib::Propagation::Stop
    });

    window.add_controller(controller);
}

/// The module roots of the rows, in reading order.  A row's own children are
/// the readings; the fold's chevron is not one of them.
fn module_widgets(rows: &[gtk::Box]) -> Vec<gtk::Widget> {
    let mut modules = Vec::new();

    for row in rows {
        let mut child = row.first_child();

        while let Some(widget) = child {
            if widget.has_css_class("whelk-module") {
                modules.push(widget.clone());
            }

            child = widget.next_sibling();
        }
    }

    modules
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The clipboard bar module, ported from `modules/clipboard.c`.
//!
//! The count is a signal from the shell's own history rather than a poll of
//! `wl-paste` or `clipman`.  A click opens the Rust picker; the right click
//! owns the history controls, including the two clear operations and restart.

use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::clipboard::{Clipboard, Status};
use crate::launcherpanel::Launcher;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 0;
const DEFAULT_ICON: &str = "\u{f328}";
const DEFAULT_TOOLTIP: &str =
    "Clipboard: {count} item(s) — click for the picker\nRight-click: history controls";

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();

    let Some(clipboard) = Clipboard::current() else {
        host.set_visible(false);

        return Some(root);
    };

    paint(&module, &host, &clipboard);

    let weak = host.downgrade();
    let callback_module = module.clone();
    let subscription = clipboard.on_changed(move || {
        let Some(host) = weak.upgrade() else {
            return;
        };
        let Some(clipboard) = Clipboard::current() else {
            return;
        };

        paint(&callback_module, &host, &clipboard);
    });
    let subscription = std::cell::RefCell::new(Some(subscription));

    root.connect_destroy(move |_| drop(subscription.borrow_mut().take()));

    if host.settings().actions.primary.is_none() {
        attach_picker(&root);
    }

    if host.settings().actions.secondary.is_none() {
        attach_menu(&root, &clipboard);
    }

    Some(root)
}

fn paint(module: &ModuleSpec, host: &ModuleHost, clipboard: &Clipboard) {
    let count = clipboard.count();
    let status = clipboard.status();
    let count_text = count.to_string();
    let values = [("count", count_text.as_str())];

    host.set_visible(true);
    host.remove_class("empty");
    host.remove_class("paused");
    host.remove_class("unavailable");

    if count == 0 {
        host.add_class("empty");
    }

    match status {
        Status::Paused => host.add_class("paused"),
        Status::Unavailable => host.add_class("unavailable"),
        Status::Ready => {}
    }

    let format = module.text("format").unwrap_or_else(|| "{icon}".to_owned());
    let icon = module
        .text("icon")
        .unwrap_or_else(|| DEFAULT_ICON.to_owned());
    let tooltip = format!(
        "{}\n{}",
        module
            .text("tooltip")
            .unwrap_or_else(|| DEFAULT_TOOLTIP.to_owned()),
        status.text()
    );

    host.present_with(&format, &icon, Some(&tooltip), &values);
}

fn attach_picker(root: &gtk::Box) {
    let gesture = gtk::GestureClick::new();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(|_, _, _, _| Launcher::show_current(Some("clipboard")));
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

fn attach_menu(root: &gtk::Box, clipboard: &Clipboard) {
    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-menu");
    popover.set_child(Some(&menu));
    popover.set_parent(root);

    let popover_weak = popover.downgrade();

    root.connect_destroy(move |_| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.unparent();
        }
    });

    let gesture = gtk::GestureClick::new();
    let clipboard = clipboard.clone();
    let root_weak = root.downgrade();
    let popover_clone = popover.clone();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);

        if root_weak.upgrade().is_some() {
            fill_menu(&menu, &popover_clone, &clipboard);
            popover_clone.popup();
        }
    });
    root.add_controller(gesture);
}

fn fill_menu(menu: &gtk::Box, popover: &gtk::Popover, clipboard: &Clipboard) {
    while let Some(child) = menu.first_child() {
        menu.remove(&child);
    }

    let paused = clipboard.is_paused();
    let pause = menu_button(if paused {
        "Resume history"
    } else {
        "Pause history"
    });
    let pause_clipboard = clipboard.clone();
    let pause_popover = popover.downgrade();

    pause.connect_clicked(move |_| {
        pause_clipboard.set_paused(!pause_clipboard.is_paused());

        if let Some(popover) = pause_popover.upgrade() {
            popover.popdown();
        }
    });
    menu.append(&pause);

    let unpinned = menu_button("Clear unpinned");
    let unpinned_clipboard = clipboard.clone();
    let unpinned_popover = popover.downgrade();

    unpinned.connect_clicked(move |_| {
        unpinned_clipboard.clear_unpinned();

        if let Some(popover) = unpinned_popover.upgrade() {
            popover.popdown();
        }
    });
    menu.append(&unpinned);

    let all = menu_button("Clear all…");
    let all_clipboard = clipboard.clone();
    let all_popover = popover.downgrade();

    all.connect_clicked(move |_| {
        let dialog = gtk::AlertDialog::builder()
            .message("Clear the whole clipboard history?")
            .detail("Pinned copies are deleted too. This cannot be undone.")
            .modal(true)
            .build();
        let clear = all_clipboard.clone();

        dialog.set_buttons(&["Cancel", "Clear all"]);
        dialog.set_cancel_button(0);
        dialog.set_default_button(0);

        if let Some(popover) = all_popover.upgrade() {
            popover.popdown();
        }

        dialog.choose(
            None::<&gtk::Window>,
            None::<&gio::Cancellable>,
            move |result| {
                if matches!(result, Ok(1)) {
                    clear.clear();
                }
            },
        );
    });
    menu.append(&all);
    menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let restart = menu_button("Restart listener");
    let restart_clipboard = clipboard.clone();
    let restart_popover = popover.downgrade();

    restart.set_sensitive(!paused);
    restart.connect_clicked(move |_| {
        restart_clipboard.restart();

        if let Some(popover) = restart_popover.upgrade() {
            popover.popdown();
        }
    });
    menu.append(&restart);

    let open = menu_button("Open picker");
    let open_popover = popover.downgrade();

    open.connect_clicked(move |_| {
        Launcher::show_current(Some("clipboard"));

        if let Some(popover) = open_popover.upgrade() {
            popover.popdown();
        }
    });
    menu.append(&open);
}

fn menu_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);

    button.add_css_class("flat");
    button.add_css_class("whelk-menu-item");
    button.set_halign(gtk::Align::Fill);

    button
}

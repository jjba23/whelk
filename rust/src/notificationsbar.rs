// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The notification-count bar module, ported from `modules/notifications.c`.
//!
//! The service is the reading: it owns the count and the DND flag, and the
//! module only formats them.  A configured `on-click` still wins; a bare left
//! click toggles the notification centre, and a bare right click toggles DND.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::notificationcenter::NotificationCenter;
use crate::notifications::NotificationService;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 0;
const DEFAULT_ICON: &str = "\u{f1f6}";
const DEFAULT_TOOLTIP: &str =
    "{count} notification(s) \u{2014} click for the centre \u{00b7} right-click for do not disturb";

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();

    let Some(service) = NotificationService::current() else {
        host.set_visible(false);

        return Some(root);
    };

    paint(&module, &host, &service);

    let weak = host.downgrade();
    let callback_module = module.clone();

    let subscription = service.on_changed(move || {
        let Some(host) = weak.upgrade() else {
            return;
        };
        let Some(service) = NotificationService::current() else {
            return;
        };

        paint(&callback_module, &host, &service);
    });

    // The service outlives a rebuild; the root's destroy handler is what
    // removes this listener.  Without the guard, every reload would append a
    // callback that can only fail its weak upgrade.
    let subscription = std::cell::RefCell::new(Some(subscription));

    root.connect_destroy(move |_| drop(subscription.borrow_mut().take()));

    if host.settings().actions.secondary.is_none() {
        attach_toggle(&root, gtk::gdk::BUTTON_SECONDARY, &service);
    }

    if host.settings().actions.primary.is_none() {
        attach_center(&root);
    }

    Some(root)
}

fn paint(module: &ModuleSpec, host: &ModuleHost, service: &NotificationService) {
    let count = service.count();
    let dnd = service.has_dnd();
    let count_text = count.to_string();
    let state = if dnd {
        "dnd"
    } else if count > 0 {
        "new"
    } else {
        "idle"
    };
    let values = [("count", count_text.as_str()), ("state", state)];

    host.remove_class("empty");
    host.remove_class("dnd");

    if dnd {
        host.add_class("dnd");
    } else if count == 0 {
        host.add_class("empty");
    }

    let format = module.text("format").unwrap_or_else(|| "{icon}".to_owned());
    let format = if dnd {
        module.text("dnd-format").unwrap_or(format)
    } else {
        format
    };
    let icon = if dnd {
        module
            .text("dnd-icon")
            .unwrap_or_else(|| DEFAULT_ICON.to_owned())
    } else {
        host.settings().icon.clone().unwrap_or_default()
    };
    let tooltip = if dnd {
        module.text("dnd-tooltip").unwrap_or_else(|| {
            "Do not disturb is on \u{2014} right-click to let notifications through".to_owned()
        })
    } else {
        host.settings()
            .tooltip
            .clone()
            .unwrap_or_else(|| DEFAULT_TOOLTIP.to_owned())
    };

    host.present_with(&format, &icon, Some(&tooltip), &values);
}

fn attach_toggle(root: &gtk::Box, button: u32, service: &NotificationService) {
    let gesture = gtk::GestureClick::new();
    let service = service.clone();

    gesture.set_button(button);
    gesture.connect_pressed(move |_, _, _, _| service.set_dnd(!service.has_dnd()));
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

fn attach_center(root: &gtk::Box) {
    let gesture = gtk::GestureClick::new();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(|_, _, _, _| NotificationCenter::toggle_current());
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

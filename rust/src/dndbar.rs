// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The do-not-disturb bar module, ported from `modules/dnd.c`.
//!
//! DND belongs to the notification service, which delegates it to the logind
//! idle manager when one exists.  This module is only a view and a toggle; the
//! C module's optional hidden-when-off state is preserved with `show-off?`.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::notifications::NotificationService;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 0;
const DND_ICON: &str = "\u{f1f6}";
const OFF_ICON: &str = "\u{f0f3}";

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

    let show_off = module.boolean("show-off?").unwrap_or(false);

    paint(&module, &host, &service, show_off);

    let weak = host.downgrade();
    let callback_module = module.clone();

    let subscription = service.on_changed(move || {
        let Some(host) = weak.upgrade() else {
            return;
        };
        let Some(service) = NotificationService::current() else {
            return;
        };

        paint(&callback_module, &host, &service, show_off);
    });

    let subscription = std::cell::RefCell::new(Some(subscription));

    root.connect_destroy(move |_| drop(subscription.borrow_mut().take()));

    if host.settings().actions.primary.is_none() {
        attach_toggle(&root, &service);
    }

    Some(root)
}

fn paint(module: &ModuleSpec, host: &ModuleHost, service: &NotificationService, show_off: bool) {
    let dnd = service.has_dnd();

    host.remove_class("on");
    host.remove_class("off");
    host.remove_class("whelk-accent");

    if !dnd && !show_off {
        host.set_visible(false);

        return;
    }

    host.set_visible(true);

    let state = if dnd { "on" } else { "off" };
    let values = [("state", state)];

    if dnd {
        host.add_class("on");
        host.add_class("whelk-accent");
    } else {
        host.add_class("off");
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
            .unwrap_or_else(|| DND_ICON.to_owned())
    } else {
        module.text("icon").unwrap_or_else(|| OFF_ICON.to_owned())
    };
    let tooltip = if dnd {
        module.text("dnd-tooltip").unwrap_or_else(|| {
            host.settings().tooltip.clone().unwrap_or_else(|| {
                "Do not disturb is on \u{2014} click to let notifications through".to_owned()
            })
        })
    } else {
        host.settings().tooltip.clone().unwrap_or_else(|| {
            "Do not disturb is off \u{2014} click to silence notifications".to_owned()
        })
    };

    host.present_with(&format, &icon, Some(&tooltip), &values);
}

fn attach_toggle(root: &gtk::Box, service: &NotificationService) {
    let gesture = gtk::GestureClick::new();
    let service = service.clone();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |_, _, _, _| service.set_dnd(!service.has_dnd()));
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

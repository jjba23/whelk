// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `wallpaper` bar module.
//!
//! One reading and one gesture: a click opens [`crate::wallpapermenu`] beside
//! the reading, where the pictures are one click away from being applied.  The
//! menu is the shell's own, so the reading is not a link to another program.
//!
//! A configuration can still say what the buttons do: `#:on-click` and
//! `#:on-click-right` are the module host's, and a reading that names them uses
//! them instead of the menu.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{Edge, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::wallpapermenu;

/// The picture-frame glyph, for a bar whose icon font carries it.
const DEFAULT_ICON: &str = "\u{f03e}";
const DEFAULT_TOOLTIP: &str =
    "Wallpaper: pick one\nClick: the wallpapers · Right-click: the full picker";

/// A reading that renders once: the collection belongs to the menu, not the bar.
pub const DEFAULT_INTERVAL_SECONDS: u32 = 0;

/// Where the menu hangs from a reading.
enum Anchor {
    /// Below (or, for a bar at the bottom, above) the reading.
    Under {
        monitor: gtk::gdk::Monitor,
        center_x: i32,
        edge_y: i32,
        above: bool,
    },
    /// Beside a vertical bar, level with the reading.
    Near {
        monitor: gtk::gdk::Monitor,
        point: (i32, i32),
    },
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();

    let icon = module
        .text("icon")
        .filter(|icon| !icon.is_empty())
        .unwrap_or_else(|| DEFAULT_ICON.to_owned());
    let format = module
        .text("format")
        .filter(|format| !format.is_empty())
        .unwrap_or_else(|| "{icon}".to_owned());
    let tooltip = module
        .text("tooltip")
        .filter(|tooltip| !tooltip.is_empty())
        .unwrap_or_else(|| DEFAULT_TOOLTIP.to_owned());

    host.present_with(&format, &icon, Some(&tooltip), &[]);

    if host.settings().actions.primary.is_none() {
        open_menu(&root, context);
    }

    if host.settings().actions.secondary.is_none() {
        browse(&root);
    }

    Some(root)
}

/// A click on the reading opens the menu beside it, on the bar's own monitor.
fn open_menu(root: &gtk::Box, context: &ModuleContext) {
    let gesture = gtk::GestureClick::new();
    let widget = root.to_owned();
    let handler_widget = widget.clone();
    let edge = context.bar.edge;
    let margins = (
        context.bar.margin_left,
        context.bar.margin_top,
        context.bar.margin_right,
        context.bar.margin_bottom,
    );
    let thickness = context.bar.height;

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        let Some(anchor) = anchor(&handler_widget, edge, margins, thickness) else {
            return;
        };
        let Some(menu) = wallpapermenu::current() else {
            return;
        };

        gesture.set_state(gtk::EventSequenceState::Claimed);

        match anchor {
            Anchor::Under {
                monitor,
                center_x,
                edge_y,
                above,
            } => menu.show_under(&monitor, center_x, edge_y, above),
            Anchor::Near { monitor, point } => menu.show_at_point(&monitor, point),
        }
    });
    widget.add_controller(gesture);
    widget.set_cursor_from_name(Some("pointer"));
}

/// A configuration without a right click still gets the full picker there: the
/// menu is four by three, and a collection can be much bigger.
fn browse(root: &gtk::Box) {
    let gesture = gtk::GestureClick::new();
    let widget = root.to_owned();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        let Some(menu) = wallpapermenu::current() else {
            return;
        };

        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu.browse_all();
    });
    widget.add_controller(gesture);
    widget.set_cursor_from_name(Some("pointer"));
}

/// The monitor the reading is on, and where the menu hangs from.
fn anchor(
    root: &gtk::Box,
    edge: Edge,
    margins: (i32, i32, i32, i32),
    thickness: i32,
) -> Option<Anchor> {
    let window = root.root()?.downcast::<gtk::Window>().ok()?;
    let surface = window.surface()?;
    let display = gtk::gdk::Display::default()?;
    let monitor = display.monitor_at_surface(&surface)?;
    let geometry = monitor.geometry();
    let bounds = root.compute_bounds(&window)?;
    let (margin_left, margin_top, margin_right, margin_bottom) = margins;
    let bar = thickness.max(window.height());

    Some(match edge {
        Edge::Top => Anchor::Under {
            monitor,
            center_x: margin_left + bounds.x() as i32 + bounds.width() as i32 / 2,
            edge_y: margin_top + bar,
            above: false,
        },
        Edge::Bottom => Anchor::Under {
            monitor,
            center_x: margin_left + bounds.x() as i32 + bounds.width() as i32 / 2,
            edge_y: geometry.height() - margin_bottom - bar,
            above: true,
        },
        // A vertical bar has no "under": the menu opens beside it, level with
        // the reading's middle.
        Edge::Left => Anchor::Near {
            monitor,
            point: (
                margin_left + bar,
                margin_top + bounds.y() as i32 + bounds.height() as i32 / 2,
            ),
        },
        Edge::Right => Anchor::Near {
            monitor,
            point: (
                geometry.width() - margin_right - bar,
                margin_top + bounds.y() as i32 + bounds.height() as i32 / 2,
            ),
        },
    })
}

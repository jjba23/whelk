// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The background's input half.
//!
//! whelk draws its wallpaper on the background layer, and a layer surface there
//! only receives the clicks nothing above it wants: the empty desktop, which is
//! exactly the place a right click should offer the wallpapers.  This surface is
//! transparent and always present -- a background the engine is not drawing
//! still belongs to whelk, and a right click on it should still work -- and the
//! only things it does are the two gestures in [`present`].
//!
//! It is the *background* layer, so it can never take a click from a window, and
//! it reserves no space.  On a server without wlr-layer-shell there is no
//! background to click on, so nothing is built there.

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::wallpapermenu;

/// One surface per monitor, in the display's order.  The caller keeps them; a
/// reload or a hotplug presents a new set and this one goes away with it.
pub fn present(application: &gtk::Application, monitors: &[gtk::gdk::Monitor]) -> Vec<gtk::Window> {
    if !crate::ui::layer_shell_available() {
        return Vec::new();
    }

    let mut surfaces = Vec::new();

    if monitors.is_empty() {
        if let Some(window) = surface(application, None) {
            surfaces.push(window);
        }

        return surfaces;
    }

    for monitor in monitors {
        if let Some(window) = surface(application, Some(monitor)) {
            surfaces.push(window);
        }
    }

    surfaces
}

fn surface(
    application: &gtk::Application,
    monitor: Option<&gtk::gdk::Monitor>,
) -> Option<gtk::Window> {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk desktop")
        .build();
    // The same skin every surface wears, which is transparent: the wallpaper is
    // its own surface underneath.
    let area = gtk::Box::new(gtk::Orientation::Vertical, 0);

    window.add_css_class("whelk-surface");
    window.set_decorated(false);
    window.set_resizable(true);
    area.set_hexpand(true);
    area.set_vexpand(true);
    window.set_child(Some(&area));

    window.init_layer_shell();
    window.set_namespace(Some("whelk-rust-desktop"));
    window.set_layer(Layer::Background);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::None);

    if let Some(monitor) = monitor {
        window.set_monitor(Some(monitor));
    }

    let right = gtk::GestureClick::new();
    let menu_monitor = monitor.cloned();

    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.connect_pressed(move |gesture, _, x, y| {
        let Some(monitor) = menu_monitor.clone() else {
            return;
        };
        let Some(menu) = wallpapermenu::current() else {
            return;
        };

        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu.show_at_point(&monitor, (x as i32, y as i32));
    });
    area.add_controller(right);

    // Any other click on the desktop is a click *away* from the menu, and the
    // menu is not a popover that could dismiss itself: the surface is the card.
    let away = gtk::GestureClick::new();

    away.set_button(gtk::gdk::BUTTON_PRIMARY);
    away.connect_pressed(|gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Denied);
        wallpapermenu::hide_current();
    });
    area.add_controller(away);

    window.present();

    Some(window.upcast())
}

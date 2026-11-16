// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The order of the menu probes, ported from `util/menuprobe.c`.
//!
//! The D-Bus side of the global menu is a fallback chain, and the order is the
//! contract: the object paths a GTK window published are tried before the
//! conventional guesses, the X11 registrar is asked only when a window id can
//! answer it, and DBusMenu comes after `org.gtk.Menus`.

use crate::session::FocusedPaths;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuProbeOrder {
    /// `org.gtk.Menus` object paths: exact X11 paths first, then the
    /// conventional paths derived from the application object.
    pub gtk_paths: Vec<String>,
    /// `com.canonical.dbusmenu` paths, with the registrar's exact path inserted
    /// at the front when the registrar answers.
    pub dbus_paths: Vec<String>,
    /// Whether the focused window has an X11 id and may be resolved through
    /// `com.canonical.AppMenu.Registrar`.
    pub ask_registrar: bool,
}

/// Builds the ordered path lists.  A bus name that cannot exist gets no
/// `org.gtk.Menus` attempts at all; the registrar can still rescue the window.
pub fn order_new(
    paths: Option<&FocusedPaths>,
    bus_name_is_valid: bool,
    app_path: Option<&str>,
) -> MenuProbeOrder {
    let mut order = MenuProbeOrder::default();

    if bus_name_is_valid {
        // The exact answers first.  They are absent on Wayland, and on X11 they
        // are what makes Firefox and LibreOffice work despite their WM_CLASS.
        if let Some(paths) = paths {
            add_path(&mut order.gtk_paths, paths.menubar_path.as_deref());

            if let Some(window_path) = paths.window_path.as_deref().filter(|path| !path.is_empty())
            {
                order.gtk_paths.push(format!("{window_path}/menus/menubar"));
            }

            add_path(&mut order.gtk_paths, paths.appmenu_path.as_deref());
        }

        // Then the protocol's conventional locations, application-wide and
        // window-scoped, and GTK's anonymous application.
        if let Some(app_path) = app_path.filter(|path| !path.is_empty()) {
            order.gtk_paths.push(format!("{app_path}/menus/menubar"));
            order.gtk_paths.push(format!("{app_path}/menus/AppMenu"));
            order
                .gtk_paths
                .push(format!("{app_path}/window/0/menus/menubar"));
        }

        order
            .gtk_paths
            .push("/org/gtk/Application/anonymous/menus/menubar".to_owned());
    }

    order.dbus_paths.push("/MenuBar/1".to_owned());
    order.dbus_paths.push("/MenuBar/0".to_owned());
    order.dbus_paths.push("/com/canonical/dbusmenu".to_owned());
    order.ask_registrar = paths.is_some_and(|paths| paths.window_id != 0);

    order
}

/// Puts the registrar's answer at the front of the DBusMenu attempt list,
/// without duplicating a path that was already conventional.
pub fn use_registrar_path(dbus_paths: &mut Vec<String>, path: &str) {
    if path.is_empty() {
        return;
    }

    dbus_paths.retain(|candidate| candidate != path);
    dbus_paths.insert(0, path.to_owned());
}

fn add_path(paths: &mut Vec<String>, path: Option<&str>) {
    if let Some(path) = path.filter(|path| !path.is_empty()) {
        paths.push(path.to_owned());
    }
}

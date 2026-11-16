// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The X11 registrar is a fallback, not the first answer.  These tests pin the
// order: the exact _GTK_* menubar path first, then the window and app-menu
// paths, then the conventional GTK paths, with the registrar available after
// them and DBusMenu after org.gtk.Menus.

use whelk_rs::menuprobe;
use whelk_rs::session::FocusedPaths;

fn x11_paths() -> FocusedPaths {
    FocusedPaths {
        window_id: 0x3a00007,
        bus_name: Some("org.example.App".to_owned()),
        app_path: Some("/org/example/App".to_owned()),
        window_path: Some("/org/example/App/window/1".to_owned()),
        menubar_path: Some("/org/example/App/menus/MenuBar".to_owned()),
        appmenu_path: Some("/org/example/App/menus/AppMenu".to_owned()),
    }
}

#[test]
fn x11_registrar_comes_after_exact_gtk_paths() {
    let paths = x11_paths();
    let mut order = menuprobe::order_new(Some(&paths), true, Some("/org/example/App"));

    assert_eq!(
        order.gtk_paths,
        vec![
            "/org/example/App/menus/MenuBar",
            "/org/example/App/window/1/menus/menubar",
            "/org/example/App/menus/AppMenu",
            "/org/example/App/menus/menubar",
            "/org/example/App/menus/AppMenu",
            "/org/example/App/window/0/menus/menubar",
            "/org/gtk/Application/anonymous/menus/menubar",
        ]
    );
    assert!(order.ask_registrar);
    assert_eq!(
        order.dbus_paths,
        vec!["/MenuBar/1", "/MenuBar/0", "/com/canonical/dbusmenu"]
    );

    // When the registrar answers, its exact path is what DBusMenu tries first;
    // the conventional paths remain behind it as a fallback.
    menuprobe::use_registrar_path(&mut order.dbus_paths, "/org/example/App/MenuBar");
    assert_eq!(
        order.dbus_paths,
        vec![
            "/org/example/App/MenuBar",
            "/MenuBar/1",
            "/MenuBar/0",
            "/com/canonical/dbusmenu",
        ]
    );

    // A registrar answer that was already conventional is moved, not doubled.
    menuprobe::use_registrar_path(&mut order.dbus_paths, "/MenuBar/1");
    assert_eq!(
        order.dbus_paths,
        vec![
            "/MenuBar/1",
            "/org/example/App/MenuBar",
            "/MenuBar/0",
            "/com/canonical/dbusmenu",
        ]
    );
}

#[test]
fn registrar_needs_an_x11_window() {
    let mut paths = x11_paths();

    paths.window_id = 0;

    let order = menuprobe::order_new(Some(&paths), true, Some("/org/example/App"));

    assert!(!order.ask_registrar);
    assert_eq!(order.gtk_paths.len(), 7);
}

#[test]
fn wayland_uses_conventional_paths_only() {
    let paths = FocusedPaths::default();
    let order = menuprobe::order_new(Some(&paths), true, Some("/org/example/App"));

    assert!(!order.ask_registrar);
    assert_eq!(
        order.gtk_paths,
        vec![
            "/org/example/App/menus/menubar",
            "/org/example/App/menus/AppMenu",
            "/org/example/App/window/0/menus/menubar",
            "/org/gtk/Application/anonymous/menus/menubar",
        ]
    );
}

#[test]
fn registrar_can_rescue_an_invalid_bus_name() {
    let paths = x11_paths();
    let order = menuprobe::order_new(Some(&paths), false, Some("/org/example/App"));

    // No org.gtk.Menus call may be addressed to a WM_CLASS.  The registrar is
    // still worth asking because it takes a window id, not a bus name.
    assert!(order.gtk_paths.is_empty());
    assert!(order.ask_registrar);
    assert_eq!(order.dbus_paths.len(), 3);
}

#[test]
fn duplicate_and_empty_registrar_paths_are_safe() {
    let mut paths = vec!["/MenuBar/1".to_owned()];

    menuprobe::use_registrar_path(&mut paths, "");
    assert_eq!(paths, vec!["/MenuBar/1"]);
}

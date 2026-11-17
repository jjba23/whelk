// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::menubar::{Menu, MenuEntry};
use whelk_rs::menuwatch::{self, Protocol, Refresh, Step};

fn file_menu(quit_label: &str) -> Vec<Menu> {
    vec![Menu::with_entries(
        "File",
        vec![MenuEntry::command(quit_label, "whelk --quit")],
    )]
}

#[test]
fn the_two_protocols_are_classified_by_interface_and_member() {
    assert_eq!(
        menuwatch::protocol_for_member("org.gtk.Menus", "Changed"),
        Some(Protocol::GtkMenus)
    );
    assert_eq!(
        menuwatch::protocol_for_member("com.canonical.dbusmenu", "LayoutUpdated"),
        Some(Protocol::DBusMenu)
    );
    assert_eq!(
        menuwatch::protocol_for_member("com.canonical.dbusmenu", "ItemsPropertiesUpdated"),
        Some(Protocol::DBusMenu)
    );
}

#[test]
fn other_signals_are_not_menu_changes() {
    assert_eq!(
        menuwatch::protocol_for_member("org.gtk.Menus", "Start"),
        None
    );
    assert_eq!(
        menuwatch::protocol_for_member("com.canonical.dbusmenu", "AboutToShow"),
        None
    );
    assert_eq!(
        menuwatch::protocol_for_member("org.freedesktop.DBus.Properties", "PropertiesChanged"),
        None
    );
    assert_eq!(
        menuwatch::protocol_for_member("org.a11y.atspi.Event.Object", "ChildrenChanged"),
        None
    );
}

#[test]
fn a_gtk_burst_restarts_its_one_window() {
    let mut refresh = Refresh::new(Protocol::GtkMenus);

    assert_eq!(refresh.signal(), Step::Arm);
    assert_eq!(
        refresh.signal(),
        Step::Arm,
        "Changed removes and re-adds the timer"
    );
}

#[test]
fn a_dbusmenu_burst_joins_the_window_already_open() {
    let mut refresh = Refresh::new(Protocol::DBusMenu);

    assert_eq!(refresh.signal(), Step::Arm);
    assert_eq!(refresh.signal(), Step::Coalesce);
    assert_eq!(refresh.signal(), Step::Coalesce);
}

#[test]
fn a_timer_that_fires_reads_the_tree_once() {
    let mut refresh = Refresh::new(Protocol::GtkMenus);

    refresh.signal();

    assert!(refresh.fire());
    assert!(!refresh.finish(), "one read owes no follow-up");
}

#[test]
fn a_change_during_a_read_owes_exactly_one_follow_up() {
    let mut refresh = Refresh::new(Protocol::DBusMenu);

    refresh.signal();
    assert!(refresh.fire());
    assert_eq!(refresh.signal(), Step::Arm);
    assert!(!refresh.fire(), "a read is already out; the next one waits");

    assert!(
        refresh.finish(),
        "the waiting read runs when the first one finishes"
    );
    assert!(!refresh.finish());
}

#[test]
fn a_gtk_change_during_a_read_owes_the_same_single_follow_up() {
    let mut refresh = Refresh::new(Protocol::GtkMenus);

    refresh.signal();
    assert!(refresh.fire());
    refresh.signal();
    refresh.signal();

    assert!(!refresh.fire());
    assert!(refresh.finish());
    assert!(!refresh.finish());
}

#[test]
fn an_identical_tree_keeps_the_menu_that_is_open() {
    let current = file_menu("Quit");

    assert!(!menuwatch::replaces(
        Protocol::GtkMenus,
        &current,
        &current.clone()
    ));
    assert!(!menuwatch::replaces(
        Protocol::DBusMenu,
        &current,
        &current.clone()
    ));
}

#[test]
fn a_changed_row_replaces_the_tree() {
    let current = file_menu("Quit");

    assert!(menuwatch::replaces(
        Protocol::GtkMenus,
        &current,
        &file_menu("Close")
    ));
    assert!(menuwatch::replaces(
        Protocol::DBusMenu,
        &current,
        &file_menu("Close")
    ));
}

#[test]
fn gtk_withdrawing_its_menus_is_an_answer_and_an_empty_layout_is_not() {
    let current = file_menu("Quit");

    assert!(menuwatch::replaces(Protocol::GtkMenus, &current, &[]));
    assert!(!menuwatch::replaces(Protocol::DBusMenu, &current, &[]));
    assert!(!menuwatch::replaces(Protocol::GtkMenus, &[], &[]));
    assert!(!menuwatch::replaces(Protocol::DBusMenu, &[], &[]));
}

#[test]
fn the_debounce_is_the_c_watchers_120_ms() {
    assert_eq!(menuwatch::DEBOUNCE_MS, 120);
}

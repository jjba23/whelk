// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::settings::SchemeValue;
use whelk_rs::startbar::mark_polygon;
use whelk_rs::startmenu::{CommandAction, MenuEntry, command_action};

fn symbol(text: &str) -> SchemeValue {
    SchemeValue::Symbol(text.to_owned())
}

fn text(value: &str) -> SchemeValue {
    SchemeValue::Text(value.to_owned())
}

#[test]
fn a_configured_menu_becomes_rows_in_order() {
    let units = vec![
        SchemeValue::List(vec![symbol("item"), text("About"), text("shell:about")]),
        SchemeValue::List(vec![symbol("separator")]),
        SchemeValue::List(vec![
            symbol("submenu"),
            text("Recent Items"),
            text("shell:recent-items"),
        ]),
    ];
    let entries = MenuEntry::from_units(&units);

    assert_eq!(entries.len(), 3);
    assert_eq!(
        MenuEntry::first(&entries),
        Some(&MenuEntry::Item {
            label: "About".to_owned(),
            action: Some("shell:about".to_owned()),
            enabled: true,
        })
    );
}

#[test]
fn the_no_scheme_fallback_still_offers_a_way_in_and_out() {
    let entries = MenuEntry::fallback_menu();

    assert!(entries.iter().any(|entry| matches!(
        entry,
        MenuEntry::Item { action: Some(action), .. } if action == "shell:launcher"
    )));
    assert!(entries.iter().any(|entry| matches!(
        entry,
        MenuEntry::Item { action: Some(action), .. } if action == "whelk --quit"
    )));
}

#[test]
fn shell_owned_reload_and_quit_do_not_spawn_a_second_shell() {
    assert_eq!(command_action("whelk --reload"), CommandAction::Reload);
    assert_eq!(command_action("whelk --quit"), CommandAction::Quit);
    assert!(matches!(
        command_action("systemctl suspend"),
        CommandAction::Run(command) if command == "systemctl suspend"
    ));
}

#[test]
fn the_mark_is_a_closed_ribbon_inside_the_requested_pixels() {
    let points = mark_polygon(24.0);

    assert!(points.len() > 400);

    for (x, y) in &points {
        assert!((0.0..=24.0).contains(x), "x={x}");
        assert!((0.0..=24.0).contains(y), "y={y}");
    }
}

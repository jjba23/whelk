// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::menubar::{Menu, MenuAction, MenuEntry};
use whelk_rs::menusearch::{ActionContext, flatten};

fn context() -> ActionContext {
    ActionContext {
        bus_name: Some("org.example.App".to_owned()),
        app_path: Some("/org/example/App".to_owned()),
    }
}

fn action(name: &str) -> MenuEntry {
    MenuEntry::item(name, Some(MenuAction::GtkActionName(format!("app.{name}"))))
}

#[test]
fn every_reachable_entry_becomes_one_row_with_its_path() {
    let menus = vec![
        Menu::with_entries(
            "File",
            vec![action("New"), MenuEntry::Separator, action("Save")],
        ),
        Menu::with_entries(
            "View",
            vec![MenuEntry::Item {
                label: "Appearance".to_owned(),
                enabled: true,
                accel: None,
                action: None,
                submenu: vec![MenuEntry::Item {
                    label: "Dark".to_owned(),
                    enabled: true,
                    accel: Some("Ctrl+Shift+D".to_owned()),
                    action: Some(MenuAction::Command("theme dark".to_owned())),
                    submenu: Vec::new(),
                }],
            }],
        ),
    ];

    let snapshot = flatten("Example", &context(), &menus);

    assert_eq!(snapshot.app_name, "Example");
    assert_eq!(
        snapshot
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>(),
        vec!["File > New", "File > Save", "View > Appearance > Dark",]
    );
    assert_eq!(snapshot.items[0].label, "New");
    assert_eq!(snapshot.items[2].label, "Dark");
    assert_eq!(snapshot.items[2].accel.as_deref(), Some("Ctrl+Shift+D"));
    assert_eq!(
        snapshot.items[2].action,
        MenuAction::Command("theme dark".to_owned())
    );
    assert_eq!(snapshot.items[0].context, context());
}

#[test]
fn rows_that_cannot_be_chosen_are_left_out() {
    let menus = vec![
        Menu::with_entries(
            "Edit",
            vec![
                MenuEntry::Item {
                    label: "Cut".to_owned(),
                    enabled: false,
                    accel: Some("Ctrl+X".to_owned()),
                    action: Some(MenuAction::Command("cut".to_owned())),
                    submenu: Vec::new(),
                },
                MenuEntry::Item {
                    label: "Copy".to_owned(),
                    enabled: true,
                    accel: Some("Ctrl+C".to_owned()),
                    action: None,
                    submenu: Vec::new(),
                },
                MenuEntry::Item {
                    label: String::new(),
                    enabled: true,
                    accel: None,
                    action: Some(MenuAction::Command("invisible".to_owned())),
                    submenu: Vec::new(),
                },
                MenuEntry::Separator,
            ],
        ),
        Menu::with_entries(
            "Help",
            vec![MenuEntry::Item {
                label: "About".to_owned(),
                enabled: true,
                accel: None,
                action: None,
                submenu: vec![MenuEntry::Item {
                    label: "Example".to_owned(),
                    enabled: true,
                    accel: None,
                    action: Some(MenuAction::Command("about".to_owned())),
                    submenu: Vec::new(),
                }],
            }],
        ),
    ];

    let snapshot = flatten("", &context(), &menus);

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].path, "Help > About > Example");
    assert_eq!(snapshot.items[0].label, "Example");
}

#[test]
fn a_menu_without_rows_publishes_nothing_to_search() {
    let menus = vec![Menu::new("File"), Menu::with_entries("Edit", Vec::new())];

    let snapshot = flatten("Example", &context(), &menus);

    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.app_name, "Example");
}

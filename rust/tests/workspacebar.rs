// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The workspace row is decided before GTK sees it: niri's and Hyprland's
// different answers become the same buttons, the configured format and
// hide-empty rule are applied, and each button carries the click command for
// its own session.

use std::collections::BTreeMap;

use whelk_rs::bar::{BarSettings, ModuleSpec, Section};
use whelk_rs::session::Session;
use whelk_rs::settings::SchemeValue;
use whelk_rs::workspacebar::{self, WorkspaceBarSpec};

const NIRI: &str = r#"[{"id": 1, "idx": 1, "name": "one", "is_active": true, "active_window_id": 42},
    {"id": 2, "idx": 2, "name": "two", "is_focused": true, "active_window_id": null},
    {"id": 3, "idx": 3, "name": "three", "is_active": false, "active_window_id": null}]"#;

const HYPRLAND: &str = r#"[{"id": 1, "name": "one", "windows": 2},
    {"id": 2, "name": "two", "windows": 0},
    {"id": -99, "name": "special:magic", "windows": 1}]

WHELK_HYPRLAND_MONITORS

[{"id": 0, "name": "DP-1", "activeWorkspace": {"id": 2, "name": "two"}}]"#;

fn module(kind: &str, options: Vec<(&str, SchemeValue)>) -> ModuleSpec {
    ModuleSpec {
        kind: kind.to_owned(),
        options: options
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn workspace_module(options: Vec<(&str, SchemeValue)>) -> ModuleSpec {
    module("workspaces", options)
}

#[test]
fn defaults_match_the_c_module() {
    let spec = WorkspaceBarSpec::from_module(&workspace_module(Vec::new()));

    assert_eq!(spec.format, "{index}");
    assert!(!spec.hide_empty);
}

#[test]
fn niri_buttons_keep_the_compositor_order() {
    let spec = WorkspaceBarSpec::from_module(&workspace_module(Vec::new()));
    let buttons = workspacebar::buttons(Session::Niri, NIRI, &spec).expect("niri row");

    assert_eq!(buttons.len(), 3);
    assert_eq!(buttons[0].index, 1);
    assert_eq!(buttons[0].label, "1");
    assert!(buttons[0].active);
    assert!(!buttons[0].empty);
    assert_eq!(
        buttons[0].focus_command.as_deref(),
        Some("niri msg action focus-workspace 1")
    );

    // is_focused is the older spelling of the same fact.
    assert!(buttons[1].active);
    assert!(buttons[1].empty);
    assert!(!buttons[2].active);
    assert!(buttons[2].empty);
}

#[test]
fn format_and_hide_empty_apply_to_both_sessions() {
    let spec = WorkspaceBarSpec::from_module(&workspace_module(vec![
        ("format", SchemeValue::Text("{index}: {name}".into())),
        ("hide-empty?", SchemeValue::Bool(true)),
        ("interval", SchemeValue::Integer(5)),
        ("css-class", SchemeValue::Text("my-workspaces".into())),
    ]));

    assert_eq!(spec.format, "{index}: {name}");
    assert!(spec.hide_empty);

    let niri = workspacebar::buttons(Session::Niri, NIRI, &spec).expect("niri row");

    assert_eq!(niri.len(), 1);
    assert_eq!(niri[0].label, "1: one");

    let hyprland = workspacebar::buttons(Session::Hyprland, HYPRLAND, &spec).expect("hypr row");

    assert_eq!(hyprland.len(), 1);
    assert_eq!(hyprland[0].label, "1: one");
    assert_eq!(
        hyprland[0].focus_command.as_deref(),
        Some("hyprctl dispatch workspace 1")
    );
}

#[test]
fn hyprland_active_workspace_comes_from_its_monitor() {
    let spec = WorkspaceBarSpec::from_module(&workspace_module(Vec::new()));
    let buttons = workspacebar::buttons(Session::Hyprland, HYPRLAND, &spec).expect("hypr row");

    assert_eq!(buttons.len(), 2);
    assert!(!buttons[0].active);
    assert!(!buttons[0].empty);
    assert!(buttons[1].active);
    assert!(buttons[1].empty);
    assert_eq!(
        buttons[1].focus_command.as_deref(),
        Some("hyprctl dispatch workspace 2")
    );
}

#[test]
fn malformed_answers_do_not_become_empty_rows() {
    let spec = WorkspaceBarSpec::from_module(&workspace_module(Vec::new()));

    assert_eq!(
        workspacebar::buttons(Session::Niri, "not json", &spec),
        None
    );
    assert_eq!(
        workspacebar::buttons(Session::Sway, "[]", &spec),
        None,
        "a session with no workspace IPC has no button row"
    );
    assert_eq!(
        workspacebar::buttons(Session::Niri, "[]", &spec),
        Some(Vec::new()),
        "an empty array is a real empty row"
    );
}

#[test]
fn substitution_is_literal_and_index_first() {
    let workspace = whelk_rs::workspaces::Workspace {
        index: 3,
        name: "room {index}".to_owned(),
        active: false,
        empty: false,
    };

    assert_eq!(
        workspacebar::label("{index}: {name} ({index})", &workspace),
        "3: room {index} (3)"
    );
}

#[test]
fn a_workspace_spec_keeps_its_configured_bar_half() {
    let mut bar = BarSettings::default();

    bar.center.push(workspace_module(Vec::new()));
    assert_eq!(bar.section("workspaces"), Some(Section::Center));

    // The factory's order is left, centre, right when a bar names a module
    // twice.
    bar.left.push(workspace_module(Vec::new()));
    assert_eq!(bar.section("workspaces"), Some(Section::Left));

    assert_eq!(bar.section("clock"), None);
}

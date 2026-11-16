// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The two workspace JSON shapes, against answers the compositors really print.
// Hyprland's command concatenates `workspaces' and `monitors' with a marker, so
// the active workspace can be read from the monitor that owns it.

use whelk_rs::session::Session;
use whelk_rs::workspaces::{self, Workspace};

#[test]
fn niri() {
    let text = r#"[{"id": 1, "idx": 1, "name": "one", "is_active": true,  "is_focused": false, "active_window_id": 42}, {"id": 2, "idx": 2, "name": "two", "is_active": false,  "is_focused": true, "active_window_id": null}, {"id": 3, "idx": -1, "name": "scratch"}]"#;
    let parsed = workspaces::parse(Session::Niri, text).expect("niri workspaces");

    assert_eq!(
        parsed,
        vec![
            Workspace {
                index: 1,
                name: "one".to_owned(),
                active: true,
                empty: false,
            },
            Workspace {
                index: 2,
                name: "two".to_owned(),
                active: true,
                empty: true,
            },
        ]
    );

    // A malformed answer is not an empty workspace set.
    assert_eq!(workspaces::parse(Session::Niri, "not json"), None);
    assert_eq!(workspaces::parse(Session::Niri, "{}"), None);
}

#[test]
fn hyprland() {
    let text = r#"[{"id": 1, "name": "one", "monitor": "DP-1",  "monitorID": 0, "windows": 2, "hasfullscreen": false,  "lastwindow": "0x1", "lastwindowtitle": "A"}, {"id": 2, "name": "two", "monitor": "DP-1",  "monitorID": 0, "windows": 0, "hasfullscreen": false,  "lastwindow": "0x0", "lastwindowtitle": ""}, {"id": -99, "name": "special:magic", "monitor": "DP-1",  "monitorID": 0, "windows": 1, "hasfullscreen": false}]

WHELK_HYPRLAND_MONITORS

[{"id": 0, "name": "DP-1", "focused": true,  "activeWorkspace": {"id": 2, "name": "two"}}]"#;
    let parsed = workspaces::parse(Session::Hyprland, text).expect("hyprland workspaces");

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].index, 1);
    assert_eq!(parsed[0].name, "one");
    assert!(!parsed[0].active);
    assert!(!parsed[0].empty);
    assert_eq!(parsed[1].index, 2);
    assert!(parsed[1].active);
    assert!(parsed[1].empty);

    // The workspaces answer alone is still usable; without monitors, no button
    // is active rather than every button being guessed active.
    let parsed = workspaces::parse(
        Session::Hyprland,
        r#"[{"id": 7, "name": "seven", "windows": 1}]"#,
    )
    .expect("workspaces without monitors");

    assert_eq!(parsed.len(), 1);
    assert!(!parsed[0].active);
}

#[test]
fn commands() {
    let niri = workspaces::command(Session::Niri).expect("niri command");
    let hypr = workspaces::command(Session::Hyprland).expect("hyprland command");

    assert!(niri.contains("niri msg -j workspaces"));
    assert!(hypr.contains("hyprctl -j workspaces"));
    // The three answers are separated by ordinary newline-producing echos, not
    // by escape sequences the shell might leave literal.
    assert!(hypr.contains("echo; echo WHELK_HYPRLAND_MONITORS; echo;"));
    assert!(hypr.contains("hyprctl -j monitors"));
    assert_eq!(workspaces::command(Session::Sway), None);
    assert_eq!(workspaces::command(Session::River), None);
    assert_eq!(workspaces::command(Session::Wayland), None);

    let niri_focus = workspaces::focus_command(Session::Niri, 3).expect("niri focus");
    let hypr_focus = workspaces::focus_command(Session::Hyprland, 3).expect("hyprland focus");

    assert!(niri_focus.contains("focus-workspace 3"));
    assert!(hypr_focus.contains("workspace 3"));
    assert_eq!(workspaces::focus_command(Session::River, 3), None);
    assert_eq!(workspaces::focus_command(Session::Niri, -1), None);
}

#[test]
fn empty() {
    assert_eq!(workspaces::parse(Session::Niri, "[]"), Some(Vec::new()));
    assert_eq!(workspaces::parse(Session::Hyprland, ""), None);
    assert_eq!(workspaces::parse(Session::River, "[]"), None);
}

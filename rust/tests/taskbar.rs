// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The taskbar's pure half: what each session's window list means, how the
//! windows group by workspace, and the commands a click and a middle click run.

use whelk_rs::session::Session;
use whelk_rs::taskbar;

const NIRI: &str = r#"[
  {"id": 1, "title": "main.rs — whelk", "app_id": "org.gnome.TextEditor",
   "workspace_id": 1, "is_focused": false},
  {"id": 7, "title": "Firefox", "app_id": "firefox", "workspace_id": 2,
   "is_focused": true},
  {"id": 9, "title": "", "app_id": "Alacritty", "workspace_id": 2,
   "is_focused": false}
]"#;

const HYPRLAND: &str = r#"[
  {"address": "0x55df", "title": "Terminal", "class": "Alacritty",
   "workspace": {"id": 3, "name": "3"}, "focusHistoryID": 3},
  {"address": "0x42aa", "title": "Editor", "class": "emacs",
   "workspace": {"id": 1, "name": "1"}, "focusHistoryID": 0}
]"#;

const SWAY: &str = r#"{
  "type": "root", "name": "root", "nodes": [
    {"type": "output", "name": "eDP-1", "nodes": [
      {"type": "workspace", "name": "2", "num": 2, "nodes": [
        {"type": "con", "name": "chat", "app_id": "Signal", "id": 42,
         "focused": true, "nodes": []}
      ]},
      {"type": "workspace", "name": "1", "num": 1, "nodes": [
        {"type": "con", "name": "editor", "app_id": "emacs", "id": 41,
         "focused": false, "nodes": []}
      ]}
    ]}
  ]
}"#;

const WMCTRL: &str = "\
0x01200007  0 4242 host  Edits — notes
0x00e00003  1 4243 host  Terminal
not a window line
";

#[test]
fn niri_windows_carry_their_id_workspace_and_focus() {
    let windows = taskbar::parse(Session::Niri, NIRI);

    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].id, "1");
    assert_eq!(windows[0].workspace, 1);
    assert!(!windows[0].active);
    assert_eq!(windows[1].id, "7");
    assert!(windows[1].active);
    // An empty title falls back to the application, which is all a bar can say.
    assert_eq!(windows[2].label(28), "Alacritty");
    assert_eq!(windows[0].label(10), "main.rs —…");
}

#[test]
fn hyprland_puts_the_focused_window_first() {
    let windows = taskbar::parse(Session::Hyprland, HYPRLAND);

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].id, "0x42aa");
    assert!(windows[0].active);
    assert_eq!(windows[0].workspace, 1);
    assert_eq!(windows[1].id, "0x55df");
    assert!(!windows[1].active);
}

#[test]
fn sway_walks_the_tree_and_remembers_the_workspace() {
    let windows = taskbar::parse(Session::Sway, SWAY);

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].id, "42");
    assert_eq!(windows[0].title, "chat");
    assert_eq!(windows[0].workspace, 2);
    assert!(windows[0].active);
    assert_eq!(windows[1].id, "41");
    assert_eq!(windows[1].workspace, 1);
    assert!(!windows[1].active);
}

#[test]
fn wmctrl_lines_become_windows_and_noise_is_dropped() {
    let windows = taskbar::parse(Session::X11, WMCTRL);

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].id, "0x01200007");
    assert_eq!(windows[0].title, "Edits — notes");
    assert_eq!(windows[0].workspace, 1);
    // wmctrl counts desktops from zero; a person counts from one.
    assert_eq!(windows[1].workspace, 2);
}

#[test]
fn windows_group_by_workspace_in_order() {
    let windows = taskbar::parse(Session::Niri, NIRI);
    let groups = taskbar::group(&windows);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0, 1);
    assert_eq!(groups[0].1.len(), 1);
    assert_eq!(groups[1].0, 2);
    assert_eq!(groups[1].1.len(), 2);
    // Inside a workspace the session's own order is kept.
    assert_eq!(groups[1].1[0].id, "7");
    assert_eq!(groups[1].1[1].id, "9");
}

#[test]
fn a_session_without_a_list_verb_has_no_modules() {
    assert!(taskbar::list_command(Session::River).is_none());
    assert!(taskbar::list_command(Session::Wayland).is_none());
    assert!(taskbar::parse(Session::River, NIRI).is_empty());
    assert!(taskbar::focus_command(Session::River, "1").is_none());
    assert!(taskbar::close_command(Session::Wayland, "1").is_none());
}

#[test]
fn a_click_focuses_and_a_middle_click_closes_with_the_sessions_verbs() {
    assert_eq!(
        taskbar::focus_command(Session::Niri, "7").as_deref(),
        Some("niri msg action focus-window --id 7")
    );
    assert_eq!(
        taskbar::close_command(Session::Niri, "7").as_deref(),
        Some("niri msg action close-window --id 7")
    );
    assert_eq!(
        taskbar::focus_command(Session::Hyprland, "0x42aa").as_deref(),
        Some("hyprctl dispatch focuswindow address:0x42aa")
    );
    assert_eq!(
        taskbar::close_command(Session::Sway, "42").as_deref(),
        Some("swaymsg '[con_id=42]' kill")
    );
    assert_eq!(
        taskbar::focus_command(Session::X11, "0x01200007").as_deref(),
        Some("wmctrl -i -a 0x01200007")
    );
    // An id that is shell syntax is quoted rather than spliced in.
    assert_eq!(
        taskbar::focus_command(Session::Niri, "1; rm -rf ~").as_deref(),
        Some("niri msg action focus-window --id '1; rm -rf ~'")
    );
}

#[test]
fn an_answer_this_module_does_not_understand_is_an_empty_taskbar() {
    assert!(taskbar::parse(Session::Niri, "").is_empty());
    assert!(taskbar::parse(Session::Niri, "not json").is_empty());
    assert!(taskbar::parse(Session::Hyprland, "{}").is_empty());
    assert!(taskbar::parse(Session::Sway, "[]").is_empty());
}

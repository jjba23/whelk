// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's key map is data, not a switch in the overlay: these tests pin
//! the built-in prefixes, the override that replaces one, the empty override
//! that disables one, and the longer-prefix rule.

use whelk_rs::launcherkeys::LauncherKeys;

#[test]
fn the_built_in_map_selects_each_mode() {
    let keys = LauncherKeys::new();

    assert_eq!(keys.default_mode(), "app");
    assert!(LauncherKeys::is_mode("clipboard"));
    assert!(!LauncherKeys::is_mode("not-a-mode"));
    assert_eq!(LauncherKeys::mode_count(), 10);
    assert_eq!(LauncherKeys::mode_at(0), Some("app"));
    assert_eq!(LauncherKeys::mode_at(99), None);

    assert_eq!(keys.mode_for_text("@firefox"), Some(("window", "firefox")));
    assert_eq!(keys.mode_for_text(">ls"), Some(("run", "ls")));
    assert_eq!(keys.mode_for_text("=2+2"), Some(("calc", "2+2")));
    assert_eq!(keys.mode_for_text("/etc"), Some(("file", "etc")));
    assert_eq!(keys.mode_for_text("?whelk"), Some(("web", "whelk")));
    assert_eq!(keys.mode_for_text("%host"), Some(("ssh", "host")));
    assert_eq!(keys.mode_for_text("!editor"), Some(("quit", "editor")));
    assert_eq!(keys.mode_for_text(";copy"), Some(("clipboard", "copy")));
    assert_eq!(keys.mode_for_text(":Save"), Some(("menuitem", "Save")));

    assert_eq!(keys.mode_for_text("firefox"), None);
}

#[test]
fn overrides_replace_and_can_disable_a_prefix() {
    let mut keys = LauncherKeys::new();

    keys.set_prefix("window", "");
    assert_eq!(keys.prefix("window"), "");
    assert_eq!(keys.mode_for_text("@firefox"), None);

    keys.set_prefix("run", "//");
    assert_eq!(keys.mode_for_text("//ls"), Some(("run", "ls")));
    assert_eq!(keys.mode_for_text(">ls"), None);

    keys.set_default_mode("clipboard");
    assert_eq!(keys.default_mode(), "clipboard");
    keys.set_default_mode("not-a-mode");
    assert_eq!(keys.default_mode(), "clipboard");
}

#[test]
fn the_longest_prefix_wins() {
    let mut keys = LauncherKeys::new();

    keys.set_prefix("ssh", ">>");
    assert_eq!(keys.mode_for_text(">>host"), Some(("ssh", "host")));
    assert_eq!(keys.mode_for_text(">ls"), Some(("run", "ls")));
}

#[test]
fn an_unknown_mode_cannot_be_edited() {
    let mut keys = LauncherKeys::new();

    keys.set_prefix("not-a-mode", "!");
    assert_eq!(keys.prefix("not-a-mode"), "");
    assert_eq!(keys.mode_for_text("!open"), Some(("quit", "open")));
}

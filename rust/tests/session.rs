// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::session::Session;

#[test]
fn environment_precedence() {
    assert_eq!(
        Session::from_values(
            Some("/niri"),
            Some("/sway"),
            Some("hypr"),
            None,
            None,
            None,
            None
        ),
        Session::Niri
    );
    assert_eq!(
        Session::from_values(None, Some("/sway"), Some("hypr"), None, None, None, None),
        Session::Sway
    );
    assert_eq!(
        Session::from_values(None, None, Some("hypr"), Some("/river"), None, None, None),
        Session::Hyprland
    );
    assert_eq!(
        Session::from_values(None, None, None, Some("/river"), None, None, None),
        Session::River
    );
}

#[test]
fn x11_guesses() {
    assert_eq!(
        Session::from_values(None, None, None, None, Some("x11"), Some(":0"), None),
        Session::X11
    );
    assert_eq!(
        Session::from_values(None, None, None, None, Some("X11"), Some(":0"), None),
        Session::X11
    );
    assert_eq!(
        Session::from_values(None, None, None, None, None, Some(":0"), None),
        Session::X11
    );
    // A Wayland display wins over a leftover DISPLAY.
    assert_eq!(
        Session::from_values(None, None, None, None, None, Some(":0"), Some("wayland-0")),
        Session::Wayland
    );
}

#[test]
fn empty_variables_do_not_count() {
    assert_eq!(
        Session::from_values(Some(""), Some(""), Some(""), Some(""), None, None, None),
        Session::Wayland
    );
    // Only an empty string is absent; a whitespace value is still set, as in
    // the C reader (`*niri` is true for a space).
    assert_eq!(
        Session::from_values(None, Some(" "), None, None, None, None, None),
        Session::Sway
    );
}

#[test]
fn names() {
    assert_eq!(Session::Niri.name(), "niri");
    assert_eq!(Session::Sway.name(), "sway");
    assert_eq!(Session::Hyprland.name(), "hyprland");
    assert_eq!(Session::River.name(), "river");
    assert_eq!(Session::X11.name(), "x11");
    assert_eq!(Session::Wayland.name(), "wayland");
}

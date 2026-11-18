// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! What assistive technology hears from the bar, without a display: the naming
//! rules a screen reader depends on, and the role each reading is offered as.

use gtk4 as gtk;
use whelk_rs::a11y::{module_label, module_name, module_role};

#[test]
fn a_reading_is_announced_as_the_text_it_draws() {
    assert_eq!(
        module_label("clock", "23:41", Some("{calendar}\nClick: calendar")),
        "23:41"
    );
    assert_eq!(module_label("battery", "82%", None), "82%");
}

#[test]
fn an_icon_only_reading_is_announced_by_its_tooltip() {
    // The volume tile draws a glyph; the tooltip's first line is what it means.
    assert_eq!(
        module_label(
            "audio",
            "",
            Some("Output: the default sink\nVolume: 40%\nClick: mixer")
        ),
        "Output: the default sink"
    );
}

#[test]
fn a_reading_with_neither_falls_back_to_its_kind() {
    assert_eq!(module_label("wifi", "", None), "Wi-Fi");
    assert_eq!(module_label("mpris", "   ", Some("  \n")), "now playing");
    assert_eq!(module_label("tray", "", None), "status icons");
}

#[test]
fn every_shipped_kind_has_a_human_name() {
    for kind in [
        "audio",
        "backlight",
        "battery",
        "bluetooth",
        "clock",
        "clipboard",
        "cpu",
        "custom",
        "dnd",
        "inhibit",
        "keyboard",
        "language",
        "memory",
        "menu",
        "mpris",
        "notifications",
        "power-profile",
        "scheme",
        "start",
        "tasks",
        "tray",
        "wallpaper",
        "wifi",
        "window",
        "workspaces",
    ] {
        let name = module_name(kind);

        assert!(!name.is_empty(), "{kind} has no accessible name");
        // A hyphen in the name is the giveaway that the identifier leaked
        // through: "now-playing" instead of "now playing".
        assert!(
            !name.replace("Wi-", "").contains('-'),
            "{kind} is announced as a bare identifier: {name}"
        );
    }
}

#[test]
fn an_unknown_kind_is_readable_rather_than_raw() {
    assert_eq!(module_name("weather-radar"), "weather radar");
}

#[test]
fn a_clickable_reading_is_a_button_and_a_plain_one_is_a_label() {
    assert_eq!(module_role(true), gtk::AccessibleRole::Button);
    assert_eq!(module_role(false), gtk::AccessibleRole::Label);
}

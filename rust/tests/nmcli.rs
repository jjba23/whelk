// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// A typed Wi-Fi password has to reach NetworkManager's saved profile.  These
// are the two halves of that: reading nmcli's terse answers, and the editor
// script that carries the key on standard input.

use whelk_rs::nmcli;

#[test]
fn split_plain() {
    assert_eq!(
        nmcli::split_terse("Manon-en-Joe:89:WPA2"),
        vec!["Manon-en-Joe", "89", "WPA2"]
    );
    assert_eq!(nmcli::split_terse(""), vec![""]);
}

#[test]
fn split_honours_escapes() {
    // An SSID may contain a colon, which nmcli escapes rather than lose.
    assert_eq!(
        nmcli::split_terse(r"Cafe\:Guest:77:WPA2"),
        vec!["Cafe:Guest", "77", "WPA2"]
    );

    // A backslash is escaped the same way, and a trailing one is kept.
    assert_eq!(nmcli::split_terse(r"a\\b:1"), vec![r"a\b", "1"]);
    assert_eq!(nmcli::split_terse(r"trailing\"), vec![r"trailing\"]);
    assert_eq!(nmcli::split_terse("open::88"), vec!["open", "", "88"]);
}

#[test]
fn profile_names() {
    let output = "Manon-en-Joe:802-11-wireless\n\
                  lo:loopback\n\
                  Cafe\\:Guest:802-11-wireless\n\
                  Wired:802-3-ethernet\n\
                  \n";

    assert_eq!(
        nmcli::wifi_profile_names(output),
        vec!["Manon-en-Joe", "Cafe:Guest"]
    );
    assert!(nmcli::wifi_profile_names("").is_empty());
}

#[test]
fn wifi_reading_hides_the_ssid() {
    // The bar should not name the network to anyone who can see the screen.
    assert_eq!(nmcli::wifi_format(false), "{icon}");
    assert_eq!(
        nmcli::wifi_tooltip(false),
        "Wi-Fi · {signal}%\nClick: networks"
    );

    // #:show-ssid? #t asks for the name back, in both places.
    assert_eq!(nmcli::wifi_format(true), "{icon} {ssid}");
    assert_eq!(
        nmcli::wifi_tooltip(true),
        "{ssid} · {signal}%\nClick: networks"
    );
}

#[test]
fn key_script_is_literal() {
    // The editor takes the rest of the line as the value: quotes are part of
    // the key, never syntax around it, and nothing is interpolated by a shell.
    let psk = r#"p@ss "word" $x\y; rm -rf /"#;

    assert_eq!(
        nmcli::key_update_script(psk),
        Some(
            "set 802-11-wireless-security.psk p@ss \"word\" $x\\y; rm -rf /\nsave\nquit\n"
                .to_owned()
        )
    );
}

#[test]
fn key_script_refuses_unwritable_keys() {
    assert_eq!(nmcli::key_update_script(""), None);

    // A newline would end the command and begin another.
    assert_eq!(nmcli::key_update_script("first\nsave"), None);
    assert_eq!(nmcli::key_update_script("tab\there"), None);
    assert_eq!(nmcli::key_update_script("ctrl\u{0001}"), None);
    assert_eq!(nmcli::key_update_script("delete\u{007f}"), None);

    // The editor trims the value's edges, so these would be stored changed.
    assert_eq!(nmcli::key_update_script(" leading"), None);
    assert_eq!(nmcli::key_update_script("trailing "), None);
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// Which tray items a bar that reads its radios itself leaves out.  The rule is
// an exact id: the loose word "network" once hid an item that only mentioned
// it in its title.

use whelk_rs::applets;

#[test]
fn network_applets() {
    assert!(applets::is_network("nm-applet"));
    assert!(applets::is_network("nm-tray"));
    assert!(applets::is_network("org.kde.networkmanagement"));

    // Ids are compared case-insensitively and trimmed.
    assert!(applets::is_network("NM-Applet"));
    assert!(applets::is_network("  nm-applet  "));
}

#[test]
fn network_near_misses() {
    // The old rule's words: none of these is an applet id.
    assert!(!applets::is_network("network"));
    assert!(!applets::is_network("wifi"));
    assert!(!applets::is_network("my-network-monitor"));

    // An id that merely starts with an applet's id is a different applet.
    assert!(!applets::is_network("nm-applet-extra"));
    assert!(!applets::is_network("org.kde.networkmanagement.something"));

    assert!(!applets::is_network(""));
    assert!(!applets::is_network("   "));
}

#[test]
fn bluetooth_applets() {
    assert!(applets::is_bluetooth("blueman"));
    assert!(applets::is_bluetooth("blueman-tray"));
    assert!(applets::is_bluetooth("blueberry"));
    assert!(applets::is_bluetooth("org.kde.bluetooth"));

    // Titles are not ids, and one radio's applet is not the other's.
    assert!(!applets::is_bluetooth("bluetooth"));
    assert!(!applets::is_bluetooth("bluetooth-sendto"));
    assert!(!applets::is_bluetooth("nm-applet"));
    assert!(!applets::is_network("blueman"));
    assert!(!applets::is_bluetooth(""));
}

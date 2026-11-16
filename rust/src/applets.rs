// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The tray applets a bar already speaks for, ported from `util/applets.c`.
//!
//! A bar with its own Wi-Fi and Bluetooth readings does not need an applet
//! that reads them too.  The applets are named by their StatusNotifierItem Id,
//! and only those exact ids are recognised: an item whose title happens to say
//! "network" is somebody else's icon and must stay.

const NETWORK_IDS: &[&str] = &[
    "nm-applet",
    "nm-tray",
    "network-manager-applet",
    "org.gnome.networkmanager",
    "org.kde.networkmanagement",
];

const BLUETOOTH_IDS: &[&str] = &[
    "blueman",
    "blueman-tray",
    "blueberry",
    "org.blueman.applet",
    "org.kde.bluetooth",
    "org.kde.bluedevil",
];

fn id_in(id: &str, known: &[&str]) -> bool {
    let id = id.trim_matches(|character: char| character.is_ascii_whitespace());

    if id.is_empty() {
        return false;
    }

    known
        .iter()
        .any(|candidate| id.eq_ignore_ascii_case(candidate))
}

pub fn is_network(id: &str) -> bool {
    id_in(id, NETWORK_IDS)
}

pub fn is_bluetooth(id: &str) -> bool {
    id_in(id, BLUETOOTH_IDS)
}

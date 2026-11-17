// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::networkpanel::{BluetoothState, is_bluetooth_address, parse_bluetooth, parse_wifi};

#[test]
fn wifi_parses_radio_active_and_sorted_access_points() {
    let state = parse_wifi(
        "radio:enabled\n\
         active:Cafe:Guest\n\
         Office:40:--\n\
         Cafe\\:Guest:70:WPA2\n\
         Guest:80:\n\
         Cafe\\:Guest:12:WPA1\n",
    );

    assert!(state.radio);
    assert_eq!(state.active.as_deref(), Some("Cafe:Guest"));
    assert_eq!(
        state
            .points
            .iter()
            .map(|point| (point.ssid.as_str(), point.signal, point.secured))
            .collect::<Vec<_>>(),
        [
            ("Guest", 80, false),
            ("Cafe:Guest", 70, true),
            ("Office", 40, false),
        ]
    );
    assert_eq!(state.status(), "Connected to Cafe:Guest");
}

#[test]
fn wifi_off_and_no_active_network_have_distinct_statuses() {
    let off = parse_wifi("radio:disabled\n");

    assert!(!off.radio);
    assert_eq!(off.status(), "Wi-Fi is off");
    assert_eq!(off.empty_message(), "Wi-Fi is off");

    let on = parse_wifi("radio:enabled\n");

    assert!(on.radio);
    assert_eq!(on.status(), "Not connected");
    assert_eq!(on.empty_message(), "No networks in range");
}

#[test]
fn wifi_ignores_blank_ssids_short_rows_and_duplicate_names() {
    let state = parse_wifi(
        "radio:enabled\n\
         :99:WPA2\n\
         one-field\n\
         Home:60:WPA2\n\
         Home:10:--\n",
    );

    assert_eq!(state.points.len(), 1);
    assert_eq!(state.points[0].ssid, "Home");
    assert_eq!(state.points[0].signal, 60);
    assert!(state.points[0].secured);
}

#[test]
fn bluetooth_marks_paired_devices_from_the_connected_section() {
    let state = parse_bluetooth(
        "powered:yes\n\
         --paired--\n\
         Device AA:BB:CC:DD:EE:FF Keyboard\n\
         Device 11:22:33:44:55:66 Mouse\n\
         --connected--\n\
         Device AA:BB:CC:DD:EE:FF Keyboard\n\
         Device 99:88:77:66:55:44 Headset\n",
    );

    assert!(state.powered);
    assert_eq!(state.devices.len(), 2);
    assert_eq!(state.devices[0].name, "Keyboard");
    assert_eq!(state.devices[0].mac, "AA:BB:CC:DD:EE:FF");
    assert!(state.devices[0].connected);
    assert_eq!(state.devices[1].name, "Mouse");
    assert!(!state.devices[1].connected);
    assert_eq!(state.status(), "2 paired devices");
}

#[test]
fn bluetooth_off_and_no_paired_devices_have_distinct_statuses() {
    let off = BluetoothState::default();

    assert_eq!(off.status(), "Bluetooth is off");
    assert_eq!(off.empty_message(), "Bluetooth is off");

    let on = parse_bluetooth("powered:yes\n--paired--\n--connected--\n");

    assert!(on.powered);
    assert_eq!(on.status(), "Bluetooth is on");
    assert_eq!(on.empty_message(), "No paired devices");
}

#[test]
fn bluetooth_rejects_addresses_that_could_be_shell_syntax() {
    assert!(is_bluetooth_address("AA:BB:CC:DD:EE:FF"));
    assert!(!is_bluetooth_address("'; reboot; '"));
    assert!(!is_bluetooth_address("AA:BB:CC:DD:EE"));
    assert!(!is_bluetooth_address("AA:BB:CC:DD:EE:GG"));
    assert!(!is_bluetooth_address("AA-BB-CC-DD-EE-FF"));

    let state = parse_bluetooth(
        "powered:yes\n\
         --paired--\n\
         Device AA:BB:CC:DD:EE:FF Keyboard\n\
         Device ';reboot;' Fake\n\
         Device AA:BB:CC:DD:EE:GG Bad\n\
         --connected--\n\
         Device AA:BB:CC:DD:EE:FF Keyboard\n",
    );

    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].mac, "AA:BB:CC:DD:EE:FF");
}

#[test]
fn bluetooth_ignores_malformed_device_lines() {
    let state = parse_bluetooth(
        "powered:yes\n\
         --paired--\n\
         Device only-address\n\
         Device\n\
         Device AA:BB:CC:DD:EE:FF Speaker\n",
    );

    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, "Speaker");
}

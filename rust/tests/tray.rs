// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::tray;

#[test]
fn watcher_entries_become_a_service_and_path() {
    assert_eq!(
        tray::split_entry("org.example.App/StatusNotifierItem"),
        Some((
            "org.example.App".to_owned(),
            "/StatusNotifierItem".to_owned()
        ))
    );
    assert_eq!(
        tray::split_entry("org.example.App"),
        Some((
            "org.example.App".to_owned(),
            "/StatusNotifierItem".to_owned()
        ))
    );
    assert_eq!(tray::split_entry(""), None);
}

#[test]
fn hiding_uses_exact_ids_until_the_user_names_fragments() {
    assert!(tray::should_hide(Some("nm-applet"), None, "", true, false));
    assert!(tray::should_hide(Some("blueman"), None, "", false, true));
    assert!(!tray::should_hide(
        Some("org.example.network"),
        Some("Network"),
        "",
        true,
        true
    ));

    // A configured rule is the user's own, and it overrides both the
    // automatic ids and the native readings.
    assert!(tray::should_hide(
        Some("org.example.nextcloud"),
        None,
        "Nextcloud, Steam",
        false,
        false
    ));
    assert!(tray::should_hide(
        None,
        Some("Steam status"),
        "steam",
        false,
        false
    ));
    assert!(!tray::should_hide(
        Some("nm-applet"),
        None,
        "steam",
        true,
        true
    ));
}

#[test]
fn item_limit_and_overflow_follow_the_c_module() {
    assert_eq!(tray::max_items(None), 6);
    assert_eq!(tray::max_items(Some(0)), 1);
    assert_eq!(tray::max_items(Some(3)), 3);
    assert_eq!(tray::visible_count(8, 3), 3);
    assert!(!tray::needs_overflow(3, 3));
    assert!(tray::needs_overflow(4, 3));
}

#[test]
fn argb32_pixmaps_are_validated_and_reordered() {
    let pixmap = tray::argb32_pixmap_to_rgba(1, 1, &[0x11, 0x22, 0x33, 0x44]).unwrap();

    assert_eq!((pixmap.width, pixmap.height), (1, 1));
    assert_eq!(pixmap.rgba, vec![0x22, 0x33, 0x44, 0x11]);
    assert_eq!(tray::argb32_pixmap_to_rgba(0, 1, &[0; 4]), None);
    assert_eq!(tray::argb32_pixmap_to_rgba(2, 2, &[0; 15]), None);
}

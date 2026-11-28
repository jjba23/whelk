// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::Path;

use bibelot::widgetstate;

#[test]
fn the_state_file_is_keyed_by_the_settings_file_stem() {
    let path = widgetstate::state_path_in(
        Path::new("/home/joe/.config/whelk/settings.scm"),
        Some(Path::new("/state")),
        Some(Path::new("/home/joe")),
    );

    assert_eq!(path, Some("/state/whelk/widgets-settings.ini".into()));

    let other = widgetstate::state_path_in(
        Path::new("/etc/whelk/desk"),
        Some(Path::new("/state")),
        None,
    );

    assert_eq!(other, Some("/state/whelk/widgets-desk.ini".into()));
}

#[test]
fn without_xdg_state_home_the_file_lives_under_local_state() {
    assert_eq!(
        widgetstate::state_path_in(
            Path::new("/home/joe/.config/whelk/settings.scm"),
            None,
            Some(Path::new("/home/joe")),
        ),
        Some("/home/joe/.local/state/whelk/widgets-settings.ini".into())
    );

    assert_eq!(
        widgetstate::state_path_in(
            Path::new("/home/joe/settings.scm"),
            Some(Path::new("")),
            Some(Path::new("/home/joe")),
        ),
        Some("/home/joe/.local/state/whelk/widgets-settings.ini".into())
    );
}

#[test]
fn a_settings_path_without_a_name_or_a_directory_has_no_state_file() {
    assert_eq!(
        widgetstate::state_path_in(Path::new("/"), None, Some(Path::new("/home/joe"))),
        None
    );
    assert_eq!(
        widgetstate::state_path_in(Path::new("settings.scm"), None, None),
        None
    );
}

#[test]
fn dragging_snaps_to_the_eight_pixel_grid() {
    assert_eq!(bibelot::widgetstate::snap(3, 8), 0);
    assert_eq!(bibelot::widgetstate::snap(4, 8), 8);
    assert_eq!(bibelot::widgetstate::snap(12, 8), 16);
    assert_eq!(bibelot::widgetstate::snap(-5, 8), 0);
    assert_eq!(bibelot::widgetstate::snap(37, 0), 37);
}

#[test]
fn a_card_stays_inside_its_monitor() {
    assert_eq!(
        bibelot::widgetstate::clamp(10, 20, 200, 100, 1920, 1080),
        (10, 20)
    );
    assert_eq!(
        bibelot::widgetstate::clamp(1900, 1000, 200, 100, 1920, 1080),
        (1720, 980)
    );
    assert_eq!(
        bibelot::widgetstate::clamp(-20, -5, 200, 100, 1920, 1080),
        (0, 0)
    );
    assert_eq!(
        bibelot::widgetstate::clamp(50, 50, 2000, 1200, 1920, 1080),
        (0, 0)
    );
}

#[test]
fn a_saved_position_survives_a_reload() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-settings.ini");

    assert!(bibelot::widgetstate::save_position(
        &path,
        "widget-clock-0",
        40,
        64
    ));
    assert_eq!(
        bibelot::widgetstate::load_position(&path, "widget-clock-0"),
        Some((40, 64))
    );
    assert_eq!(
        bibelot::widgetstate::load_position(&path, "widget-notes-1"),
        None
    );
}

#[test]
fn a_position_is_remembered_per_monitor_and_falls_back_to_the_last_place() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-settings.ini");

    assert!(bibelot::widgetstate::save_position_for(
        &path,
        "widget-clock-0",
        Some("DP-1"),
        100,
        120
    ));

    assert_eq!(
        bibelot::widgetstate::load_position_for(&path, "widget-clock-0", Some("DP-1")),
        Some((100, 120))
    );

    // The monitor-less key was written too: it is where an unplugged monitor's
    // card goes when it comes back on a screen the file never saw.
    assert_eq!(
        bibelot::widgetstate::load_position_for(&path, "widget-clock-0", Some("HDMI-1")),
        Some((100, 120))
    );
    assert_eq!(
        bibelot::widgetstate::load_position_for(&path, "widget-clock-0", None),
        Some((100, 120))
    );
}

#[test]
fn a_named_monitor_keeps_its_own_position() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-settings.ini");

    bibelot::widgetstate::save_position_for(&path, "widget-clock-0", Some("DP-1"), 8, 8);
    bibelot::widgetstate::save_position_for(&path, "widget-clock-0", Some("HDMI-1"), 16, 24);

    assert_eq!(
        bibelot::widgetstate::load_position_for(&path, "widget-clock-0", Some("DP-1")),
        Some((8, 8))
    );
    assert_eq!(
        bibelot::widgetstate::load_position_for(&path, "widget-clock-0", Some("HDMI-1")),
        Some((16, 24))
    );

    let text = std::fs::read_to_string(&path).expect("read state file");

    assert!(text.contains("[positions]"), "{text}");
    assert!(text.contains("widget-clock-0@DP-1=8,8"), "{text}");
    assert!(text.contains("widget-clock-0@HDMI-1=16,24"), "{text}");
    assert!(text.contains("widget-clock-0=16,24"), "{text}");
}

#[test]
fn a_negative_or_malformed_position_is_refused() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-settings.ini");

    assert!(!bibelot::widgetstate::save_position(
        &path,
        "widget-clock-0",
        -8,
        0
    ));
    assert!(!bibelot::widgetstate::save_position(
        &path,
        "widget-clock-0",
        0,
        -8
    ));
    assert_eq!(
        bibelot::widgetstate::load_position(&path, "widget-clock-0"),
        None
    );

    std::fs::write(
        &path,
        "[positions]\nwidget-clock-0=nonsense\nwidget-notes-1=12,16,20\n",
    )
    .expect("write a broken state file");

    assert_eq!(
        bibelot::widgetstate::load_position(&path, "widget-clock-0"),
        None
    );
    assert_eq!(
        bibelot::widgetstate::load_position(&path, "widget-notes-1"),
        None
    );
}

#[test]
fn per_widget_state_round_trips_and_is_removed() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-settings.ini");

    assert!(bibelot::widgetstate::set(
        &path,
        "widget-reminders-2",
        "items",
        Some("Water the plants\n[x] Ship the release")
    ));
    assert_eq!(
        bibelot::widgetstate::get(&path, "widget-reminders-2", "items").as_deref(),
        Some("Water the plants\n[x] Ship the release")
    );

    let text = std::fs::read_to_string(&path).expect("read state file");

    assert!(text.contains("[state]"), "{text}");
    assert!(text.contains("widget-reminders-2.items="), "{text}");

    assert!(bibelot::widgetstate::set(
        &path,
        "widget-reminders-2",
        "items",
        None
    ));
    assert_eq!(
        bibelot::widgetstate::get(&path, "widget-reminders-2", "items"),
        None
    );
}

#[test]
fn a_missing_state_file_is_the_first_run_not_an_error() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("widgets-none.ini");

    assert_eq!(bibelot::widgetstate::load_position(&path, "any"), None);
    assert_eq!(bibelot::widgetstate::get(&path, "any", "items"), None);
    assert!(bibelot::widgetstate::set(
        &path,
        "any",
        "items",
        Some("one")
    ));
    assert!(path.is_file());
}

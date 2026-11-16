// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::time::Duration;

use whelk_rs::wallpaper_view::{ken_burns_zoom, transition_progress};

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.000_001
}

#[test]
fn transition_progress_clamps_and_completes_at_zero_duration() {
    assert!(near(transition_progress(Duration::ZERO, 1200), 0.0));
    assert!(near(
        transition_progress(Duration::from_millis(600), 1200),
        0.5
    ));
    assert!(near(
        transition_progress(Duration::from_millis(1200), 1200),
        1.0
    ));
    assert!(near(
        transition_progress(Duration::from_secs(10), 1200),
        1.0
    ));
    assert!(near(transition_progress(Duration::from_secs(10), 0), 1.0));
}

#[test]
fn ken_burns_is_off_at_both_ends_of_one_breath() {
    assert!(near(ken_burns_zoom(false, Duration::ZERO), 1.0));
    assert!(near(ken_burns_zoom(true, Duration::ZERO), 1.0));
    assert!(near(ken_burns_zoom(true, Duration::from_secs(30)), 1.06));
    assert!(near(ken_burns_zoom(true, Duration::from_secs(60)), 1.0));
}

#[test]
fn ken_burns_is_slow_enough_to_be_a_background() {
    let one_second = ken_burns_zoom(true, Duration::from_secs(1));
    let ten_seconds = ken_burns_zoom(true, Duration::from_secs(10));

    assert!(one_second > 1.0);
    assert!(one_second < 1.001);
    assert!(ten_seconds > one_second);
    assert!(ten_seconds < 1.02);
}

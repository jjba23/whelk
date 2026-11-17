// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::Path;
use std::time::{Duration, SystemTime};

use gdk_pixbuf::prelude::*;
use whelk_rs::wallpaper_surface::{Output, frame_delay, needs_rebuild, signature};

#[test]
fn a_zero_or_missing_frame_delay_becomes_100ms() {
    assert_eq!(frame_delay(None), Duration::from_millis(100));
    assert_eq!(
        frame_delay(Some(Duration::ZERO)),
        Duration::from_millis(100)
    );
    assert_eq!(
        frame_delay(Some(Duration::from_millis(1))),
        Duration::from_millis(100)
    );
    assert_eq!(
        frame_delay(Some(Duration::from_millis(19))),
        Duration::from_millis(100)
    );
}

#[test]
fn useful_frame_delays_are_kept() {
    assert_eq!(
        frame_delay(Some(Duration::from_millis(20))),
        Duration::from_millis(20)
    );
    assert_eq!(
        frame_delay(Some(Duration::from_millis(250))),
        Duration::from_millis(250)
    );
}

#[test]
fn an_animated_gif_exposes_and_advances_its_frames() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/animated.gif");
    let animation = gdk_pixbuf::PixbufAnimation::from_file(path).expect("animated gif");

    assert!(!animation.is_static_image());

    let iter = animation.iter(None);
    let first = iter.pixbuf();
    let delay = iter.delay_time().expect("first frame delay");

    assert!(delay >= Duration::from_millis(20));
    assert!(
        iter.advance(SystemTime::now() + delay),
        "the second frame should be due"
    );

    let second = iter.pixbuf();
    assert_ne!(first.read_pixel_bytes(), second.read_pixel_bytes());
}

#[test]
fn an_output_set_signs_itself_with_the_connector_and_size() {
    let outputs = [
        Output {
            connector: Some("eDP-1".to_owned()),
            width: 1920,
            height: 1200,
        },
        Output {
            connector: Some("HDMI-1".to_owned()),
            width: 2560,
            height: 1440,
        },
    ];

    assert_eq!(signature(&outputs), "eDP-1:1920x1200;HDMI-1:2560x1440;");
    // Identical outputs and one surface each are the case that must not tear
    // down a running movie.
    assert!(!needs_rebuild(
        Some("eDP-1:1920x1200;HDMI-1:2560x1440;"),
        &outputs,
        2
    ));
}

#[test]
fn a_resolution_or_connector_change_rebuilds_the_backgrounds() {
    let before = [Output {
        connector: Some("eDP-1".to_owned()),
        width: 1920,
        height: 1200,
    }];
    let resized = [Output {
        connector: Some("eDP-1".to_owned()),
        width: 2560,
        height: 1600,
    }];
    let other = [Output {
        connector: Some("DP-2".to_owned()),
        width: 1920,
        height: 1200,
    }];
    let old = signature(&before);

    assert_ne!(signature(&resized), old);
    assert!(needs_rebuild(Some(&old), &resized, 1));
    assert!(needs_rebuild(Some(&old), &other, 1));
}

#[test]
fn a_missing_connector_is_a_question_mark() {
    let outputs = [Output {
        connector: None,
        width: 800,
        height: 600,
    }];

    assert_eq!(signature(&outputs), "?:800x600;");
}

#[test]
fn the_first_presentation_and_a_stale_surface_count_rebuild() {
    let outputs = [Output {
        connector: Some("eDP-1".to_owned()),
        width: 1920,
        height: 1200,
    }];
    let signed = signature(&outputs);

    // No previous signature yet: nothing has been built.
    assert!(needs_rebuild(None, &outputs, 0));
    // One output but two surfaces (or none) is a stale set.
    assert!(needs_rebuild(Some(&signed), &outputs, 2));
    assert!(needs_rebuild(Some(&signed), &outputs, 0));
    assert!(!needs_rebuild(Some(&signed), &outputs, 1));
}

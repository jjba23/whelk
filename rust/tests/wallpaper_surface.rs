// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::Path;
use std::time::{Duration, SystemTime};

use gdk_pixbuf::prelude::*;
use whelk_rs::wallpaper_surface::frame_delay;

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

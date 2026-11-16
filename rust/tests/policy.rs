// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::wallpaper::{
    Animation, Fit, Geometry, Kind, ease, is_supported, kind_for_path, next_index,
};

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.0001
}

#[test]
fn extension_table_matches_the_c_engine() {
    assert_eq!(kind_for_path("/w/a.png"), Kind::Image);
    assert_eq!(kind_for_path("/w/a.JPEG"), Kind::Image);
    assert_eq!(kind_for_path("a.webp"), Kind::Image);
    assert_eq!(kind_for_path("a.avif"), Kind::Image);
    assert_eq!(kind_for_path("a.gif"), Kind::Image);
    assert_eq!(kind_for_path("a.svg"), Kind::Image);
    assert_eq!(kind_for_path("a.mp4"), Kind::Video);
    assert_eq!(kind_for_path("a.WEBM"), Kind::Video);
    assert!(!is_supported("notes.txt"));
    assert!(!is_supported("README"));
    assert!(!is_supported("/home/me/.walls/README"));
}

#[test]
fn every_animation_name_round_trips_and_parsing_is_case_insensitive() {
    for animation in Animation::ALL {
        assert_eq!(Animation::parse(animation.name()), Some(animation));
        assert_eq!(
            Animation::parse(&animation.name().to_ascii_uppercase()),
            Some(animation)
        );
    }

    assert_eq!(Animation::parse("explode"), None);
    assert_eq!(Animation::parse("slide--left"), None);
}

#[test]
fn fit_names_round_trip() {
    for fit in [Fit::Cover, Fit::Contain, Fit::Fill] {
        assert_eq!(Fit::parse(fit.name()), Some(fit));
    }

    assert_eq!(Fit::parse("stretch"), None);
}

#[test]
fn smoothstep_has_fixed_ends() {
    assert!(near(ease(0.0), 0.0));
    assert!(near(ease(1.0), 1.0));
    assert!(near(ease(-1.0), 0.0));
    assert!(near(ease(2.0), 1.0));
}

#[test]
fn transition_math_matches_the_c_view() {
    assert!(near(Animation::Crossfade.alpha_in(0.0), 0.0));
    assert!(near(Animation::Crossfade.alpha_in(1.0), 1.0));
    assert!(near(Animation::Crossfade.alpha_out(0.5), 1.0));

    assert!(near(Animation::Fade.alpha_in(0.5), 0.0));
    assert!(near(Animation::Fade.alpha_out(0.5), 0.0));
    assert!(near(Animation::Fade.alpha_in(1.0), 1.0));
    assert!(near(Animation::Fade.alpha_out(0.0), 1.0));

    let left = Animation::SlideLeft.offsets(0.5, 1000.0, 500.0);
    assert_eq!(
        left,
        whelk_rs::wallpaper::Offsets {
            out_x: -500.0,
            out_y: 0.0,
            in_x: 500.0,
            in_y: 0.0,
        }
    );

    let down = Animation::SlideDown.offsets(0.25, 1000.0, 500.0);
    assert!(near(down.out_y, 125.0));
    assert!(near(down.in_y, -375.0));
    assert!(near(down.out_x, 0.0));

    assert!(Animation::Zoom.zoom_in(0.0) > 1.0);
    assert!(near(Animation::Zoom.zoom_in(1.0), 1.0));
    assert!(near(Animation::Crossfade.zoom_in(0.0), 1.0));
}

#[test]
fn fit_geometry_matches_the_c_engine() {
    let cover = Fit::Cover.geometry(1920.0, 1080.0, 1000.0, 1000.0, 1.0);
    assert_eq!(
        cover,
        Geometry {
            scale_x: 1.92,
            scale_y: 1.92,
            offset_x: 0.0,
            offset_y: -420.0,
        }
    );

    let contain = Fit::Contain.geometry(1920.0, 1080.0, 1000.0, 1000.0, 1.0);
    assert!(near(contain.scale_x, 1.08));
    assert!(near(contain.offset_x, 420.0));
    assert!(near(contain.offset_y, 0.0));

    let fill = Fit::Fill.geometry(1920.0, 1080.0, 1000.0, 1000.0, 1.0);
    assert!(near(fill.scale_x, 1.92));
    assert!(near(fill.scale_y, 1.08));
    assert!(near(fill.offset_x, 0.0));

    let zoomed = Fit::Cover.geometry(100.0, 100.0, 100.0, 100.0, 1.1);
    assert!(near(zoomed.scale_x, 1.1));
    assert!(near(zoomed.offset_x, -5.0));

    let empty = Fit::Cover.geometry(0.0, 100.0, 100.0, 100.0, 1.0);
    assert_eq!(empty.scale_x, 1.0);
    assert_eq!(empty.offset_x, 0.0);
}

#[test]
fn rotation_is_sequential_or_reproducibly_shuffled() {
    assert_eq!(next_index(0, 0, false, &mut 0), 0);
    assert_eq!(next_index(1, 0, false, &mut 0), 0);
    assert_eq!(next_index(3, 2, false, &mut 0), 0);

    let mut state = 12_345;
    let mut previous = 0;

    for _ in 0..40 {
        let next = next_index(5, previous, true, &mut state);
        assert!(next < 5);
        assert_ne!(next, previous);
        previous = next;
    }

    let mut a = 99;
    let mut b = 99;
    assert_eq!(
        next_index(7, 0, true, &mut a),
        next_index(7, 0, true, &mut b)
    );
}

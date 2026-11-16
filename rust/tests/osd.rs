// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::bar::{BarSettings, Edge as BarEdge};
use whelk_rs::osd;

#[test]
fn the_hud_sits_above_the_bar_or_at_the_default_margin() {
    let bottom = BarSettings {
        edge: BarEdge::Bottom,
        height: 42,
        ..BarSettings::default()
    };
    let top = BarSettings {
        edge: BarEdge::Top,
        height: 42,
        ..BarSettings::default()
    };

    assert_eq!(osd::bottom_margin(&bottom), 64);
    assert_eq!(osd::bottom_margin(&top), 64);

    let tall = BarSettings {
        edge: BarEdge::Bottom,
        height: 70,
        ..BarSettings::default()
    };

    assert_eq!(osd::bottom_margin(&tall), 92);
}

#[test]
fn progress_fills_to_one_hundred_and_clamps_the_rest() {
    assert_eq!(osd::progress(-10), 0.0);
    assert_eq!(osd::progress(0), 0.0);
    assert_eq!(osd::progress(50), 0.5);
    assert_eq!(osd::progress(100), 1.0);
    assert_eq!(osd::progress(150), 1.0);
}

#[test]
fn the_value_label_keeps_the_surrounding_percentage() {
    assert_eq!(osd::value_text(0), "0%");
    assert_eq!(osd::value_text(100), "100%");
    assert_eq!(osd::value_text(150), "150%");
}

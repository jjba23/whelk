// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! C parity for the launcher's pure halves: how a query finds a name, what an
//! arithmetic expression is worth, and the card's geometry.

use whelk_rs::calc::{eval, format};
use whelk_rs::fuzzy::{best, score};
use whelk_rs::launcher;

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.01
}

#[test]
fn fuzzy_finds_a_subsequence() {
    assert!(score("ffx", "Firefox").expect("ffx") > 0);
    assert!(score("wterm", "WezTerm").expect("wterm") > 0);
    assert_eq!(score("xyz", "Firefox"), None);

    // An empty query matches everything.
    assert_eq!(score("", "Firefox"), Some(0));
    assert_eq!(score("f", ""), None);
}

#[test]
fn fuzzy_orders_by_what_people_mean() {
    // A prefix beats a match buried in the middle, and a word start beats an
    // interior character.
    assert!(score("term", "Terminal") > score("term", "WezTerm"));
    assert!(score("fx", "Firefox") > score("fx", "Suffix"));

    // Case does not matter.
    assert_eq!(score("FIRE", "firefox"), score("fire", "firefox"));
}

#[test]
fn fuzzy_picks_the_best_candidate() {
    let candidates = ["Files", "Firefox", "Terminal"];

    assert_eq!(best("fire", &candidates), Some(1));
    assert_eq!(best("term", &candidates), Some(2));
    assert_eq!(best("zzz", &candidates), None);
    assert_eq!(best("fi", &candidates), Some(0));
}

#[test]
fn calculator_arithmetic() {
    assert_eq!(eval("1 + 2 * 3"), Some(7.0));
    assert_eq!(eval("(1 + 2) * 3"), Some(9.0));
    assert_eq!(eval("2 ^ 3 ^ 2"), Some(512.0));
    assert_eq!(eval("-4 + 10 / 5"), Some(-2.0));
    assert_eq!(eval("10 % 4"), Some(2.0));
    assert_eq!(eval("1.5 * 2"), Some(3.0));
}

#[test]
fn calculator_refusals() {
    assert_eq!(eval(""), None);
    assert_eq!(eval("1 +"), None);
    assert_eq!(eval("1 / 0"), None);
    assert_eq!(eval("hello"), None);
    assert_eq!(eval("2x"), None);
    assert_eq!(eval("(1 + 2"), None);
}

#[test]
fn calculator_format() {
    assert_eq!(format(42.0), "42");
    assert_eq!(format(2.5), "2.5");
}

#[test]
fn scroll_follows_the_selection() {
    assert!(near(
        launcher::scroll_value(0.0, 100.0, 0.0, 500.0, 150.0, 180.0),
        80.0
    ));
    assert!(near(
        launcher::scroll_value(80.0, 100.0, 0.0, 500.0, 300.0, 330.0),
        230.0
    ));
    assert!(near(
        launcher::scroll_value(80.0, 100.0, 0.0, 500.0, 70.0, 100.0),
        70.0
    ));

    // A list shorter than the viewport has nowhere to scroll.
    assert!(near(
        launcher::scroll_value(0.0, 100.0, 0.0, 60.0, 10.0, 30.0),
        0.0
    ));
}

#[test]
fn a_click_outside_the_card_closes_it() {
    assert!(!launcher::point_outside(
        50.0, 100.0, 10.0, 20.0, 110.0, 220.0
    ));
    assert!(!launcher::point_outside(
        10.0, 20.0, 10.0, 20.0, 110.0, 220.0
    ));
    assert!(launcher::point_outside(
        9.0, 100.0, 10.0, 20.0, 110.0, 220.0
    ));
    assert!(launcher::point_outside(50.0, 9.0, 10.0, 20.0, 110.0, 220.0));
    assert!(launcher::point_outside(
        111.0, 100.0, 10.0, 20.0, 110.0, 220.0
    ));
    assert!(launcher::point_outside(
        50.0, 221.0, 10.0, 20.0, 110.0, 220.0
    ));
}

#[test]
fn appearance_bounds() {
    assert_eq!(launcher::clamp_width(100), 360);
    assert_eq!(launcher::clamp_width(720), 720);
    assert_eq!(launcher::clamp_width(3000), 1600);
    assert_eq!(launcher::clamp_height(100), 300);
    assert_eq!(launcher::clamp_height(700), 700);
    assert_eq!(launcher::clamp_height(2000), 1200);
    assert_eq!(launcher::clamp_icon_size(1), 16);
    assert_eq!(launcher::clamp_icon_size(32), 32);
    assert_eq!(launcher::clamp_icon_size(200), 64);
    assert_eq!(launcher::clamp_font_scale(20), 75);
    assert_eq!(launcher::clamp_font_scale(125), 125);
    assert_eq!(launcher::clamp_font_scale(500), 200);
}

#[test]
fn the_launcher_surface_is_the_card_not_the_output() {
    // A 1880x1253 output with the default 660x640 card: the surface keeps room
    // for the shadow, sits centred, and holds the card's top at 120.
    let placement = launcher::surface_placement(1880, 1253, 660, 640, 120);

    assert_eq!(placement.width, 660 + 2 * launcher::SHADOW_ROOM);
    assert_eq!(placement.height, 640 + 2 * launcher::SHADOW_ROOM);
    assert_eq!(placement.left, (1880 - placement.width) / 2);
    assert_eq!(placement.top + launcher::SHADOW_ROOM, 120);
    // The point of the cut: a fullscreen blur is expensive, a card-sized one
    // is not, and the surface stays far below the output's area.
    assert!(
        placement.width * placement.height < 1880 * 1253 / 4,
        "the surface must stay a fraction of the output"
    );
}

#[test]
fn the_launcher_surface_fits_a_small_output() {
    // The card is clamped to the launcher's own bounds, so a small output is
    // the case where the surface has to give way instead.
    let placement = launcher::surface_placement(320, 240, 1600, 1200, 120);

    assert!(placement.width <= 320);
    assert!(placement.height <= 240);
    assert_eq!(placement.left, 0);
    assert_eq!(placement.top, 0);
    assert_eq!(
        placement,
        launcher::SurfacePlacement {
            left: 0,
            top: 0,
            width: 320,
            height: 240,
        }
    );
}

#[test]
fn the_card_keeps_its_place_when_the_output_is_short() {
    // A card taller than the room under the top edge is pushed up, never off
    // the output.
    let placement = launcher::surface_placement(1280, 720, 720, 680, 120);

    assert_eq!(placement.top, 0);
    assert!(placement.height <= 720);
}

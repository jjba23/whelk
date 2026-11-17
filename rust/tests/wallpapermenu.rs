// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper menu's decisions, without a display: how much of a collection
//! the card shows before the picker takes over, and where the card lands for a
//! click on the desktop or a click on the bar's reading.

use whelk_rs::wallpapermenu;

const OUTPUT: (i32, i32) = (1880, 1253);

#[test]
fn a_big_collection_hands_the_rest_to_the_picker() {
    // Four by three is what a menu can show at once; the row that opens the
    // picker appears exactly when there is something it cannot show.
    assert_eq!(wallpapermenu::capacity(), 12);
    assert!(!wallpapermenu::needs_browser(0));
    assert!(!wallpapermenu::needs_browser(3));
    assert!(!wallpapermenu::needs_browser(12));
    assert!(wallpapermenu::needs_browser(13));
    assert!(wallpapermenu::needs_browser(400));
}

#[test]
fn a_click_near_a_corner_keeps_the_card_on_the_output() {
    let card = (480, 320);

    // Room to the right and below: the pointer is the card's top-left.
    let corner = wallpapermenu::placement_near_point(OUTPUT, card, (200, 300));

    assert_eq!(corner, (206, 306));

    // Near the bottom-right the card flips to the other side of the pointer
    // instead of being pushed under it.
    let flipped = wallpapermenu::placement_near_point(OUTPUT, card, (1860, 1240));

    assert!(flipped.0 + card.0 <= 1860, "{flipped:?}");
    assert!(flipped.1 + card.1 <= 1240, "{flipped:?}");
    assert!(flipped.0 >= 0 && flipped.1 >= 0, "{flipped:?}");
}

#[test]
fn the_card_hangs_under_a_top_bar_and_above_a_bottom_one() {
    let card = (480, 320);

    let under = wallpapermenu::placement_under(OUTPUT, card, 940, 38, false);

    assert_eq!(under.0, 940 - card.0 / 2);
    assert_eq!(under.1, 38 + 6);

    let above = wallpapermenu::placement_under(OUTPUT, card, 940, 1253 - 38, true);

    assert_eq!(above.0, 940 - card.0 / 2);
    assert_eq!(above.1 + card.1, 1253 - 38 - 6);
}

#[test]
fn a_card_bigger_than_the_output_is_pinned_to_its_corner() {
    let corner = wallpapermenu::placement_near_point((200, 120), (480, 320), (150, 100));

    assert_eq!(corner, (0, 0));
    assert_eq!(
        wallpapermenu::clamp_inside((200, 120), (480, 320), -40, -900),
        (0, 0)
    );
}

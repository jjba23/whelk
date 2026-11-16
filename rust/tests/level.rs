// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The first reading a bar takes is not a change: showing a HUD on startup for
//! a volume nobody touched would be the shell talking to itself.

use whelk_rs::level::Level;

#[test]
fn the_first_reading_is_not_a_change() {
    let mut level = Level::default();

    assert!(!level.update(42, false));
    assert!(level.known);
    assert_eq!(level.percent, 42);
}

#[test]
fn the_same_reading_is_not_a_change() {
    let mut level = Level::default();

    level.update(42, false);
    assert!(!level.update(42, false));
}

#[test]
fn a_different_number_is_a_change() {
    let mut level = Level::default();

    level.update(42, false);
    assert!(level.update(55, false));
    assert_eq!(level.percent, 55);
    assert!(!level.update(55, false));
}

#[test]
fn muting_is_a_change_at_the_same_number() {
    let mut level = Level::default();

    level.update(42, false);
    assert!(level.update(42, true));
    assert!(level.muted);
    assert!(!level.update(42, true));
    assert!(level.update(42, false));
}

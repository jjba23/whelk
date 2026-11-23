// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::globalmenu::visible_prefix;

#[test]
fn a_row_that_fits_keeps_every_menu() {
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 184, 24), 3);
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 300, 24), 3);
}

#[test]
fn a_narrow_row_keeps_the_longest_prefix_before_the_chevron() {
    // Two buttons plus their gap and the chevron need 148px; one needs 86px.
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 147, 24), 1);
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 148, 24), 2);
}

#[test]
fn no_menu_fits_when_only_the_chevron_does() {
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 60, 24), 0);
    assert_eq!(visible_prefix(&[60, 60, 60], 2, 23, 24), 0);
}

#[test]
fn an_empty_row_has_no_prefix() {
    assert_eq!(visible_prefix(&[], 2, 100, 24), 0);
}

#[test]
fn spacing_is_counted_once_per_gap() {
    // Widths 40 + 40, no gaps, chevron 26: 106.  With two 3px gaps: 112.
    assert_eq!(visible_prefix(&[40, 40, 40], 0, 106, 26), 2);
    assert_eq!(visible_prefix(&[40, 40, 40], 3, 111, 26), 1);
    assert_eq!(visible_prefix(&[40, 40, 40], 3, 112, 26), 2);
}

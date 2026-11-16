// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's small geometry decisions, ported from
//! `util/launchergeometry.c`.
//!
//! They live here rather than in the widgets so they can be tested without a
//! display: the scroll value that brings a selected row into view, whether a
//! click landed outside the card, and the bounds of the window, icon and font
//! zoom.

/// The scroll value that brings a selected row into view.
///
/// The scroll position follows the selected row, not just the selection: an
/// arrow key at the bottom of the view must bring the next row into view.
pub fn scroll_value(
    current: f64,
    page: f64,
    lower: f64,
    upper: f64,
    row_top: f64,
    row_bottom: f64,
) -> f64 {
    let maximum = lower.max(upper - page);
    let mut value = current;

    if row_top < value {
        value = row_top;
    } else if row_bottom > value + page {
        value = row_bottom - page;
    }

    value.clamp(lower, maximum)
}

/// Whether a click landed outside the card (and should close the overlay).
pub fn point_outside(x: f64, y: f64, left: f64, top: f64, right: f64, bottom: f64) -> bool {
    x < left || x >= right || y < top || y >= bottom
}

/// The card can be dragged bigger than a laptop screen if nothing stops it, and
/// a 2px icon or 20% text is not a usable launcher.  These are the same bounds
/// the settings window's spin buttons use.
pub fn clamp_width(width: i32) -> i32 {
    width.clamp(360, 1600)
}

pub fn clamp_height(height: i32) -> i32 {
    height.clamp(300, 1200)
}

pub fn clamp_icon_size(size: i32) -> i32 {
    size.clamp(16, 64)
}

pub fn clamp_font_scale(percent: i32) -> i32 {
    percent.clamp(75, 200)
}

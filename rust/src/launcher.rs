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

/// The room the card's own CSS shadow needs inside the surface: it paints
/// `0 24px 70px` under the card, and a surface cut to the card's border box
/// would clip it.
pub const SHADOW_ROOM: i32 = 36;

/// Where the launcher's layer surface goes, in output-relative logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePlacement {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// The smallest surface a launcher card can sit in.
///
/// A layer surface that covers the whole output makes the compositor blur the
/// whole output: niri applies a layer rule's `background-effect { blur true }`
/// to the surface's own geometry, so a fullscreen launcher means a fullscreen
/// blur on every keystroke and every caret blink, which is what made the
/// launcher feel slow on a machine with blur enabled.  The surface is therefore
/// the card plus the room its shadow needs, centred horizontally and hung
/// `top` below the output's top edge through layer-shell margins.
pub fn surface_placement(
    output_width: i32,
    output_height: i32,
    card_width: i32,
    card_height: i32,
    top: i32,
) -> SurfacePlacement {
    let output_width = output_width.max(1);
    let output_height = output_height.max(1);
    let width = (card_width.max(1) + 2 * SHADOW_ROOM).min(output_width);
    let height = (card_height.max(1) + 2 * SHADOW_ROOM).min(output_height);
    let left = ((output_width - width) / 2).max(0);
    let top = (top - SHADOW_ROOM).clamp(0, (output_height - height).max(0));

    SurfacePlacement {
        left,
        top,
        width,
        height,
    }
}

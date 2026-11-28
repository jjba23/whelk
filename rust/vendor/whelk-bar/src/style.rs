// whelk-bar -- a bar surface for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The bar's shared spacing rhythm.

/// The horizontal padding inside one bar module, from the bar's `spacing`.
pub fn module_padding(spacing: i32) -> i32 {
    (spacing / 2).clamp(2, 12)
}

/// The gap between an icon and its label inside one module.
pub fn module_inner_spacing(spacing: i32) -> i32 {
    (spacing * 2 / 3).max(2)
}

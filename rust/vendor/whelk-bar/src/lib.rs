// whelk-bar -- a bar surface for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The bar engine split out of [whelk](https://github.com/jjba23/whelk).
//!
//! A host describes bars as typed [`bar::BarSettings`], supplies one widget per
//! configured module and a stylesheet, and gets a presented layer-shell bar
//! back.  The project owns placement, folding, auto-hide, the module host and
//! the keyboard/accessibility path; it does not depend on WheLK.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod a11y;
pub mod autohide;
pub mod bar;
pub mod barmodule;
pub mod barmonitors;
pub mod command;
pub mod format;
pub mod sources;
pub mod style;
pub mod surface;

/// The version of this crate, for a host's about page.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

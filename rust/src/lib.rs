// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Rust half of whelk's migration.
//!
//! The C shell still builds and runs.  This crate starts the replacement from
//! the inside out: pure policy that can be tested without a display, a small
//! audited Guile boundary, and a GTK4 bootstrap that presents the shell's first
//! Rust surface.  Each layer has a stable Rust API so a later commit can move
//! callers over without rewriting the logic again.

pub mod bar;
pub mod calc;
pub mod chord;
pub mod collection;
pub mod fuzzy;
pub mod guile;
pub mod idlepolicy;
pub mod inhibitors;
pub mod launcher;
pub mod lockauth;
pub mod locker;
pub mod lockpolicy;
pub mod logind;
pub mod rotation;
pub mod settings;
pub mod style;
pub mod ui;
pub mod wallpaper;
pub mod wallpaper_surface;
pub mod wallpaper_view;

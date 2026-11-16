// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A reading that can change, ported from `util/level.c`.
//!
//! The volume and the backlight are polled, so a module sees the same number
//! over and over; only a move should raise the on-screen display, and the first
//! reading a module ever gets is not a move -- nobody pressed anything.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Level {
    pub percent: i32,
    pub muted: bool,
    pub known: bool,
}

impl Level {
    /// Records the reading and returns `true` when it differs from a reading
    /// this level already had.  The first call only fills it in.
    pub fn update(&mut self, percent: i32, muted: bool) -> bool {
        if !self.known {
            self.percent = percent;
            self.muted = muted;
            self.known = true;

            return false;
        }

        if self.percent == percent && self.muted == muted {
            return false;
        }

        self.percent = percent;
        self.muted = muted;

        true
    }
}

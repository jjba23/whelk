// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! When the idle lock timer should fire, ported from `util/idlepolicy.c`.
//!
//! The manager's clock is logind's `IdleHint`: the session says the seat is
//! idle, and whelk decides what happens next.  This is that decision as a pure
//! function, so the interesting cases -- a zero-second policy, an inhibitor, a
//! timer that has already run out -- are tested without a bus, a compositor or
//! a wall clock.

/// No timer should run: the session is active, an inhibitor suspends the
/// policy, or timed locking is disabled.
pub const NO_TIMER: i64 = -1;

/// Seconds until the lock, `0` for "now", or [`NO_TIMER`].
///
/// `lock_after` is the configured idle seconds; `0` locks as soon as the
/// session reports idle, and a negative value disables timed locking.  A
/// negative `idle_for` counts as zero, so a hint that arrives before the
/// clock's epoch cannot postpone the lock.
pub fn lock_delay(lock_after: i64, idle: bool, inhibited: bool, idle_for: i64) -> i64 {
    if !idle || inhibited || lock_after < 0 {
        return NO_TIMER;
    }

    // Zero is the compatible policy: the old shell locked on the first
    // IdleHint, and a user who has not configured a delay still gets exactly
    // that.
    if lock_after == 0 {
        return 0;
    }

    let idle_for = idle_for.max(0);

    if idle_for >= lock_after {
        return 0;
    }

    lock_after - idle_for
}

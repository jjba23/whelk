// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The idle manager's decision, without a bus, a compositor or a clock.

use whelk_rs::idlepolicy::{NO_TIMER, lock_delay};

#[test]
fn an_active_session_has_no_timer() {
    assert_eq!(lock_delay(300, false, false, 0), NO_TIMER);
}

#[test]
fn zero_locks_as_soon_as_idle_is_reported() {
    assert_eq!(lock_delay(0, true, false, 0), 0);
}

#[test]
fn a_positive_policy_counts_down_from_the_first_hint() {
    assert_eq!(lock_delay(300, true, false, 0), 300);
    assert_eq!(lock_delay(300, true, false, 42), 258);
    assert_eq!(lock_delay(300, true, false, 300), 0);
    assert_eq!(lock_delay(300, true, false, 999), 0);
}

#[test]
fn an_inhibitor_suspends_the_timer() {
    assert_eq!(lock_delay(300, true, true, 42), NO_TIMER);
}

#[test]
fn a_negative_policy_disables_timed_locking() {
    assert_eq!(lock_delay(-1, true, false, 42), NO_TIMER);
}

#[test]
fn a_negative_elapsed_time_counts_as_zero() {
    assert_eq!(lock_delay(300, true, false, -5), 300);
}

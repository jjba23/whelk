// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The lock screen's rebuild policy, tested without a display: C's
// `locker_rebuild`/`queue_session_rebuild` flags are one small state machine,
// and every transition here is one a monitor change can hit.

use whelk_rs::locker::RebuildState;

#[test]
fn a_monitor_change_queues_one_rebuild() {
    let mut state = RebuildState::Idle;

    assert!(state.request());
    assert_eq!(state, RebuildState::Queued);
    // A burst of monitor changes is still one rebuild.
    assert!(!state.request());
    assert_eq!(state, RebuildState::Queued);

    assert!(state.start());
    assert_eq!(state, RebuildState::Running);
    assert!(!state.start(), "a rebuild cannot start twice");

    state.finish();
    assert_eq!(state, RebuildState::Idle);
    assert!(state.request(), "the next change queues again");
}

#[test]
fn a_change_caused_by_a_rebuild_is_swallowed() {
    let mut state = RebuildState::Idle;

    state.request();
    state.start();

    // Destroying and recreating lock windows makes the monitor model emit
    // again; C's `rebuilding_session_lock` guard drops that answer.
    assert!(!state.request());
    assert_eq!(state, RebuildState::Running);

    state.finish();
    assert_eq!(state, RebuildState::Idle);
}

#[test]
fn an_unlocked_session_never_rebuilds() {
    let mut state = RebuildState::Idle;

    assert!(state.request());
    state.stop();

    assert_eq!(state, RebuildState::Stopped);
    assert!(
        !state.start(),
        "a queued idle must not rebuild after unlock"
    );
    assert!(!state.request());

    state.finish();
    assert_eq!(state, RebuildState::Stopped, "finish cannot revive it");
}

#[test]
fn a_rebuild_that_never_started_cannot_finish_into_another() {
    let mut state = RebuildState::Idle;

    assert!(!state.start());
    assert_eq!(state, RebuildState::Idle);
    assert!(state.request());
}

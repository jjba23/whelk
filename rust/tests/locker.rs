// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The lock screen's rebuild policy, tested without a display: C's
// `locker_rebuild`/`queue_session_rebuild` flags are one small state machine,
// and every transition here is one a monitor change can hit.

use whelk_rs::locker::{
    EXIT_NO_SURFACE, RebuildState, Recovery, presentation_blocker, recovery_action,
};

#[test]
fn a_lock_needs_a_display_and_a_presentation_path() {
    assert_eq!(
        presentation_blocker(false, true, true),
        Some("there is no display to cover")
    );
    assert_eq!(
        presentation_blocker(true, false, false),
        Some("the compositor has neither ext-session-lock-v1 nor layer shell")
    );

    // Either path is enough: the hardened one where the compositor has it,
    // the layer-shell overlay where it does not.
    assert_eq!(presentation_blocker(true, true, false), None);
    assert_eq!(presentation_blocker(true, false, true), None);
}

#[test]
fn a_dead_lock_is_replaced_or_retried() {
    // It had a surface and died without unlocking: take the lock again, so the
    // compositor is not left showing its own background forever.
    assert_eq!(recovery_action(None, true, false), Recovery::Restart);
    assert_eq!(recovery_action(Some(1), true, true), Recovery::Restart);

    // It released the lock because nothing reached the screen: one retry with
    // the software renderer, then stop instead of looping.
    let no_surface = Some(i32::from(EXIT_NO_SURFACE));

    assert_eq!(
        recovery_action(no_surface, false, false),
        Recovery::Software
    );
    assert_eq!(recovery_action(no_surface, false, true), Recovery::None);

    // An ordinary unlock, or a lock that never took the session, is left alone.
    assert_eq!(recovery_action(Some(0), false, false), Recovery::None);
    assert_eq!(recovery_action(Some(1), false, false), Recovery::None);
}

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

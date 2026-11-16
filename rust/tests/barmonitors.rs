// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A bar's monitor rule is the difference between a layout and a fallback.
//! These are the decisions the shell makes on every build and every hotplug.

use whelk_rs::bar::When;
use whelk_rs::barmonitors::{DEFAULT_MONITOR, select_monitors, when_matches};

fn outputs() -> Vec<String> {
    vec!["eDP-1".to_owned(), "DP-1".to_owned()]
}

#[test]
fn all_selects_every_output() {
    assert_eq!(
        select_monitors(When::Always, "all", &[], &outputs()),
        vec![0, 1]
    );
}

#[test]
fn a_named_connector_waits_for_its_screen() {
    let outputs = outputs();

    assert_eq!(
        select_monitors(When::Always, "DP-1", &[], &outputs),
        vec![1]
    );
    // The external screen is gone: this bar waits for it rather than moving to
    // the laptop's panel.
    assert!(select_monitors(When::Always, "DP-1", &[], &outputs[..1]).is_empty());
}

#[test]
fn a_connector_list_selects_the_present_names() {
    let list = vec!["eDP-1".to_owned(), "HDMI-A-1".to_owned()];

    assert_eq!(
        select_monitors(When::Always, "all", &list, &outputs()),
        vec![0]
    );
    assert!(select_monitors(When::Always, "all", &list, &[]).is_empty());
}

#[test]
fn the_output_count_rule_is_applied_first() {
    assert!(when_matches(When::Multi, 2));
    assert!(!when_matches(When::Multi, 1));
    assert!(!when_matches(When::Multi, 0));
    assert!(when_matches(When::Single, 1));
    assert!(!when_matches(When::Single, 2));
    assert!(when_matches(When::Always, 0));

    let outputs = outputs();

    assert_eq!(
        select_monitors(When::Multi, "all", &[], &outputs),
        vec![0, 1]
    );
    assert!(select_monitors(When::Multi, "all", &[], &outputs[..1]).is_empty());
    assert_eq!(
        select_monitors(When::Single, "all", &[], &outputs[..1]),
        vec![0]
    );
    assert!(select_monitors(When::Single, "all", &[], &outputs).is_empty());
}

#[test]
fn default_asks_the_compositor_and_an_empty_model_still_gets_a_surface() {
    assert_eq!(
        select_monitors(When::Always, "default", &[], &outputs()),
        vec![DEFAULT_MONITOR]
    );
    // No monitor model: "all" still gets one surface, which the compositor
    // places; that is what a start before the outputs are announced needs.
    assert_eq!(
        select_monitors(When::Always, "all", &[], &[]),
        vec![DEFAULT_MONITOR]
    );
}

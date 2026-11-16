// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Which bar goes where, ported from `util/barmonitors.c`.
//!
//! A bar's monitor rule is deliberately declarative: a connector name, a list
//! of connectors, `all`, `default`, and an output-count condition.  The rule is
//! evaluated against the monitor model each time the bars are built, so a
//! hotplug re-evaluates it the same way a start does.
//!
//! The selection returns indexes into the caller's monitor list.  The sentinel
//! [`DEFAULT_MONITOR`] means "let the compositor choose", which is what a
//! `default` rule means and what a machine with no monitor model gets.

use crate::bar::When;

pub const DEFAULT_MONITOR: i32 = -1;

/// Whether a bar with this output-count rule belongs on a machine with
/// `monitor_count` outputs.
pub fn when_matches(when: When, monitor_count: usize) -> bool {
    match when {
        When::Single => monitor_count == 1,
        When::Multi => monitor_count > 1,
        When::Always => true,
    }
}

/// Applies the `when` condition first, then the connector rule.
///
/// A named connector that is not present selects nothing: a bar configured for
/// the external screen disappears when that screen does, rather than silently
/// moving to the laptop's panel.  That is the difference between a layout and
/// a fallback.
pub fn select_monitors(
    when: When,
    monitor: &str,
    monitors: &[String],
    connectors: &[String],
) -> Vec<i32> {
    let mut selected = Vec::new();

    if !when_matches(when, connectors.len()) {
        return selected;
    }

    if !monitors.is_empty() {
        for (index, connector) in connectors.iter().enumerate() {
            if monitors.iter().any(|wanted| wanted == connector) {
                select_one(&mut selected, index as i32);
            }
        }

        return selected;
    }

    let rule = if monitor.is_empty() { "all" } else { monitor };

    if rule == "all" {
        if connectors.is_empty() {
            select_one(&mut selected, DEFAULT_MONITOR);
        } else {
            for index in 0..connectors.len() {
                select_one(&mut selected, index as i32);
            }
        }

        return selected;
    }

    if rule == "default" {
        select_one(&mut selected, DEFAULT_MONITOR);

        return selected;
    }

    for (index, connector) in connectors.iter().enumerate() {
        if connector == rule {
            select_one(&mut selected, index as i32);
        }
    }

    selected
}

fn select_one(selected: &mut Vec<i32>, index: i32) {
    if !selected.contains(&index) {
        selected.push(index);
    }
}

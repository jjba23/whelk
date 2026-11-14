// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_BARMONITORS_H
#define WHELK_BARMONITORS_H

#include "config.h"

#include <glib.h>

/* =========================================================================
 * WHICH BAR GOES WHERE
 * =========================================================================
 * A bar's monitor rule is deliberately declarative: a connector name, a list
 * of connectors, "all", "default", and an output-count condition.  The rule
 * is evaluated against the monitor model each time the bars are built, so a
 * hotplug re-evaluates it the same way a start does.
 *
 * The selection returns indexes into the caller's monitor list.  The sentinel
 * WHELK_MONITOR_DEFAULT (-1) means "let the compositor choose", which is what
 * a "default" rule means and what a machine with no monitor model gets.
 */

#define WHELK_MONITOR_DEFAULT (-1)

/* TRUE when a bar with this output-count rule belongs on a machine with
 * @monitor_count outputs. */
gboolean whelk_bar_when_matches(WhelkBarWhen when, guint monitor_count);

/**
 * whelk_bar_select_monitors:
 * @bar: the bar's rule.
 * @monitor_count: how many monitors the display currently has.
 * @connectors: (array length=monitor_count) (nullable): each monitor's
 *   connector name, in the display model's order.
 * @out: (out caller-allocates): cleared and filled with monitor indexes, or
 *   WHELK_MONITOR_DEFAULT.  An empty array means the bar is not drawn.
 *
 * Applies @bar's `when` condition first, then its connector rule.  A named
 * connector that is not present selects nothing: a bar configured for the
 * external screen disappears when that screen does, rather than silently
 * moving to the laptop's panel.  That is the difference between a layout and
 * a fallback.
 */
void whelk_bar_select_monitors(const WhelkBarConfig *bar, guint monitor_count,
                               const char *const *connectors, GArray *out);

#endif // WHELK_BARMONITORS_H

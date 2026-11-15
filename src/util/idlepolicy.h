// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_IDLEPOLICY_H
#define WHELK_IDLEPOLICY_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * WHEN THE LOCK TIMER SHOULD FIRE
 * =========================================================================
 * The manager's clock is logind's IdleHint: the session says the seat is idle,
 * and whelk decides what happens next.  This is that decision as a pure
 * function, so the interesting cases -- a zero-second policy, an inhibitor,
 * a timer that has already run out -- are tested without a bus, a compositor
 * or a wall clock.
 */

#define WHELK_IDLE_NO_TIMER (-1)

/**
 * whelk_idle_lock_delay:
 * @lock_after: the configured idle seconds before locking; 0 locks as soon as
 *   the session reports idle, and a negative value disables timed locking.
 * @idle: whether logind currently reports the session idle.
 * @inhibited: whether whelk is holding its presentation inhibitor.
 * @idle_for: seconds since the session became idle.
 *
 * Returns: seconds until the lock, 0 for "now", or
 *   #WHELK_IDLE_NO_TIMER when no timer should run (the session is active, an
 *   inhibitor suspends the policy, or timed locking is disabled).
 */
int whelk_idle_lock_delay(int lock_after, gboolean idle, gboolean inhibited,
                          gint64 idle_for);

G_END_DECLS

#endif // WHELK_IDLEPOLICY_H

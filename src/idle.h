// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_IDLE_H
#define WHELK_IDLE_H

#include <glib-object.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE IDLE AND INHIBIT MANAGER
 * =========================================================================
 * One object owns the three answers that used to be scattered:
 *
 *   - the idle timer.  The session's logind IdleHint starts it, and it locks
 *     after `idle-settings #:lock-after' seconds.  Zero means lock on the first
 *     idle hint, which is what the shell did before it had a manager;
 *   - the presentation inhibitor.  While it is on, whelk holds a real logind
 *     "idle" inhibitor (an fd from Manager.Inhibit) and pauses its own
 *     countdown.  The lock is a logind idle lock; a compositor that blanks on
 *     its own idle protocol is still the compositor's to control;
 *   - the do-not-disturb flag.  The notification service consults this object
 *     instead of keeping a copy.  DND silences toasts; it deliberately does
 *     not pause the lock timer, and the status says so.
 *
 * `whelk_idle_describe' is the answer to "why did the screen blank, or not":
 * the hint, the timer, whelk's own lock, and every inhibitor systemd reports.
 */

#define WHELK_TYPE_IDLE (whelk_idle_get_type())
G_DECLARE_FINAL_TYPE(WhelkIdle, whelk_idle, WHELK, IDLE, GObject)

typedef void (*WhelkIdleLockFunc)(gpointer user_data);

WhelkIdle *whelk_idle_new(void);

/* Starts watching logind.  Idempotent; the manager still works (with no bus)
 * as the owner of DND and the presentation toggle if logind is absent. */
void whelk_idle_start(WhelkIdle *self);

/* Stops the timer, releases whelk's inhibitor fd and drops the logind watch. */
void whelk_idle_shutdown(WhelkIdle *self);

/* What to run when the timer fires.  The shell passes its lock path, so the
 * manager does not need to know about lock screens. */
void whelk_idle_set_lock_action(WhelkIdle *self, WhelkIdleLockFunc action,
                                gpointer user_data);

/* Seconds of idle before locking; 0 locks immediately, negative disables. */
void whelk_idle_set_lock_after(WhelkIdle *self, int seconds);
int whelk_idle_lock_after(WhelkIdle *self);

gboolean whelk_idle_is_idle(WhelkIdle *self);

/* Presentation mode: take/release a logind idle inhibitor.  Returns FALSE when
 * the lock could not be taken (usually: no system bus), and leaves the state
 * unchanged so a caller can report an honest failure. */
gboolean whelk_idle_set_inhibited(WhelkIdle *self, gboolean inhibited,
                                  const char *reason);
gboolean whelk_idle_is_inhibited(WhelkIdle *self);
const char *whelk_idle_inhibit_reason(WhelkIdle *self);

gboolean whelk_idle_has_dnd(WhelkIdle *self);
/* Emits WhelkIdle::dnd-changed. */
void whelk_idle_set_dnd(WhelkIdle *self, gboolean dnd);

/* A human-readable report, including the system's inhibitor list. */
char *whelk_idle_describe(WhelkIdle *self);

G_END_DECLS

#endif // WHELK_IDLE_H

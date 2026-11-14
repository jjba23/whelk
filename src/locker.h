// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LOCKER_H
#define WHELK_LOCKER_H

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE SCREEN LOCKER
 * =========================================================================
 * Locking is not a panel that can be clicked away.  Every output gets an
 * overlay layer surface with the keyboard taken exclusively; the only input
 * that reaches the session is a password, and the only thing that removes the
 * surfaces is a successful PAM authentication for the user who owns the shell.
 *
 * The security properties are deliberate:
 *
 *   - fail closed: a missing PAM, a conversation error, an account-management
 *     refusal or a missing display all leave the session locked;
 *   - no username field: PAM is asked about the session user, not about a name
 *     typed at the lock screen;
 *   - no bypass on the bus: there is no unlock method, no environment variable
 *     and no command-line flag that skips authentication.  `whelk --quit` is a
 *     process signal, not an unlock;
 *   - one attempt at a time, on a worker thread, with the password wiped as
 *     soon as PAM has seen it and a doubling delay after each failure;
 *   - logind's Lock signal locks the session, so `loginctl lock-session` and a
 *     desktop's lock key reach the same code.
 *
 * On X11 there is no layer shell to make the window unbypassable: the surfaces
 * are ordinary fullscreen windows.  The lock is still enforced by whelk, but a
 * window manager that lets another window above it is outside this process's
 * control.  The README says so instead of pretending otherwise.
 */

/* Prepares the locker and starts watching logind's Lock signal.  Idempotent. */
void whelk_locker_init(GtkApplication *app);

/* Locks every output.  Does nothing when already locked or before init. */
void whelk_locker_lock(void);

/* TRUE from the moment the overlays are up until PAM accepts a password. */
gboolean whelk_locker_is_locked(void);

/* Destroys the locker, drops the logind subscription and joins a pending
 * authentication. */
void whelk_locker_shutdown(void);

G_END_DECLS

#endif // WHELK_LOCKER_H

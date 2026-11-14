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
 * Locking is not a panel that can be clicked away.  On a Wayland compositor
 * that implements ext-session-lock-v1, whelk asks the compositor for a session
 * lock (through gtk4-layer-shell's GtkSessionLockInstance): the compositor
 * owns the lock surfaces, no other client can place a window above them, and
 * the session stays locked if this process dies.  The only input that reaches
 * the session is a password, and the only thing that removes the lock is a
 * successful PAM authentication for the user who owns the shell.
 *
 * A compositor without ext-session-lock-v1 (and X11) gets the older user-space
 * fallback: one overlay layer surface per output with the keyboard taken
 * exclusively.  That path is a compatibility path, not the hardened one; the
 * README states its limits plainly.
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
 * On X11 there is no layer shell to make the window unbypassable: the fallback
 * surfaces are ordinary fullscreen windows.  The lock is still enforced by
 * whelk, but a window manager that lets another window above it is outside
 * this process's control.  The README says so instead of pretending
 * otherwise.
 */

/* Prepares the locker and starts watching logind's Lock signal.  Idempotent. */
void whelk_locker_init(GtkApplication *app);

/* Locks every output.  Does nothing when already locked or before init. */
void whelk_locker_lock(void);

/* TRUE from the moment the lock is requested until PAM accepts a password. */
gboolean whelk_locker_is_locked(void);

/* Destroys the locker, drops the logind subscription and joins a pending
 * authentication. */
void whelk_locker_shutdown(void);

G_END_DECLS

#endif // WHELK_LOCKER_H

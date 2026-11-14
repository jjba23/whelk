// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LOCKPOLICY_H
#define WHELK_LOCKPOLICY_H

#include <glib.h>

G_BEGIN_DECLS

#define WHELK_LOCK_MAX_DELAY_MS 30000

/**
 * whelk_lock_retry_delay_ms:
 * @failures: consecutive failed unlocks, 0 before the first attempt.
 *
 * A wrong password costs one second; each further failure doubles the wait,
 * capped at half a minute.  The delay is enforced by the locker, not by the
 * authentication service: an attacker cannot make the next guess faster by
 * restarting the shell, but neither can a mistyped password lock the owner out
 * for good.
 *
 * Returns: the delay before another attempt may be made.
 */
guint whelk_lock_retry_delay_ms(guint failures);

/**
 * whelk_lock_submission_allowed:
 * @authenticating: a PAM call is already in flight
 * @retry_id: the retry timer is active (non-zero)
 * @entry_sensitive: the password entry is accepting input
 *
 * The UI can deliver an Unlock activation from the button even while the
 * entry is disabled.  Keep the authoritative guard in one testable place.
 *
 * Returns: %TRUE when a password may be submitted now.
 */
gboolean whelk_lock_submission_allowed(gboolean authenticating, guint retry_id,
                                       gboolean entry_sensitive);

G_END_DECLS

#endif // WHELK_LOCKPOLICY_H

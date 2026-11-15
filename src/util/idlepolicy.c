// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/idlepolicy.h"

int whelk_idle_lock_delay(int lock_after, gboolean idle, gboolean inhibited,
                          gint64 idle_for) {
  if (!idle || inhibited || lock_after < 0)
    return WHELK_IDLE_NO_TIMER;

  /* Zero is the compatible policy: the old shell locked on the first IdleHint,
   * and a user who has not configured a delay still gets exactly that. */
  if (lock_after == 0)
    return 0;

  if (idle_for < 0)
    idle_for = 0;

  if (idle_for >= lock_after)
    return 0;

  return (int)(lock_after - idle_for);
}

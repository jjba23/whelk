// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/lockpolicy.h"

guint whelk_lock_retry_delay_ms(guint failures) {
  if (failures == 0)
    return 0;

  /* 1, 2, 4, 8, 16, 30 seconds.  Shifting a guint by more than its width is
   * undefined, so the exponent is clamped before the shift. */
  const guint exponent = MIN(failures - 1, 5);
  const guint delay = 1000u << exponent;

  return MIN(delay, WHELK_LOCK_MAX_DELAY_MS);
}

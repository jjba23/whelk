// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/level.h"

gboolean whelk_level_update(WhelkLevel *level, int percent, gboolean muted) {
  if (!level)
    return FALSE;

  if (!level->known) {
    level->percent = percent;
    level->muted = muted;
    level->known = TRUE;

    return FALSE;
  }

  if (level->percent == percent && level->muted == muted)
    return FALSE;

  level->percent = percent;
  level->muted = muted;

  return TRUE;
}

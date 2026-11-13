// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LEVEL_H
#define WHELK_LEVEL_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * A READING THAT CAN CHANGE
 * =========================================================================
 * The volume and the backlight are polled, so the module sees the same number
 * over and over; only a move should raise the on-screen display, and the first
 * reading a module ever gets is not a move -- nobody pressed anything.
 *
 * The rule is kept here, apart from the bar and the surface, so it can be
 * tested without a display or a mixer.
 */

typedef struct {
  int percent;
  gboolean muted;
  gboolean known;
} WhelkLevel;

/**
 * whelk_level_update:
 * @level: the last reading.
 * @percent: the reading just taken.
 * @muted: whether the sink/panel is muted.
 *
 * Records @percent and @muted, and returns TRUE when that is a change from a
 * reading this level already had.  The first call only fills it in.
 */
gboolean whelk_level_update(WhelkLevel *level, int percent, gboolean muted);

G_END_DECLS

#endif // WHELK_LEVEL_H

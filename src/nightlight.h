// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NIGHTLIGHT_CONTROLLER_H
#define WHELK_NIGHTLIGHT_CONTROLLER_H

#include "util/nightlight.h"

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE NIGHT LIGHT
 * =========================================================================
 * One object owns the filter for the whole shell: it chooses the display
 * server backend once, computes the strength the clock asks for, and sends the
 * gamma table only when the colour actually moves.  A reload hands it a new
 * configuration; the shell frees it on exit, which restores the table the
 * display had before whelk touched it.
 *
 * The temperature never exceeds 6500 K: a "night light" that cools the screen
 * further than neutral is a different feature with a different name.
 */

typedef struct _WhelkNightLight WhelkNightLight;

/* Chooses the backend from the session and returns an object even when no
 * backend is available, so the rest of the shell can hold one unconditionally.
 * The unsupported case is logged once. */
WhelkNightLight *whelk_night_light_new(void);

/* Replaces the configuration, applies it now and schedules the next change. */
void whelk_night_light_set_config(WhelkNightLight *night,
                                  const WhelkNightLightConfig *config);

/* Closes the display-server connection.  The backend destroys its gamma
 * controls, which restores the table each output had before whelk touched
 * it. */
void whelk_night_light_free(WhelkNightLight *night);

G_END_DECLS

#endif // WHELK_NIGHTLIGHT_CONTROLLER_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_RIVERSTATUS_H
#define WHELK_RIVERSTATUS_H

#include <glib.h>

/* =========================================================================
 * RIVER'S FOCUSED VIEW
 * =========================================================================
 * riverctl has no query for the focused view.  River publishes it on the
 * river-status protocol instead: one zriver_seat_status_v1 per seat, whose
 * focused_view event carries the title.  whelk opens a second Wayland
 * connection for this (the GTK one belongs to GDK), watches its fd through
 * the GLib main loop, and hands each title to the callback.
 *
 * The protocol publishes the title, not the application id; a river window
 * therefore reads its name from the title and leaves `{app-id}' empty.  The
 * status object is owned by its caller and must outlive the callback.
 */

typedef struct _WhelkRiverStatus WhelkRiverStatus;

/* Called with the focused view's title (possibly empty) on every change, and
 * once when the seat status is bound.  The string is owned by the status. */
typedef void (*WhelkRiverViewFunc)(const char *title, gpointer data);

/* Returns NULL when there is no Wayland connection or the fd cannot be
 * watched; a compositor that does not advertise river-status leaves the title
 * NULL and the module empty rather than guessing. */
WhelkRiverStatus *whelk_river_status_new(WhelkRiverViewFunc callback,
                                        gpointer data);
void whelk_river_status_free(WhelkRiverStatus *status);

/* The last title, or NULL before the first event. */
const char *whelk_river_status_title(WhelkRiverStatus *status);

#endif // WHELK_RIVERSTATUS_H

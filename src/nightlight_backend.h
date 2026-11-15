// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NIGHTLIGHT_BACKEND_H
#define WHELK_NIGHTLIGHT_BACKEND_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE DISPLAY-SERVER HALF
 * =========================================================================
 * A backend owns the connection to the display server, the gamma table of
 * every output, and the events that say an output appeared, disappeared,
 * changed mode or lost its gamma control.  The controller above it only says
 * "these are the three channel multipliers"; each backend builds the ramp at
 * the size its server wants and keeps it for outputs that arrive later.
 *
 * The two implementations are the two native answers:
 *
 *   - src/nightlight_wayland.c, wlr-gamma-control-unstable-v1, implemented by
 *     the wlroots compositors and by niri, Hyprland and river;
 *   - src/nightlight_x11.c, the RandR CRTC gamma table, which is how an X11
 *     server has exposed hardware gamma since RandR 1.2.
 *
 * Neither draws an overlay.  A tinted layer surface would cover the bar and
 * every window but would leave gamma untouched, hide HDR and video from the
 * calibration the user asked for, and cost a full-screen surface per output.
 */

typedef struct {
  const char *name;
  /* Applies RGB multipliers to every output; a neutral colour restores the
   * table each output had before the backend touched it.  Returns TRUE when
   * the backend accepted them; FALSE when its display connection has gone
   * away. */
  gboolean (*set_color)(gpointer data, const double rgb[3]);
  /* Closes the connection and restores the table each output had before this
   * backend touched it. */
  void (*destroy)(gpointer data);
  gpointer data;
} WhelkNightLightBackend;

/* Returns NULL when the running compositor does not advertise
 * wlr-gamma-control-unstable-v1.  That is a fact about the compositor, not an
 * error: the controller reports it once instead of pretending an overlay is
 * the same thing. */
WhelkNightLightBackend *whelk_night_light_wayland_backend_new(void);

/* Returns NULL when DISPLAY is absent, when the X server has no RandR
 * extension, or when the build was made without libXrandr. */
WhelkNightLightBackend *whelk_night_light_x11_backend_new(void);

void whelk_night_light_backend_free(WhelkNightLightBackend *backend);

G_END_DECLS

#endif // WHELK_NIGHTLIGHT_BACKEND_H

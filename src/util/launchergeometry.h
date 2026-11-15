// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LAUNCHER_GEOMETRY_H
#define WHELK_LAUNCHER_GEOMETRY_H

#include <glib.h>

G_BEGIN_DECLS

/* The launcher's small geometry decisions, separated from its widgets so they
 * can be tested without a display:
 *
 * - the scroll value that brings a selected row into view,
 * - whether a click landed outside the card (and should close the overlay),
 * - the bounds of the window, icon and font zoom.
 */

double whelk_launcher_scroll_value(double current, double page, double lower,
                                   double upper, double row_top,
                                   double row_bottom);

gboolean whelk_launcher_point_outside(double x, double y, double left,
                                      double top, double right, double bottom);

int whelk_launcher_clamp_width(int width);
int whelk_launcher_clamp_height(int height);
int whelk_launcher_clamp_icon_size(int size);
int whelk_launcher_clamp_font_scale(int percent);

G_END_DECLS

#endif // WHELK_LAUNCHER_GEOMETRY_H

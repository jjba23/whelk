// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/launchergeometry.h"

double whelk_launcher_scroll_value(double current, double page, double lower,
                                   double upper, double row_top,
                                   double row_bottom) {
  double value = current;
  const double maximum = MAX(lower, upper - page);

  if (row_top < value)
    value = row_top;
  else if (row_bottom > value + page)
    value = row_bottom - page;

  return CLAMP(value, lower, maximum);
}

gboolean whelk_launcher_point_outside(double x, double y, double left,
                                      double top, double right, double bottom) {
  return x < left || x >= right || y < top || y >= bottom;
}

/* The card can be dragged bigger than a laptop screen if nothing stops it, and
 * a 2px icon or 20% text is not a usable launcher.  These are the same bounds
 * the settings window's spin buttons use. */
int whelk_launcher_clamp_width(int width) {
  return CLAMP(width, 360, 1600);
}

int whelk_launcher_clamp_height(int height) {
  return CLAMP(height, 300, 1200);
}

int whelk_launcher_clamp_icon_size(int size) {
  return CLAMP(size, 16, 64);
}

int whelk_launcher_clamp_font_scale(int percent) {
  return CLAMP(percent, 75, 200);
}

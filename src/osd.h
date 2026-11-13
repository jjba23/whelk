// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_OSD_H
#define WHELK_OSD_H

#include <gtk/gtk.h>

#include "config.h"

G_BEGIN_DECLS

/* =========================================================================
 * THE ON-SCREEN DISPLAY
 * =========================================================================
 * macOS answers a volume or brightness key with a small HUD that appears,
 * shows the new level and goes away on its own.  This is that HUD: one overlay
 * surface, one icon, one bar, a timeout.  It is the shell's, not a module's, so
 * two bars do not draw two of them and a module only has to say "this changed".
 */

#define WHELK_TYPE_OSD (whelk_osd_get_type())
G_DECLARE_FINAL_TYPE(WhelkOsd, whelk_osd, WHELK, OSD, GObject)

WhelkOsd *whelk_osd_new(GtkApplication *app, const WhelkBarConfig *style);

/**
 * whelk_osd_show:
 * @osd: the display.
 * @icon: the glyph to show (a Font Awesome character, like the bar's).
 * @percent: the level, 0 to 150; the bar fills to 100 and the number says the
 *   rest.
 *
 * Shows the HUD and restarts its timer.  Called only when a level changed, by
 * the module that reads it.
 */
void whelk_osd_show(WhelkOsd *osd, const char *icon, int percent);

void whelk_osd_hide(WhelkOsd *osd);

G_END_DECLS

#endif // WHELK_OSD_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WIDGETS_H
#define WHELK_WIDGETS_H

#include "config.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * DESKTOP WIDGETS
 * =========================================================================
 * A widget is a floating layer surface with its own card, refresh source and
 * optional network request.  The settings file describes it with a
 * <widget-config>; this runtime turns that description into the GTK4 cards a
 * desktop wears.  The manager is rebuilt with the bars, so moving a widget or
 * changing its options is a settings save away.
 */

typedef struct _WhelkWidgets WhelkWidgets;

WhelkWidgets *whelk_widgets_new(GtkApplication *app, const char *config_path);

/* Replaces every widget with CONFIGS, drawing them in STYLE's palette.  A
 * dragged position saved for CONFIG_PATH is applied over the configured one. */
void whelk_widgets_build(WhelkWidgets *widgets, const GPtrArray *configs,
                         const WhelkBarConfig *style);

guint whelk_widgets_count(WhelkWidgets *widgets);

void whelk_widgets_free(WhelkWidgets *widgets);

G_END_DECLS

#endif // WHELK_WIDGETS_H

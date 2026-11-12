// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_BAR_H
#define WHELK_BAR_H

#include "config.h"
#include "surface.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE BAR
 * =========================================================================
 * A bar is a surface with three module rows in it, one CSS provider for its
 * skin, and the modules themselves.  It owns a copy of its configuration, so a
 * reload can free the configuration array while the bar that is still on screen
 * keeps its own numbers.
 */

#define WHELK_TYPE_BAR (whelk_bar_get_type())
G_DECLARE_FINAL_TYPE(WhelkBar, whelk_bar, WHELK, BAR, GObject)

/**
 * whelk_bar_new:
 * @app: the application the bar's window belongs to.
 * @config: the bar's description; copied, so the caller may free it.
 * @monitor: (nullable): the monitor to pin to, or NULL for the default one.
 *
 * Returns: (transfer full): a presented bar.
 */
WhelkBar *whelk_bar_new(GtkApplication *app, const WhelkBarConfig *config,
                        GdkMonitor *monitor);

GtkWidget *whelk_bar_widget(WhelkBar *bar);

const char *whelk_bar_name(WhelkBar *bar);

G_END_DECLS

#endif // WHELK_BAR_H

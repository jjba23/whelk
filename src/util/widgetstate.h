// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WIDGETSTATE_H
#define WHELK_WIDGETSTATE_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * DESKTOP WIDGET STATE
 * =========================================================================
 * Where a widget was dragged to, and the small pieces of a widget that change
 * without a settings save (today: which reminders are done).  This is runtime
 * state, not configuration: it lives under XDG_STATE_HOME, one file per
 * settings file, and deleting it returns every widget to the place its
 * settings file gave it.  A settings reload never has to rewrite the file the
 * user versions.
 *
 * The file is a GKeyFile, so a line that cannot be parsed costs one value and
 * not the rest of the desktop.
 */

#define WHELK_WIDGET_GRID 8

/**
 * whelk_widget_state_path:
 * @config_path: the settings file the widgets belong to.
 *
 * Returns: (transfer full) (nullable): the state file for that settings file,
 * or NULL when there is no settings path to key it by.
 */
char *whelk_widget_state_path(const char *config_path);

/* Round VALUE to the nearest multiple of GRID, never below zero.  A grid of
 * zero or less means no snapping. */
int whelk_widget_snap(int value, int grid);

/* Keep a WIDTH x HEIGHT widget inside an AREA_WIDTH x AREA_HEIGHT monitor.
 * A widget larger than the monitor is pinned to the top-left corner rather
 * than given a negative position. */
void whelk_widget_clamp(int *x, int *y, int width, int height,
                        int area_width, int area_height);

/* Read or write a widget's saved top-left corner.  A missing file, group or
 * value is not an error: the widget keeps the position its settings file
 * gave it. */
gboolean whelk_widget_state_load_position(const char *path, const char *id,
                                          int *x, int *y);
gboolean whelk_widget_state_save_position(const char *path, const char *id,
                                          int x, int y);

/* Read or write one string of per-widget state.  Passing NULL as VALUE
 * removes the key.  The returned string is owned by the caller. */
char *whelk_widget_state_get(const char *path, const char *id,
                             const char *key);
gboolean whelk_widget_state_set(const char *path, const char *id,
                                const char *key, const char *value);

G_END_DECLS

#endif // WHELK_WIDGETSTATE_H

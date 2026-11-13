// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_PREFERENCES_H
#define WHELK_PREFERENCES_H

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * GUI PREFERENCES
 * =========================================================================
 * The settings window edits a focused set of bar options and persists each
 * change through (whelk prefs).  It never writes the primary settings file:
 * the saved values are a state-directory override that wins when the shell
 * reloads, and Reset deletes that file.
 *
 * RELOAD is called after the window has written one or more values.  The shell
 * passes its reload function; a test can pass NULL and load the settings
 * itself, which keeps the widget test out of the whole shell runtime.
 */

typedef void (*WhelkPreferencesReloadFunc)(gpointer user_data);

/* Opens the window for CONFIG_PATH, or presents the one that is already open. */
void whelk_preferences_show(GtkApplication *app, const char *config_path,
                            WhelkPreferencesReloadFunc reload,
                            gpointer reload_data);

/* The window's contents without a GtkWindow, for the widget test.  The caller
 * owns the returned widget. */
GtkWidget *whelk_preferences_content_for(const char *config_path,
                                         WhelkPreferencesReloadFunc reload,
                                         gpointer reload_data);

/* Destroys any window this module owns; called when the shell shuts down. */
void whelk_preferences_shutdown(void);

G_END_DECLS

#endif // WHELK_PREFERENCES_H

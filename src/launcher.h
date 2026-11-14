// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LAUNCHER_H
#define WHELK_LAUNCHER_H

#include "config.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE LAUNCHER
 * =========================================================================
 * The shell's launcher: one overlay, one entry, one list, and the answer to "what
 * did I mean" in every form a desktop needs it.
 *
 *   app     applications, from every applications/ directory on the system
 *   window  the open windows niri knows about
 *   run     a command, or a program found on PATH
 *   calc    an arithmetic expression, copied to the clipboard
 *   file    any file or directory under $HOME, caches and repositories
 *           skipped, the query ranked by name and by path
 *   web     the default browser, with the query
 *   menu    any program that prints "label<TAB>command" lines
 *   ssh     the hosts ~/.ssh/config names, opened in a terminal
 *   quit    the open windows, closed rather than focused (a Force Quit menu)
 *   clipboard  what was copied, newest first; Enter puts it back
 *   menuitem   every entry of the focused application's live menubar, with its
 *              path and chord (macOS's Help menu search)
 *
 * The mode is chosen by --launcher-mode, by the shell's own keybinding, or by a
 * prefix typed into the entry (> run, = calc, / file, ? web, @ window, % ssh,
 * ! close, ; clipboard, : menu items).  What every mode shares is the matching:
 * whelk_fuzzy_score() ranks
 * a prefix above a buried match, and Tab completes the selection into the
 * entry.
 */

#define WHELK_TYPE_LAUNCHER (whelk_launcher_get_type())
G_DECLARE_FINAL_TYPE(WhelkLauncher, whelk_launcher, WHELK, LAUNCHER, GObject)

/**
 * whelk_launcher_new:
 * @app: the application the overlay belongs to.
 * @style: (nullable): the bar whose palette the overlay wears.
 *
 * Returns: (transfer full): the launcher, presented and hidden.
 */
WhelkLauncher *whelk_launcher_new(GtkApplication *app,
                                  const WhelkBarConfig *style);

/* Replaces the palette and material without rebuilding the overlay.  Called
 * when a theme change reloads the shell: the launcher keeps its history and
 * widgets, and only its skin changes. */
void whelk_launcher_set_style(WhelkLauncher *self,
                              const WhelkBarConfig *style);

/**
 * whelk_launcher_show:
 * @self: the launcher.
 * @mode: (nullable): "app" (the default), "window", "run", "calc", "file",
 *   "web", "menu", "ssh", "quit", "clipboard" or "menuitem".
 *
 * Shows the overlay with an empty query, and focuses the entry.
 */
void whelk_launcher_show(WhelkLauncher *self, const char *mode);

/* Shows the launcher in its generic menu mode, listing @command's output. */
void whelk_launcher_show_menu(WhelkLauncher *self, const char *command);

/* Shows the shell's own session menu: lock, log out, suspend, hibernate,
 * restart and shut down.  It is built in, so no external menu program is a
 * dependency of a power button. */
void whelk_launcher_show_session_menu(WhelkLauncher *self);

void whelk_launcher_toggle(WhelkLauncher *self, const char *mode);

void whelk_launcher_hide(WhelkLauncher *self);

gboolean whelk_launcher_is_visible(WhelkLauncher *self);

G_END_DECLS

#endif // WHELK_LAUNCHER_H

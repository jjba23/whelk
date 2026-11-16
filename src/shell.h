// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_SHELL_H
#define WHELK_SHELL_H

#include "clipboard.h"
#include "notifications.h"
#include "osd.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE SHELL RUNTIME
 * =========================================================================
 * Everything that is true of the whole shell rather than of one surface: which
 * settings file is in charge, which bars exist, when they are rebuilt, and what
 * happens when the configuration or the monitors change.
 *
 * Rebuilding is deliberately total: a reload destroys the bars and builds them
 * again from the new description.  A running bar holds no state that outlives
 * its configuration (a module's reading is re-read, its CSS is re-generated),
 * so there is nothing to reconcile and no half-updated bar to reason about.
 */

typedef struct _WhelkShell WhelkShell;

WhelkShell *whelk_shell_new(GtkApplication *app);

/* Reads the configuration, builds the bars and starts watching for changes. */
void whelk_shell_start(WhelkShell *shell);

/* Re-reads the settings file and rebuilds every bar. */
void whelk_shell_reload(WhelkShell *shell);

/* Locks every output.  Also reached by logind's Lock signal and `whelk
 * --lock`; a second call while locked does nothing. */
void whelk_shell_lock(WhelkShell *shell);

/* Opens the GUI settings window for the loaded configuration. */
void whelk_shell_show_settings(WhelkShell *shell);

void whelk_shell_free(WhelkShell *shell);

/* =========================================================================
 * THE SHELL'S SURFACES
 * =========================================================================
 * The bar is one surface among several.  These are the entry points a module,
 * a keybinding or the command line uses to reach the others -- the shape of the
 * API a bar module has is "ask the shell", never "build your own window".
 */

/* The running shell, or NULL before/when there is none.  A module's click
 * handler is the caller that cannot be handed the pointer. */
WhelkShell *whelk_shell_get_default(void);

/* The notification service, so a module can listen to it. */
WhelkNotifications *whelk_shell_notifications(WhelkShell *shell);

/* The clipboard history: the launcher's clipboard mode reads it, and a click
 * that copies something puts it back. */
WhelkClipboard *whelk_shell_clipboard(WhelkShell *shell);

/* Shows the launcher in @mode; see src/launcher.h for the modes. */
void whelk_shell_show_launcher(WhelkShell *shell, const char *mode);

/* Shows the launcher in menu mode, listing @command's "label<TAB>command"
 * lines. */
void whelk_shell_show_menu(WhelkShell *shell, const char *command);

/* Shows the built-in session menu (lock, log out, suspend, hibernate,
 * restart, shut down) without a script or a menu program. */
void whelk_shell_show_session_menu(WhelkShell *shell);

/* Opens or closes the notification centre. */
void whelk_shell_toggle_notifications(WhelkShell *shell);

/* The wallpaper picker: a card in the middle of the output, expandable to the
 * whole screen.  `whelk --wallpaper-picker' is its keybinding surface. */
void whelk_shell_show_wallpaper_picker(WhelkShell *shell);

/* Shows PATH as the wallpaper and saves the choice as a GUI preference.
 * Returns FALSE when the file is missing or not a picture whelk can read. */
gboolean whelk_shell_set_wallpaper(WhelkShell *shell, const char *path);

/* Turns do-not-disturb on or off.  The idle manager owns the flag. */
void whelk_shell_toggle_dnd(WhelkShell *shell);

/* Flips the night light's saved enable switch and reloads the shell: the
 * keybinding's equivalent of the Night light page's switch.  Returns 1 when
 * on, 0 when off, or -1 when the override could not be saved. */
int whelk_shell_toggle_night_light(WhelkShell *shell);

/* Takes or releases whelk's presentation inhibitor: a real logind idle lock
 * held by this process.  REASON is shown by `systemd-inhibit --list'. */
void whelk_shell_toggle_inhibit(WhelkShell *shell, const char *reason);

/* The idle manager's report: hint, timer, whelk's inhibitor and DND.  The
 * caller frees the string. */
char *whelk_shell_idle_status(WhelkShell *shell);

/* Shows the network panel on @page ("wifi" or "bluetooth"). */
void whelk_shell_show_network_panel(WhelkShell *shell, const char *page);

/* The volume and brightness HUD.  A module shows it when its reading changes;
 * there is one per shell, so two bars do not draw two. */
WhelkOsd *whelk_shell_osd(WhelkShell *shell);

G_END_DECLS

#endif // WHELK_SHELL_H

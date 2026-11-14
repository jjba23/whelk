// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_GUILE_BRIDGE_H
#define WHELK_GUILE_BRIDGE_H

#include "config.h"
#include "util/chord.h"
#include "util/menudict.h"

#include <glib.h>

/* =========================================================================
 * THE GUILE BRIDGE
 * =========================================================================
 * The only place C reads the Scheme configuration.  Every call here is a plain
 * C signature: no SCM appears in this header, and no scm_* call lives outside
 * guile_bridge.c, so the rest of the shell can be read (and tested) without
 * knowing that its configuration engine is a Scheme interpreter.
 *
 * Contract: the bridge never hands out a live Scheme value.  It converts to
 * plain structs and strings, both owned by the caller, so a bar's description
 * cannot be invalidated by a garbage collection or by the next reload.
 */

/**
 * whelk_bridge_start_guile:
 * Brings the interpreter up for a process that is not the shell.
 *
 * The shell hands Guile the whole process through `scm_boot_guile'; a test
 * harness is a plain C program that wants to read the Scheme modules, so it
 * asks for the interpreter here instead.  Idempotent, and never needed by
 * main.c.
 */
void whelk_bridge_start_guile(void);

/**
 * whelk_bridge_setup_load_path:
 * Adds this checkout's `scheme' directory and the installation's Guile site
 * directory to Guile's %load-path, so `(use-modules (whelk core))` resolves
 * whether whelk runs from the build tree or from /usr.
 */
void whelk_bridge_setup_load_path(void);

/**
 * whelk_bridge_use_core:
 * Loads (whelk core) into the interpreter.
 *
 * Returns: TRUE when the module was found.  A shell without its Scheme modules
 * still runs on the C defaults, so this is a warning and not a failure.
 */
gboolean whelk_bridge_use_core(void);

/**
 * whelk_bridge_config_path:
 * Returns: (transfer full): $XDG_CONFIG_HOME/whelk/settings.scm.
 */
char *whelk_bridge_config_path(void);

/* Defines the `whelk-widget-*' primitives a `scheme-module' procedure uses to
 * build the GTK widget it returns.  Idempotent; called by
 * whelk_bridge_use_core() and by a harness that boots Guile itself. */
void whelk_bridge_register_widget_primitives(void);

/**
 * whelk_bridge_call_module_widget:
 * @token: a `scheme-module' update token, from the module spec's `update'
 *   option.
 *
 * Calls the registered procedure on the current thread and takes the widget it
 * returns.  A plain string or a `(label "...")'-shaped description is accepted
 * as well as a widget made by a `whelk-widget-*' primitive.
 *
 * Returns: (transfer full) (nullable): a widget the caller owns, or NULL when
 *   the procedure raised, returned nothing, or its token belongs to a settings
 *   file that is no longer loaded.  GTK calls must be on the main thread.
 */
GtkWidget *whelk_bridge_call_module_widget(const char *token);

/* Calls the Scheme entry point that reads the settings file and the fallback
 * bar: `init-shell!'.  Returns TRUE when the Scheme side ran. */
gboolean whelk_bridge_init(void);

/* Calls `load-shell-config!' on @path. */
gboolean whelk_bridge_load(const char *path);

/* Calls `start-repl!' for @path (it is idempotent per socket). */
void whelk_bridge_start_repl(const char *path);

/* Calls `stop-repl!': removes the Unix socket files the shell is serving. */
void whelk_bridge_stop_repl(void);

/**
 * whelk_bridge_bars:
 * @path: the settings file the bars belong to (may be NULL).
 *
 * Returns: (transfer full) (element-type WhelkBarConfig): the configured bars,
 * or an empty array when the Scheme side has none.
 */
GPtrArray *whelk_bridge_bars(const char *path);

/**
 * whelk_bridge_widgets:
 * @path: the settings file the widgets belong to (may be NULL).
 *
 * Reads `(shell-widgets->alists)' from `(whelk state)': the floating desktop
 * widgets a configuration declared, with their placement and own options.  An
 * empty or missing configuration is an empty array, not a failure: desktop
 * widgets are opt-in.
 *
 * Returns: (transfer full) (element-type WhelkWidgetConfig): the widgets.
 */
GPtrArray *whelk_bridge_widgets(const char *path);

/**
 * whelk_bridge_clipboard:
 * @path: the settings file whose clipboard policy is wanted (may be NULL).
 *
 * Reads `(clipboard-settings)': persistence mode, whether persistence is a
 * pinned-only vault, the memory caps, and the retention time.  An empty or
 * missing configuration answers a config with @configured FALSE, which tells
 * the shell to keep honouring WHELK_CLIPBOARD_PERSIST.
 *
 * Returns: (transfer full): the resolved policy; never NULL.
 */
WhelkClipboardConfig *whelk_bridge_clipboard(const char *path);

/**
 * whelk_bridge_menudict:
 *
 * The global menu's dictionary, from `(whelk menudict)': the menus written for
 * the applications that publish none.  A module that cannot be loaded is a
 * warning and an empty table, never an abort -- the shell then offers the
 * shared Edit and View and says so in the log.
 *
 * Returns: (transfer full) (element-type WhelkDictApp): the table, ready for
 *   whelk_menudict_load().
 */
GPtrArray *whelk_bridge_menudict(void);

/**
 * whelk_bridge_orbit_menubar:
 * @app_id: (nullable): the focused window's application id.
 * @shortcut_tool: (nullable): the program that types a chord.
 * @style: how the shortcut is shown.
 * @out_name: (out) (optional) (transfer full): the corpus's name for the
 *   matching application, or NULL when nothing matched.
 *
 * Reads `(whelk orbit)' -- the Orbit Global Menu shortcut corpus converted to
 * Scheme -- and builds the same menu tree the other producers use.  The
 * corpus is GPL-3.0-or-later data, kept in its own module with its own
 * licence header; this bridge is the MIT runtime that reads it.
 *
 * Returns: (transfer full) (element-type WhelkMenu): the menus, or an empty
 *   array when the application is unknown, the corpus is absent, or no
 *   shortcut tool is installed.
 */
GPtrArray *whelk_bridge_orbit_menubar(const char *app_id,
                                      const char *shortcut_tool,
                                      WhelkChordStyle style, char **out_name);

/**
 * whelk_bridge_shell_menu:
 * @path: the settings file whose shell menu is wanted (may be NULL).
 *
 * The shell's own menu -- the one behind the mark at the far left of the bar --
 * as `(whelk menu)' units: `(item label action)', `(separator)' and
 * `(submenu label unit ...)'.  A settings file that configured one is read; one
 * that did not gets `default-shell-menu' from `(whelk defaults)', so the menu is
 * never empty and never a C table somebody has to recompile to change.
 *
 * Returns: (transfer full) (element-type WhelkMenu): a menubar holding one
 *   menu -- the popover's contents -- or an empty array when Scheme never came
 *   up.
 */
GPtrArray *whelk_bridge_shell_menu(const char *path);

/**
 * whelk_bridge_themes:
 *
 * The theme names from `(whelk themes)', in the order the settings window
 * should offer them.
 *
 * Returns: (transfer full) (element-type utf8): the names; never NULL.
 */
GPtrArray *whelk_bridge_themes(void);

/* Save one GUI preference.  The typed C value becomes the Scheme value the
 * preference file stores; the primary settings file is never touched. */
gboolean whelk_bridge_save_preference_bool(const char *path, const char *key,
                                           gboolean value);
gboolean whelk_bridge_save_preference_int(const char *path, const char *key,
                                          int value);
gboolean whelk_bridge_save_preference_string(const char *path, const char *key,
                                             const char *value);
gboolean whelk_bridge_save_preference_symbol(const char *path, const char *key,
                                             const char *value);

/* Delete every GUI preference for @path; the settings file takes over again. */
gboolean whelk_bridge_reset_preferences(const char *path);

#endif // WHELK_GUILE_BRIDGE_H

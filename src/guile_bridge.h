// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_GUILE_BRIDGE_H
#define WHELK_GUILE_BRIDGE_H

#include "config.h"

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

/* Calls the Scheme entry point that reads the settings file and the fallback
 * bar: `init-shell!'.  Returns TRUE when the Scheme side ran. */
gboolean whelk_bridge_init(void);

/* Calls `load-shell-config!' on @path. */
gboolean whelk_bridge_load(const char *path);

/* Calls `start-repl!' for @path (it is idempotent per socket). */
void whelk_bridge_start_repl(const char *path);

/**
 * whelk_bridge_bars:
 * @path: the settings file the bars belong to (may be NULL).
 *
 * Returns: (transfer full) (element-type WhelkBarConfig): the configured bars,
 * or an empty array when the Scheme side has none.
 */
GPtrArray *whelk_bridge_bars(const char *path);

#endif // WHELK_GUILE_BRIDGE_H

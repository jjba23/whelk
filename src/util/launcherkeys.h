// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_LAUNCHERKEYS_H
#define WHELK_LAUNCHERKEYS_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE LAUNCHER'S OWN KEYS
 * =========================================================================
 * whelk does not own the session's keyboard: a compositor binding is what
 * opens the launcher.  Once it is open, however, the launcher reads one key of
 * its own -- the prefix that chooses a mode, `@' for windows, `>' for a
 * command, and so on -- and that map is whelk's to change.
 *
 * The settings window edits this map and the mode `whelk --launcher' opens
 * with; both are GUI overrides in the state file, and both are read here as
 * plain data so the mapping can be tested without a display, a compositor or
 * an open launcher.
 */

/**
 * WhelkLauncherKeys:
 *
 * The mode prefixes and the default mode, as the settings window last saved
 * them.  Build with whelk_launcher_keys_new(); every other function tolerates
 * NULL and falls back to the built-in map.
 */
typedef struct _WhelkLauncherKeys WhelkLauncherKeys;

/**
 * whelk_launcher_keys_new:
 *
 * Returns: (transfer full): the built-in map: `@' window, `>' run, `=' calc,
 *   `/` file, `?' web, `%' ssh, `!' quit, `;' clipboard, `:' menu item, and
 *   `app' as the mode `whelk --launcher' opens with.
 */
WhelkLauncherKeys *whelk_launcher_keys_new(void);

void whelk_launcher_keys_free(WhelkLauncherKeys *keys);

/**
 * whelk_launcher_keys_is_mode:
 * @mode: a mode name.
 *
 * Returns: whether @mode is one of the launcher's modes.  An unknown mode in
 *   the override file is ignored rather than opening an empty overlay.
 */
gboolean whelk_launcher_keys_is_mode(const char *mode);

/* The modes in the order the settings window lists them. */
guint whelk_launcher_keys_mode_count(void);
const char *whelk_launcher_keys_mode_at(guint index);

/**
 * whelk_launcher_keys_set_default_mode:
 * @keys: the map.
 * @mode: the mode `whelk --launcher' should open with; unknown modes are
 *   ignored and `app' is kept.
 */
void whelk_launcher_keys_set_default_mode(WhelkLauncherKeys *keys,
                                         const char *mode);

const char *whelk_launcher_keys_default_mode(const WhelkLauncherKeys *keys);

/**
 * whelk_launcher_keys_set_prefix:
 * @keys: the map.
 * @mode: the mode the prefix chooses.
 * @prefix: the text; an empty string (or NULL) disables the prefix.
 *
 * A prefix may be longer than one character, and when two prefixes overlap the
 * longer one wins.
 */
void whelk_launcher_keys_set_prefix(WhelkLauncherKeys *keys, const char *mode,
                                    const char *prefix);

const char *whelk_launcher_keys_prefix(const WhelkLauncherKeys *keys,
                                       const char *mode);

/**
 * whelk_launcher_keys_mode_for_text:
 * @keys: the map (NULL for the built-in one).
 * @text: what is typed in the launcher's entry.
 * @query: (out) (optional): the text after the prefix.
 *
 * Returns: (nullable): the mode a prefix selects, or NULL when @text starts
 *   with no enabled prefix.  The caller keeps @text.
 */
const char *whelk_launcher_keys_mode_for_text(const WhelkLauncherKeys *keys,
                                              const char *text,
                                              const char **query);

G_END_DECLS

#endif // WHELK_LAUNCHERKEYS_H

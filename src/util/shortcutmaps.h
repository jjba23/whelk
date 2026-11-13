// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_SHORTCUTMAPS_H
#define WHELK_SHORTCUTMAPS_H

#include <glib.h>

#include "util/chord.h"
#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * SHORTCUT MAPS FROM ORBIT GLOBAL MENU
 * =========================================================================
 * Orbit Global Menu ships a large, reviewed corpus of menu specifications for
 * applications that publish no menubar.  Its files are JSON: menus, nested
 * submenus, labels and GTK accelerator strings.  This reader consumes that
 * format from a user-installed directory, so whelk can use the corpus without
 * copying its GPL-3.0 data into this MIT project.
 *
 * The importer is a fallback, not a replacement for whelk's own Scheme
 * dictionary: the dictionary is consulted first, and this one only answers
 * for an application it does not know.  The standard locations are
 * `$XDG_CONFIG_HOME/global-menu/shortcuts/apps` and
 * `$XDG_DATA_HOME/global-menu/shortcuts/apps`, with
 * `WHELK_SHORTCUT_MAPS` naming one directory or file to use instead.
 *
 * Every shortcut becomes a command typed by the configured tool; an item
 * without a readable shortcut stays visible but disabled, which is the honest
 * representation of an action that needs a menu of its own or a mouse.
 */

/**
 * whelk_shortcutmaps_menubar_for_app:
 * @app_id: (nullable): the focused window's application id.
 * @shortcut_tool: (nullable): the program that types a chord, or NULL when
 *   no such program is installed.
 * @style: how the chord is shown.
 *
 * Reads the first user-installed mapping whose `identifiers` match @app_id,
 * and turns it into the same tree the other global-menu producers use.  No
 * tool means no menus: an entry that cannot be typed is worse than absent.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu; empty when there is no
 *   matching mapping, no shortcut tool, or the mapping is blank.
 */
GPtrArray *whelk_shortcutmaps_menubar_for_app(const char *app_id,
                                              const char *shortcut_tool,
                                              WhelkChordStyle style);

/**
 * whelk_shortcutmaps_reset:
 * Forgets the mapping index.  The next lookup rescans the directories; tests
 * use this after pointing WHELK_SHORTCUT_MAPS at a fixture.
 */
void whelk_shortcutmaps_reset(void);

G_END_DECLS

#endif // WHELK_SHORTCUTMAPS_H

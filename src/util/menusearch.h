// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUSEARCH_H
#define WHELK_MENUSEARCH_H

#include <glib.h>

#include "util/menuaction.h"
#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * THE FOCUSED APPLICATION'S MENUS, FLATTENED
 * =========================================================================
 * The bar knows the focused application's menubar -- whichever producer made
 * it, GTK, DBusMenu, a script or the dictionary.  This is that same tree,
 * flattened into one row per entry with its path ("View > Appearance > Dark")
 * and its chord, kept where the launcher can search it: a menu bar's answer to
 * the Help menu's search field.
 *
 * Publishing copies everything: the module may rebuild its menus on the next
 * focus change while a search is open, and a row that outlives its tree still
 * has to activate what it names.
 */

typedef struct {
  char *path;  /* "File > Export > PDF" */
  char *label; /* "PDF" */
  char *accel; /* "Ctrl+Shift+E", or NULL */
  WhelkMenuEntry *entry; /* owned copy, the activation payload */
  WhelkMenuActionContext context;
} WhelkMenuSearchItem;

/**
 * whelk_menu_search_publish:
 * @app_name: (nullable): what the application calls itself.
 * @context: (nullable): the bus name, object path and desktop file the entries
 *   belong to.
 * @menus: (nullable): a GPtrArray of WhelkMenu, borrowed for the call.
 *
 * Replaces the searchable snapshot.  Disabled entries and rules are left out:
 * a search result that cannot be chosen is not a result.
 */
void whelk_menu_search_publish(const char *app_name,
                               const WhelkMenuActionContext *context,
                               GPtrArray *menus);

void whelk_menu_search_clear(void);

/* The current snapshot, borrowed.  Empty (never NULL) when no menu is up. */
GPtrArray *whelk_menu_search_snapshot(void);

/* The application name for the mode's heading, borrowed; "" when unknown. */
const char *whelk_menu_search_app_name(void);

/* A deep copy, for a row that must survive the next publish. */
WhelkMenuSearchItem *whelk_menu_search_item_copy(const WhelkMenuSearchItem *item);
void whelk_menu_search_item_free(WhelkMenuSearchItem *item);

/* Activates the entry the way the bar's click does. */
void whelk_menu_search_item_activate(const WhelkMenuSearchItem *item);

G_END_DECLS

#endif // WHELK_MENUSEARCH_H

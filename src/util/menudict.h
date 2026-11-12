// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUDICT_H
#define WHELK_MENUDICT_H

#include <glib.h>

/* =========================================================================
 * THE MENU DICTIONARY
 * =========================================================================
 * Most applications publish no menubar.  A Wayland shell cannot ask a GTK
 * application for its menus -- that is the compositor's `gtk-shell' protocol,
 * which niri and sway do not implement -- so the shell's global menu needs a
 * second source: menus written from each application's documented keyboard
 * shortcuts, typed into the focused window when an entry is chosen.
 *
 * The table below is that source.  It is data, not logic: an application is a
 * list of identifiers it answers to, and a list of menus whose items name a
 * label, the chord on its right, and the chord to type.  Adding an application
 * is one entry; there is nothing else to change.
 *
 * Nothing here is a guess about what an application's own menu says.  Every
 * chord is the one its documentation gives, and an application with no entry
 * gets the generic Edit and View instead of an invented File menu.
 */

/* One entry: what the menu says, what it shows on the right, and what it types. */
typedef struct {
  const char *label;
  const char *accel;     /* "Ctrl+N", shown dim on the right */
  const char *modifiers; /* "ctrl", "ctrl shift", NULL for none */
  const char *key;       /* "n", "F11" */
} WhelkDictItem;

typedef struct {
  const char *label;
  const WhelkDictItem *items;
  guint n_items;
} WhelkDictMenu;

typedef struct {
  const char *name; /* for the log: "Firefox" */
  /* App ids, desktop ids, or the names compositors report; matched with or
   * without their dotted prefix, and case-insensitively. */
  const char *const *ids;
  const WhelkDictMenu *menus;
  guint n_menus;
} WhelkDictApp;

/**
 * whelk_menudict_for_app:
 * @app_id: the focused window's application id, as the session reports it.
 *
 * Finds the curated menus for @app_id.  A bare name matches the dotted id it
 * belongs to ("firefox" for "org.mozilla.firefox") and the other way round, so
 * a native Wayland id and an X11 WM_CLASS both find the same table.
 *
 * Returns: (transfer none) (nullable): the application's menus, or NULL when
 *   the dictionary has nothing for it.
 */
const WhelkDictApp *whelk_menudict_for_app(const char *app_id);

/* How many applications the dictionary knows, for the log and the docs. */
guint whelk_menudict_size(void);

/**
 * whelk_menudict_at:
 * @index: 0 to whelk_menudict_size() - 1.
 *
 * The table in order.  The shell itself looks applications up by id; this is
 * how the suite checks that every entry is complete.
 *
 * Returns: (transfer none) (nullable): the application at @index.
 */
const WhelkDictApp *whelk_menudict_at(guint index);

#endif // WHELK_MENUDICT_H

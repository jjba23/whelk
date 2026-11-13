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
 * The dictionary itself is Scheme, in `(whelk menudict)': it is configuration
 * data, and the configuration language of this shell is Scheme.  This file is
 * the runtime half -- it holds the table the bridge loaded at startup, and
 * answers which application a compositor's app id belongs to.  Nothing here
 * knows an application; adding one is one form in the Scheme module.
 */

/* One entry: what the menu says, what it shows on the right, and what it types.
 *
 * A chord may be an Emacs sequence (C-x C-s), so the typed half is an array:
 * modifiers[i] is the modifiers held for keys[i], and a NULL modifier is a
 * bare key.  One chord is the usual case; the arrays have n_chords entries. */
typedef struct {
  const char *label;
  const char *accel;     /* "Ctrl+X Ctrl+S", shown dim on the right */
  char **modifiers;      /* n_chords entries, each "ctrl" or NULL */
  char **keys;           /* n_chords entries, each "x", "F11" */
  guint n_chords;
} WhelkDictItem;

typedef struct {
  const char *label;
  WhelkDictItem *items;
  guint n_items;
} WhelkDictMenu;

typedef struct {
  const char *name; /* for the log: "Firefox" */
  /* App ids, desktop ids, or the names compositors report; matched with or
   * without their dotted prefix, and case-insensitively.  NULL-terminated. */
  char **ids;
  WhelkDictMenu *menus;
  guint n_menus;
} WhelkDictApp;

/* Builders, for the Guile bridge and for tests.  The table owns every string
 * and array it is given: hand over copies, and never free them again. */
WhelkDictApp *whelk_dict_app_new(const char *name, char **ids,
                                 WhelkDictMenu *menus, guint n_menus);
WhelkDictMenu *whelk_dict_menus_new(guint n_menus);
WhelkDictItem *whelk_dict_items_new(guint n_items);

/**
 * whelk_menudict_table_new:
 *
 * Returns: (transfer full) (element-type WhelkDictApp): an empty table with the
 *   right free function, ready for the bridge to fill.
 */
GPtrArray *whelk_menudict_table_new(void);

/**
 * whelk_menudict_load:
 * @table: (transfer full) (nullable): the dictionary, or NULL to clear it.
 *
 * Replaces the dictionary with @table, taking ownership.  Called once, at
 * startup, with what the Scheme module described.
 */
void whelk_menudict_load(GPtrArray *table);

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

/**
 * whelk_menudict_ids_match:
 * @app_id: what a compositor reports.
 * @id: one identifier an application answers to.
 *
 * The same comparison the dictionary uses: case-insensitive, tolerant of a
 * trailing `.desktop`, and accepting a dotted suffix in either direction.
 * Other menu corpora use the same rules for their identifiers.
 */
gboolean whelk_menudict_ids_match(const char *app_id, const char *id);

/* How many applications the dictionary knows, for the log and the docs. */
guint whelk_menudict_size(void);

/**
 * whelk_menudict_at:
 * @index: 0 to whelk_menudict_size() - 1.
 *
 * The table in order.  The shell itself looks applications up by id; this is
 * how the suite walks every entry.
 *
 * Returns: (transfer none) (nullable): the application at @index.
 */
const WhelkDictApp *whelk_menudict_at(guint index);

#endif // WHELK_MENUDICT_H

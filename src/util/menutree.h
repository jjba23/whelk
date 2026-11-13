// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUTREE_H
#define WHELK_MENUTREE_H

#include <gio/gio.h>
#include <glib.h>

#include "util/chord.h"

/* =========================================================================
 * THE GLOBAL MENU'S TREE
 * =========================================================================
 * A menubar is a list of menus (File, Edit, View), each a list of entries,
 * each either a command or a submenu.  Two producers fill it: an application
 * that exports its menubar over D-Bus (`org.gtk.Menus`, which is what GTK's
 * `gtk_application_set_menubar()` publishes), and the shell itself, which
 * synthesizes the same shape for applications that export nothing.
 *
 * The tree is plain data so the parsing and the synthesizing are testable
 * without a display, a bus or a running application.
 */

typedef enum {
  WHELK_MENU_ENTRY,
  WHELK_MENU_SEPARATOR,
} WhelkMenuKind;

typedef struct WhelkMenu WhelkMenu;

typedef struct {
  WhelkMenuKind kind;
  char *label;
  /* The chord this entry types, e.g. "Ctrl+N": shown dim on the right of the
   * label, and NULL for an entry that is not a shortcut. */
  char *accel;
  gboolean enabled;
  /* One of these says what activating the entry does. */
  char *action;  /* an org.gtk.Actions name, "app.save" or "win.copy" */
  char *target;  /* the action's string parameter, when it has one */
  char *command; /* a shell command line */
  WhelkMenu *submenu;
} WhelkMenuEntry;

struct WhelkMenu {
  char *label;
  GPtrArray *entries; /* of WhelkMenuEntry */
};

WhelkMenu *whelk_menu_new(const char *label);
void whelk_menu_free(WhelkMenu *menu);

WhelkMenuEntry *whelk_menu_add_entry(WhelkMenu *menu, const char *label);
WhelkMenuEntry *whelk_menu_add_separator(WhelkMenu *menu);
WhelkMenu *whelk_menu_add_submenu(WhelkMenu *menu, const char *label);

/* A GPtrArray of WhelkMenu, freeing its menus; the menubar's own type. */
GPtrArray *whelk_menubar_new(void);

/**
 * whelk_menubar_from_gtk:
 * @reply: the `a(uuaa{sv})` answer to `org.gtk.Menus.Start`, or NULL.
 * @group: the group of the menubar's root menu.
 * @menu: the menu id of the menubar's root.
 * @style: how a chord GTK exports (its `accel' attribute) is shown.
 *
 * Turns an exported menubar into menus: `:section` links are inlined (a
 * section's items belong to the menu that holds the link), `:submenu` links
 * become submenus, separators sit between sections.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu; empty when nothing could
 *   be read.
 */
GPtrArray *whelk_menubar_from_gtk(GVariant *reply, guint group, guint menu,
                                 WhelkChordStyle style);

/**
 * whelk_menubar_for_app:
 * @app_id: (nullable): the focused window's application id.
 * @action_ids: (array length=action_count): the application's desktop actions.
 * @action_labels: (array length=action_count): their labels.
 * @action_count: how many there are.
 * @close_command: (nullable): the command that closes the focused window on
 *   this session; NULL leaves the entry out.
 * @shortcut_tool: (nullable): the program that types keys, "wtype" on Wayland
 *   and "xdotool" on X11; NULL when neither is installed, which leaves the
 *   menus to the entries that do not need one.
 * @style: how every chord is written in the menus: CUA ("Ctrl+V") or Emacs
 *   ("C-v").  The dictionary may store either notation; this is the display.
 * @out_source: (out caller-allocates) (optional): what the menus were made
 *   from, for the log: the dictionary's name for the application, or NULL
 *   when the generic menus were used.
 *
 * The menubar an application that exports nothing gets.  File always carries
 * the actions its desktop entry declares and a Close Window; then:
 *
 * - an application the dictionary knows gets its own menus -- the documented
 *   shortcuts of that application, each entry showing its chord and typing it;
 * - everything else gets Edit and View built from the shortcuts every
 *   application shares.
 *
 * Nothing pretends to know an application's own menu: a menu entry appears
 * only if something can carry it out.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu.
 */
GPtrArray *whelk_menubar_for_app(const char *app_id,
                                 const char *const *action_ids,
                                 const char *const *action_labels,
                                 guint action_count,
                                 const char *close_command,
                                 const char *shortcut_tool,
                                 WhelkChordStyle style,
                                 const char **out_source);

/* The command that types a chord, "wtype -M ctrl -k z -m ctrl". */
char *whelk_shortcut_command(const char *tool, const char *modifiers,
                             const char *key);

#endif // WHELK_MENUTREE_H

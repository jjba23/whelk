// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUTREE_H
#define WHELK_MENUTREE_H

#include <gio/gio.h>
#include <glib.h>

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
 *
 * Turns an exported menubar into menus: `:section` links are inlined (a
 * section's items belong to the menu that holds the link), `:submenu` links
 * become submenus, separators sit between sections.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu; empty when nothing could
 *   be read.
 */
GPtrArray *whelk_menubar_from_gtk(GVariant *reply, guint group, guint menu);

/**
 * whelk_menubar_synthesize:
 * @action_ids: (array length=action_count): the application's desktop actions.
 * @action_labels: (array length=action_count): their labels.
 * @action_count: how many there are.
 * @shortcut_tool: (nullable): the program that types keys, "wtype" when it is
 *   installed; NULL when it is not, which leaves the synthesized menus to the
 *   entries that do not need one.
 *
 * The menubar an application without a menubar gets: File with its desktop
 * actions and a Close Window, Edit and View built from the shortcuts every
 * application shares.  Nothing pretends to know an application's own menu.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu.
 */
GPtrArray *whelk_menubar_synthesize(const char *const *action_ids,
                                    const char *const *action_labels,
                                    guint action_count,
                                    const char *shortcut_tool);

/* The command that types a chord, "wtype -M ctrl -k z -m ctrl". */
char *whelk_shortcut_command(const char *tool, const char *modifiers,
                             const char *key);

#endif // WHELK_MENUTREE_H

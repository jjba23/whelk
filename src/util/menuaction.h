// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUACTION_H
#define WHELK_MENUACTION_H

#include <glib.h>

#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * CHOOSING A MENU ENTRY
 * =========================================================================
 * An entry is one of four things: a shell command, a DBusMenu item id, a
 * desktop-entry action, or an org.gtk.Actions name.  The first two carry
 * everything they need; the last two belong to the application that published
 * them, so activating one needs the bus name, object path and desktop file the
 * entry came from.  That is the context, and passing it explicitly is what lets
 * the bar and the launcher's menu search share one activation path.
 */

typedef struct {
  char *bus_name;     /* the application's bus name, "org.example.App" */
  char *app_path;     /* its object path, "/org/example/App" */
  char *desktop_path; /* its .desktop file, for "desktop:" actions */
} WhelkMenuActionContext;

void whelk_menu_action_context_set(WhelkMenuActionContext *context,
                                   const char *bus_name, const char *app_path,
                                   const char *desktop_path);
void whelk_menu_action_context_clear(WhelkMenuActionContext *context);

/* "win.copy" -> "copy"; "app.new-window" -> "new-window". */
const char *whelk_menu_action_name(const char *action);

/* The object path an action is addressed to: "win." goes to the focused
 * window, everything else to the application. */
char *whelk_menu_action_object_path(const char *action, const char *app_path);

/* Does what clicking the entry in the bar does. */
void whelk_menu_entry_activate(const WhelkMenuEntry *entry,
                               const WhelkMenuActionContext *context);

G_END_DECLS

#endif // WHELK_MENUACTION_H

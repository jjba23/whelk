// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_ATSPIMENUS_H
#define WHELK_ATSPIMENUS_H

#include <gio/gio.h>
#include <glib.h>

#include "util/chord.h"
#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * THE ACCESSIBILITY BUS
 * =========================================================================
 * AT-SPI is the one menu source that is not a menu protocol.  GTK, Qt,
 * Electron and everything else that implements accessibility expose their
 * menus as ordinary accessible objects: a MENU_BAR whose children are MENU
 * objects, whose children are MENU_ITEMs with names, states, key bindings and
 * an Action interface.  Those names are the application's own labels, already
 * translated into the session language, which is exactly what the hand-written
 * dictionary cannot promise.
 *
 * The bus is slow and the tree is large, so the read runs on a worker thread
 * and the bar asks for it only after the protocol producers (org.gtk.Menus and
 * DBusMenu) have found nothing.  That order matters: a menu is stale the moment
 * the application rebuilds it, and a protocol that has a change signal is
 * better than a tree that must be re-walked.
 *
 * Known limits, stated rather than hidden: some applications (Firefox is the
 * usual example) do not populate a top-level menu until it has been opened
 * once; this producer reads the passive tree and does not pop native menus
 * open.  Such an application falls back to the dictionary, which is still the
 * behaviour it had before AT-SPI.
 */

typedef struct _WhelkAtspiMenuRequest WhelkAtspiMenuRequest;

/**
 * WhelkAtspiMenuFunc:
 * @menus: (transfer full): the menubar; empty when the application publishes
 *   no menu bar on the accessibility bus.
 * @user_data: the pointer given to whelk_atspi_menubar_async().
 *
 * Called once on the main thread, unless the request was freed first.
 */
typedef void (*WhelkAtspiMenuFunc)(GPtrArray *menus, gpointer user_data);

/**
 * whelk_atspi_menubar_async:
 * @app_name: (nullable): the focused application's short name, as the desktop
 *   entry and AT-SPI spell it.
 * @pid: the focused application's process id, or 0 when the session cannot
 *   report one.  A pid is the reliable match; the name is the fallback.
 * @style: how every key binding is written.
 * @done: called with the tree.
 * @user_data: passed to @done.
 *
 * Reads the accessible tree on a worker thread.  The accessibility bus address
 * comes from org.a11y.Bus on the session bus; every call has a short timeout
 * and a bounded tree walk, so a hung or enormous application costs the bar a
 * background task rather than a frame.
 *
 * Returns: (transfer full): the request; free it to cancel.
 */
WhelkAtspiMenuRequest *whelk_atspi_menubar_async(const char *app_name, gint pid,
                                                 WhelkChordStyle style,
                                                 WhelkAtspiMenuFunc done,
                                                 gpointer user_data);

/* Cancels the read.  A callback that has not run yet will not run. */
void whelk_atspi_menubar_request_free(WhelkAtspiMenuRequest *request);

/**
 * whelk_menubar_from_atspi_connection:
 * @bus: an accessibility-bus connection.
 * @app_name: (nullable): the application to match by name.
 * @pid: the application to match by pid, or 0.
 * @style: how every key binding is written.
 *
 * The synchronous core, exposed for tests and for callers that already have a
 * bus.  It starts from `org.a11y.atspi.Registry` at the root accessible and
 * walks until it finds a MENU_BAR.
 *
 * Returns: (transfer full): a GPtrArray of WhelkMenu; never NULL.
 */
GPtrArray *whelk_menubar_from_atspi_connection(GDBusConnection *bus,
                                               const char *app_name, gint pid,
                                               WhelkChordStyle style);

/**
 * whelk_atspi_activate:
 * @service: the accessible's bus name on the accessibility bus.
 * @path: its object path.
 * @action: the action index (0 is the default action).
 *
 * Sends `org.a11y.atspi.Action.DoAction`.  A menu entry is activated the way a
 * screen reader activates it, not by typing a shortcut; that works even when
 * the application has no keyboard accelerator for the entry.
 *
 * Returns: TRUE when the bus accepted the call.
 */
gboolean whelk_atspi_activate(const char *service, const char *path,
                              int action);

G_END_DECLS

#endif // WHELK_ATSPIMENUS_H

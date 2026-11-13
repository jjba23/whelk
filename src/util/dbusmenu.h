// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_DBUSMENU_H
#define WHELK_DBUSMENU_H

#include <gio/gio.h>
#include <glib.h>

#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * A MENU THAT CHANGES WHILE IT IS OPEN
 * =========================================================================
 * `GetLayout' is a snapshot, and a snapshot of a live menu goes stale: Firefox
 * rebuilds its File menu as windows open, Thunderbird's View menu changes with
 * the folder, and every id in an older tree belongs to a layout that no longer
 * exists.  Sending `Event(id, "clicked")' with one of those ids is not a
 * harmless no-op -- it may fire whatever now holds that id.
 *
 * This watch follows `LayoutUpdated' and `ItemsPropertiesUpdated' on the
 * object that published the menu, debounces their bursts, and asks for the
 * whole tree again.  Every answer replaces the previous one, ids included, and
 * the module closes what is open before adopting it.
 */

typedef struct _WhelkDBusMenuWatch WhelkDBusMenuWatch;

/**
 * WhelkDBusMenuChangedFunc:
 * @menus: (transfer full): the fresh tree, a GPtrArray of WhelkMenu.
 * @user_data: the pointer given to whelk_dbusmenu_watch_new().
 */
typedef void (*WhelkDBusMenuChangedFunc)(GPtrArray *menus, gpointer user_data);

/**
 * WhelkDBusMenuInvalidatedFunc:
 * @user_data: the pointer given to whelk_dbusmenu_watch_new().
 *
 * Called the moment a `LayoutUpdated' arrives, before the new tree is read:
 * the ids the caller is holding may already be gone, so anything drawn from
 * them closes now rather than after the round trip.
 */
typedef void (*WhelkDBusMenuInvalidatedFunc)(gpointer user_data);

/**
 * whelk_dbusmenu_watch_new:
 * @bus: the session bus.
 * @service: the bus name that publishes the menu.
 * @path: the object path the menu answers on.
 * @style: how the new tree's chords are written.
 * @changed: (nullable): called with every refreshed tree.
 * @invalidated: (nullable): called when the published layout changes, before
 *   the re-read; what is open should go away then, not later.
 * @user_data: passed to @changed and @invalidated.
 *
 * Watches @service at @path.  The first tree is the caller's (the module reads
 * it once when it adopts the producer); this watch starts with the next
 * change.
 *
 * Returns: (transfer full): the watch; free it with
 *   whelk_dbusmenu_watch_free().
 */
WhelkDBusMenuWatch *whelk_dbusmenu_watch_new(
    GDBusConnection *bus, const char *service, const char *path,
    WhelkChordStyle style, WhelkDBusMenuChangedFunc changed,
    WhelkDBusMenuInvalidatedFunc invalidated, gpointer user_data);

/**
 * whelk_dbusmenu_watch_free:
 * @watch: (nullable): the watch.
 *
 * Stops listening and cancels any read in flight.  A reply that was already on
 * its way finds the watch cancelled and calls nobody.
 */
void whelk_dbusmenu_watch_free(WhelkDBusMenuWatch *watch);

G_END_DECLS

#endif // WHELK_DBUSMENU_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_GTKMENU_H
#define WHELK_GTKMENU_H

#include <gio/gio.h>
#include <glib.h>

#include "util/menutree.h"

G_BEGIN_DECLS

/* =========================================================================
 * A GTK MENU THAT CHANGES WHILE IT IS OPEN
 * =========================================================================
 * org.gtk.Menus does have a Changed signal: an array of
 * (group, menu, position, removed, added) tuples.  A client keeps the
 * subscription it made with Start, and the application emits the signal only
 * for subscribed groups.  The shell does not need to apply the diff itself --
 * its tree is read-only and has already been rendered -- so it closes the open
 * popover and asks Start for the whole current tree again.
 *
 * This is the same shape as the DBusMenu watch: one signal subscription, a
 * short debounce for a burst of menu changes, and a fresh tree in place of the
 * previous one.
 */

typedef struct _WhelkGtkMenuWatch WhelkGtkMenuWatch;

typedef void (*WhelkGtkMenuChangedFunc)(GPtrArray *menus, gpointer user_data);
typedef void (*WhelkGtkMenuInvalidatedFunc)(gpointer user_data);

/**
 * whelk_gtkmenu_watch_new:
 * @bus: the session bus.
 * @service: the application's bus name.
 * @path: the menu object path that answered Start.
 * @style: how the refreshed tree's chords are written.
 * @changed: (nullable) (transfer full): called with every fresh tree, empty
 *   included when the application removed its menus; the callback owns the
 *   array.
 * @invalidated: (nullable): called when Changed arrives, before the re-read.
 * @user_data: passed to the callbacks.
 *
 * Subscribes to Changed before calling Start, so a change in between is not
 * missed, then requests subscription group 0.  The first tree is handed to
 * @changed as well; the caller may keep the tree it already read or replace it.
 *
 * Returns: (transfer full): the watch.
 */
WhelkGtkMenuWatch *whelk_gtkmenu_watch_new(
    GDBusConnection *bus, const char *service, const char *path,
    WhelkChordStyle style, WhelkGtkMenuChangedFunc changed,
    WhelkGtkMenuInvalidatedFunc invalidated, gpointer user_data);

/**
 * whelk_gtkmenu_watch_free:
 * @watch: (nullable): the watch.
 *
 * Unsubscribes the signal and calls End for the subscription group.  A Start
 * already in flight is cancelled and calls nobody.
 */
void whelk_gtkmenu_watch_free(WhelkGtkMenuWatch *watch);

G_END_DECLS

#endif // WHELK_GTKMENU_H

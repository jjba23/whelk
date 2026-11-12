// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NOTIFICATION_CENTER_H
#define WHELK_NOTIFICATION_CENTER_H

#include "config.h"
#include "notifications.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE NOTIFICATION CENTRE
 * =========================================================================
 * Two surfaces, one idea: a notification is not an interruption, it is a thing
 * that happened, and the shell should be able to show it twice.
 *
 *   - the toast stack, top right, three at most, each leaving on its own;
 *   - the centre, a panel on the right edge, grouping what is still there by
 *     application, with the actions the application asked for.
 *
 * Both read the same store (src/notifications.c), so dismissing a card in one
 * removes it from the other, and "clear all" means all of it.
 */

#define WHELK_TYPE_NOTIFICATION_CENTER (whelk_notification_center_get_type())
G_DECLARE_FINAL_TYPE(WhelkNotificationCenter, whelk_notification_center, WHELK,
                     NOTIFICATION_CENTER, GObject)

/**
 * whelk_notification_center_new:
 * @app: the application the surfaces belong to.
 * @notifications: the service to render (borrowed for the centre's lifetime).
 * @style: (nullable): the bar whose palette the surfaces wear.
 *
 * Returns: (transfer full): both surfaces, presented and hidden.
 */
WhelkNotificationCenter *
whelk_notification_center_new(GtkApplication *app,
                              WhelkNotifications *notifications,
                              const WhelkBarConfig *style);

void whelk_notification_center_show(WhelkNotificationCenter *self);
void whelk_notification_center_hide(WhelkNotificationCenter *self);
void whelk_notification_center_toggle(WhelkNotificationCenter *self);

gboolean whelk_notification_center_is_visible(WhelkNotificationCenter *self);

G_END_DECLS

#endif // WHELK_NOTIFICATION_CENTER_H

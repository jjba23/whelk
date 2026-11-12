// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NOTIFICATIONS_H
#define WHELK_NOTIFICATIONS_H

#include <glib-object.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE NOTIFICATION SERVICE
 * =========================================================================
 * Whelk owns org.freedesktop.Notifications.  A notification daemon is the one
 * piece of a desktop that every program on it talks to, and hosting it in the
 * shell is what makes notifications *ours*: the same process that draws the bar
 * decides how a notification is grouped, when it interrupts, and what a click
 * on it does.
 *
 * The service keeps the history (a notification stays in the centre after its
 * toast has gone, the way a desktop's notification list does), and exposes it
 * as an oldest-first snapshot so the centre can group it.  Rendering is
 * src/notification_center.c's business; this file is the protocol and the store.
 *
 * When another daemon (mako, dunst, swaync) already owns the name, whelk says so
 * once and leaves the name to it: two daemons on one name is a race the user
 * loses.
 */

#define WHELK_TYPE_NOTIFICATIONS (whelk_notifications_get_type())
G_DECLARE_FINAL_TYPE(WhelkNotifications, whelk_notifications, WHELK,
                     NOTIFICATIONS, GObject)

typedef struct _WhelkNotification WhelkNotification;

/** The server's D-Bus name, also this service's only name. */
#define WHELK_NOTIFICATIONS_BUS_NAME "org.freedesktop.Notifications"
#define WHELK_NOTIFICATIONS_OBJECT_PATH "/org/freedesktop/Notifications"

/**
 * whelk_notifications_new:
 *
 * Returns: (transfer full): the service.  Nothing is exported until
 * whelk_notifications_start().
 */
WhelkNotifications *whelk_notifications_new(void);

/**
 * whelk_notifications_start:
 * @self: the service.
 * @error: (out) (optional): why the name could not be requested.
 *
 * Asks for org.freedesktop.Notifications asynchronously; the service is live
 * once the bus reports the name as acquired.
 *
 * Returns: FALSE when the session bus itself is unavailable.
 */
gboolean whelk_notifications_start(WhelkNotifications *self, GError **error);

/* TRUE once this process owns the notification name. */
gboolean whelk_notifications_is_owner(WhelkNotifications *self);

/**
 * whelk_notifications_snapshot:
 * @self: the service.
 *
 * Returns: (transfer none) (element-type WhelkNotification): the stored
 * notifications, oldest first.  Borrowed: use it before the next main-loop
 * iteration, and never free its members.
 */
GPtrArray *whelk_notifications_snapshot(WhelkNotifications *self);

/* How many notifications are stored. */
guint whelk_notifications_count(WhelkNotifications *self);

/* --- do not disturb -------------------------------------------------------- */

gboolean whelk_notifications_has_dnd(WhelkNotifications *self);

/* With DND on, notifications are still stored and still counted; they simply
 * do not raise a toast.  That is what "do not disturb" means. */
void whelk_notifications_set_dnd(WhelkNotifications *self, gboolean dnd);

/* --- dismissal ------------------------------------------------------------- */

/* Drops a notification the user dismissed (NotificationClosed reason 2). */
void whelk_notifications_close(WhelkNotifications *self, guint id);

/* Drops a notification whose toast timed out (reason 1). */
void whelk_notifications_expire(WhelkNotifications *self, guint id);

/* Drops all of them. */
void whelk_notifications_close_all(WhelkNotifications *self);

/**
 * whelk_notifications_invoke:
 * @self: the service.
 * @notification: one from the snapshot.
 * @action: (nullable): the action id to invoke, or NULL for the default action.
 *
 * Emits ActionInvoked for the notification and closes it, which is what an
 * interactive notification does when it is clicked.  An app listening for its
 * own action is what makes "Open" in a notification mean something.
 */
void whelk_notifications_invoke(WhelkNotifications *self,
                                WhelkNotification *notification,
                                const char *action);

/* --- a notification's reading ---------------------------------------------- */

guint whelk_notification_id(WhelkNotification *notification);
const char *whelk_notification_app(WhelkNotification *notification);
const char *whelk_notification_summary(WhelkNotification *notification);
const char *whelk_notification_body(WhelkNotification *notification);
const char *whelk_notification_icon(WhelkNotification *notification);
const char *whelk_notification_category(WhelkNotification *notification);
int whelk_notification_urgency(WhelkNotification *notification);
gboolean whelk_notification_is_transient(WhelkNotification *notification);
gint64 whelk_notification_timestamp(WhelkNotification *notification);

/* How long its toast should stay, in milliseconds; 0 means "until dismissed". */
int whelk_notification_timeout(WhelkNotification *notification);

/* "14:05", for the centre's cards. */
char *whelk_notification_time(WhelkNotification *notification);

/* Number of actions, and one action's label ("" for a default action). */
guint whelk_notification_action_count(WhelkNotification *notification);
const char *whelk_notification_action_label(WhelkNotification *notification,
                                            guint index);
const char *whelk_notification_action_id(WhelkNotification *notification,
                                         guint index);

G_END_DECLS

#endif // WHELK_NOTIFICATIONS_H

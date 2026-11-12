// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "notifications.h"

#include <gio/gio.h>
#include <string.h>

#include "logger.h"

/* =========================================================================
 * THE PROTOCOL
 * =========================================================================
 * The Desktop Notifications specification, as much of it as a shell needs to be
 * a good citizen: Notify, CloseNotification, GetCapabilities,
 * GetServerInformation, and the two signals an application waits for.
 */

static const char notification_xml[] =
    "<node>"
    "  <interface name='org.freedesktop.Notifications'>"
    "    <method name='Notify'>"
    "      <arg type='s' name='app_name' direction='in'/>"
    "      <arg type='u' name='replaces_id' direction='in'/>"
    "      <arg type='s' name='app_icon' direction='in'/>"
    "      <arg type='s' name='summary' direction='in'/>"
    "      <arg type='s' name='body' direction='in'/>"
    "      <arg type='as' name='actions' direction='in'/>"
    "      <arg type='a{sv}' name='hints' direction='in'/>"
    "      <arg type='i' name='expire_timeout' direction='in'/>"
    "      <arg type='u' name='id' direction='out'/>"
    "    </method>"
    "    <method name='CloseNotification'>"
    "      <arg type='u' name='id' direction='in'/>"
    "    </method>"
    "    <method name='GetCapabilities'>"
    "      <arg type='as' name='capabilities' direction='out'/>"
    "    </method>"
    "    <method name='GetServerInformation'>"
    "      <arg type='s' name='name' direction='out'/>"
    "      <arg type='s' name='vendor' direction='out'/>"
    "      <arg type='s' name='version' direction='out'/>"
    "      <arg type='s' name='spec_version' direction='out'/>"
    "    </method>"
    "    <signal name='NotificationClosed'>"
    "      <arg type='u' name='id'/>"
    "      <arg type='u' name='reason'/>"
    "    </signal>"
    "    <signal name='ActionInvoked'>"
    "      <arg type='u' name='id'/>"
    "      <arg type='s' name='action_key'/>"
    "    </signal>"
    "  </interface>"
    "</node>";

/* How long a toast stays when the application does not say (-1). */
#define WHELK_NOTIFICATION_DEFAULT_TIMEOUT 6000

/* Reasons, from the specification. */
#define WHELK_CLOSED_EXPIRED 1
#define WHELK_CLOSED_DISMISSED 2
#define WHELK_CLOSED_CALL 3

typedef struct {
  char *id;
  char *label; /* "" for the default action */
} NotificationAction;

struct _WhelkNotification {
  guint id;
  char *app_name;
  char *app_icon;
  char *summary;
  char *body;
  char *desktop_entry;
  char *image_path;
  char *category;
  int urgency;
  gboolean transient;
  gint64 timestamp;
  gint expire_timeout;
  GPtrArray *actions; /* NotificationAction* */
};

struct _WhelkNotifications {
  GObject parent_instance;

  GPtrArray *notifications; /* WhelkNotification*, oldest first */
  guint next_id;
  gboolean dnd;
  gboolean owner;
  guint registration_id;
  guint owner_id;
  GDBusConnection *connection;
};

G_DEFINE_TYPE(WhelkNotifications, whelk_notifications, G_TYPE_OBJECT)

enum {
  SIGNAL_CHANGED,
  SIGNAL_POSTED,
  SIGNAL_CLOSED,
  SIGNAL_DND_CHANGED,
  SIGNAL_LAST
};

static guint signals[SIGNAL_LAST] = {0};

/* =========================================================================
 * A NOTIFICATION
 * ========================================================================= */

static NotificationAction *action_new(const char *id, const char *label) {
  NotificationAction *action = g_new0(NotificationAction, 1);

  action->id = g_strdup(id ? id : "");
  action->label = g_strdup(label ? label : "");

  return action;
}

static void action_free(gpointer data) {
  NotificationAction *action = data;

  if (!action)
    return;

  g_free(action->id);
  g_free(action->label);
  g_free(action);
}

static WhelkNotification *notification_new(void) {
  WhelkNotification *notification = g_new0(WhelkNotification, 1);

  notification->actions =
      g_ptr_array_new_with_free_func((GDestroyNotify)action_free);
  notification->urgency = 1;
  notification->expire_timeout = WHELK_NOTIFICATION_DEFAULT_TIMEOUT;
  notification->timestamp = g_get_real_time();

  return notification;
}

static void notification_free(gpointer data) {
  WhelkNotification *notification = data;

  if (!notification)
    return;

  g_free(notification->app_name);
  g_free(notification->app_icon);
  g_free(notification->summary);
  g_free(notification->body);
  g_free(notification->desktop_entry);
  g_free(notification->image_path);
  g_free(notification->category);
  g_clear_pointer(&notification->actions, g_ptr_array_unref);
  g_free(notification);
}

#define COPY_FIELD(field)                                                      \
  do {                                                                         \
    g_free(target->field);                                                     \
    target->field = g_strdup(source->field);                                   \
  } while (0)

/* Replaces the contents of @target with @source's, keeping @target's id: what
 * "replaces_id" asks for. */
static void notification_replace(WhelkNotification *target,
                                 WhelkNotification *source) {
  COPY_FIELD(app_name);
  COPY_FIELD(app_icon);
  COPY_FIELD(summary);
  COPY_FIELD(body);
  COPY_FIELD(desktop_entry);
  COPY_FIELD(image_path);
  COPY_FIELD(category);

  target->urgency = source->urgency;
  target->transient = source->transient;
  target->expire_timeout = source->expire_timeout;
  target->timestamp = source->timestamp;

  g_clear_pointer(&target->actions, g_ptr_array_unref);
  target->actions = source->actions;
  source->actions = NULL;
}

static WhelkNotification *find_notification(WhelkNotifications *self,
                                            guint id) {
  for (guint i = 0; i < self->notifications->len; i++) {
    WhelkNotification *notification =
        g_ptr_array_index(self->notifications, i);

    if (notification->id == id)
      return notification;
  }

  return NULL;
}

/* =========================================================================
 * THE STORE
 * ========================================================================= */

static void emit_closed(WhelkNotifications *self, guint id, guint reason) {

  if (self->connection) {
    g_dbus_connection_emit_signal(
        self->connection, NULL, WHELK_NOTIFICATIONS_OBJECT_PATH,
        "org.freedesktop.Notifications", "NotificationClosed",
        g_variant_new("(uu)", id, reason), NULL);
  }

  g_signal_emit(self, signals[SIGNAL_CLOSED], 0, id);
}

static void remove_notification(WhelkNotifications *self,
                                WhelkNotification *notification,
                                guint reason) {

  if (!notification)
    return;

  const guint id = notification->id;

  g_ptr_array_remove(self->notifications, notification);

  emit_closed(self, id, reason);
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

/* =========================================================================
 * D-BUS
 * ========================================================================= */

static void on_method_call(GDBusConnection *connection, const char *sender,
                           const char *object_path, const char *interface_name,
                           const char *method_name, GVariant *parameters,
                           GDBusMethodInvocation *invocation,
                           gpointer user_data) {
  WhelkNotifications *self = user_data;

  (void)connection;
  (void)sender;
  (void)object_path;
  (void)interface_name;

  if (g_strcmp0(method_name, "GetCapabilities") == 0) {
    /* "persistence": a notification stays in the centre after its toast has
     * gone, which is exactly what this store does. */
    const char *capabilities[] = {"body",     "body-markup", "actions",
                                  "icon-static", "persistence", NULL};

    g_dbus_method_invocation_return_value(
        invocation, g_variant_new("(^as)", capabilities));
    return;
  }

  if (g_strcmp0(method_name, "GetServerInformation") == 0) {
    g_dbus_method_invocation_return_value(
        invocation,
        g_variant_new("(ssss)", "whelk", "whelk", PACKAGE_VERSION, "1.2"));
    return;
  }

  if (g_strcmp0(method_name, "CloseNotification") == 0) {
    guint id = 0;

    g_variant_get(parameters, "(u)", &id);
    remove_notification(self, find_notification(self, id), WHELK_CLOSED_CALL);
    g_dbus_method_invocation_return_value(invocation, NULL);
    return;
  }

  if (g_strcmp0(method_name, "Notify") != 0) {
    g_dbus_method_invocation_return_error(
        invocation, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_METHOD,
        "whelk does not implement %s", method_name);
    return;
  }

  const char *app_name = NULL;
  const char *app_icon = NULL;
  const char *summary = NULL;
  const char *body = NULL;
  guint replaces_id = 0;
  gint expire_timeout = -1;
  g_autoptr(GVariant) actions = NULL;
  g_autoptr(GVariant) hints = NULL;

  g_variant_get(parameters, "(&su&s&s&s@as@a{sv}i)", &app_name, &replaces_id,
                &app_icon, &summary, &body, &actions, &hints,
                &expire_timeout);

  WhelkNotification *incoming = notification_new();

  incoming->app_name = g_strdup(app_name ? app_name : "");
  incoming->app_icon = g_strdup(app_icon ? app_icon : "");
  incoming->summary = g_strdup(summary ? summary : "");
  incoming->body = g_strdup(body ? body : "");
  incoming->expire_timeout = expire_timeout;

  /* Actions arrive as (id, label, id, label, ...). */
  const gsize action_strings = g_variant_n_children(actions);

  for (gsize i = 0; i + 1 < action_strings; i += 2) {
    g_autoptr(GVariant) id = g_variant_get_child_value(actions, i);
    g_autoptr(GVariant) label = g_variant_get_child_value(actions, i + 1);

    g_ptr_array_add(incoming->actions,
                    action_new(g_variant_get_string(id, NULL),
                               g_variant_get_string(label, NULL)));
  }

  /* The hints worth keeping: what the notification looks like and how urgent
   * it says it is.  Unknown hints are dropped rather than stored forever. */
  const gsize hint_count = g_variant_n_children(hints);

  for (gsize i = 0; i < hint_count; i++) {
    g_autoptr(GVariant) entry = g_variant_get_child_value(hints, i);
    const char *key = NULL;
    g_autoptr(GVariant) wrapper = NULL;
    g_autoptr(GVariant) value = NULL;

    g_variant_get(entry, "{&s@v}", &key, &wrapper);
    value = g_variant_get_variant(wrapper);

    if (g_strcmp0(key, "desktop-entry") == 0 ||
        g_strcmp0(key, "desktop_entry") == 0) {
      if (g_variant_is_of_type(value, G_VARIANT_TYPE_STRING))
        incoming->desktop_entry = g_strdup(g_variant_get_string(value, NULL));
    } else if (g_strcmp0(key, "image-path") == 0 ||
               g_strcmp0(key, "image_path") == 0) {
      if (g_variant_is_of_type(value, G_VARIANT_TYPE_STRING))
        incoming->image_path = g_strdup(g_variant_get_string(value, NULL));
    } else if (g_strcmp0(key, "category") == 0) {
      if (g_variant_is_of_type(value, G_VARIANT_TYPE_STRING))
        incoming->category = g_strdup(g_variant_get_string(value, NULL));
    } else if (g_strcmp0(key, "urgency") == 0) {
      if (g_variant_is_of_type(value, G_VARIANT_TYPE_BYTE))
        incoming->urgency = g_variant_get_byte(value);
      else if (g_variant_is_of_type(value, G_VARIANT_TYPE_UINT32))
        incoming->urgency = (int)g_variant_get_uint32(value);
    } else if (g_strcmp0(key, "transient") == 0) {
      if (g_variant_is_of_type(value, G_VARIANT_TYPE_BOOLEAN))
        incoming->transient = g_variant_get_boolean(value);
    }
  }

  /* No timeout from the application means "this server decides".  A critical
   * notification never ages out on its own; that is what critical means. */
  if (expire_timeout < 0)
    incoming->expire_timeout = incoming->urgency >= 2
                                   ? 0
                                   : WHELK_NOTIFICATION_DEFAULT_TIMEOUT;

  WhelkNotification *stored =
      replaces_id != 0 ? find_notification(self, replaces_id) : NULL;

  guint id = 0;

  if (stored) {
    notification_replace(stored, incoming);
    id = stored->id;
    notification_free(incoming);
  } else {
    id = ++self->next_id;
    incoming->id = id;
    g_ptr_array_add(self->notifications, incoming);
    stored = incoming;
  }

  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);

  if (!self->dnd)
    g_signal_emit(self, signals[SIGNAL_POSTED], 0, stored);

  g_dbus_method_invocation_return_value(invocation, g_variant_new("(u)", id));
}

static const GDBusInterfaceVTable notification_vtable = {
    .method_call = on_method_call,
    .get_property = NULL,
    .set_property = NULL,
};

static void on_bus_acquired(GDBusConnection *connection, const char *name,
                            gpointer user_data) {
  WhelkNotifications *self = user_data;
  g_autoptr(GDBusNodeInfo) info = NULL;
  g_autoptr(GError) error = NULL;

  LOG_C_DEBUG("Notifications", "bus acquired for %s", name);

  self->connection = g_object_ref(connection);

  info = g_dbus_node_info_new_for_xml(notification_xml, &error);

  if (!info) {
    LOG_C_ERROR("Notifications", "the interface did not parse: %s",
                error->message);
    return;
  }

  self->registration_id = g_dbus_connection_register_object(
      connection, WHELK_NOTIFICATIONS_OBJECT_PATH, info->interfaces[0],
      &notification_vtable, self, NULL, &error);

  if (self->registration_id == 0) {
    LOG_C_ERROR("Notifications", "could not export the interface: %s",
                error->message);
  }
}

static void on_name_acquired(GDBusConnection *connection, const char *name,
                             gpointer user_data) {
  WhelkNotifications *self = user_data;

  (void)connection;

  self->owner = TRUE;

  LOG_C_INFO("Notifications", "%s is ours: this is the notification daemon",
             name);
}

static void on_name_lost(GDBusConnection *connection, const char *name,
                         gpointer user_data) {
  WhelkNotifications *self = user_data;

  (void)connection;

  self->owner = FALSE;

  LOG_C_WARN("Notifications",
             "%s is owned by another daemon: whelk keeps its store, and the "
             "other daemon shows the desktop's notifications",
             name);
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

WhelkNotifications *whelk_notifications_new(void) {
  return g_object_new(WHELK_TYPE_NOTIFICATIONS, NULL);
}

gboolean whelk_notifications_start(WhelkNotifications *self, GError **error) {
  g_return_val_if_fail(WHELK_IS_NOTIFICATIONS(self), FALSE);


  if (self->owner_id != 0)
    return TRUE;

  self->owner_id = g_bus_own_name(
      G_BUS_TYPE_SESSION, WHELK_NOTIFICATIONS_BUS_NAME,
      G_BUS_NAME_OWNER_FLAGS_NONE, on_bus_acquired, on_name_acquired,
      on_name_lost, self, NULL);

  (void)error;

  return self->owner_id != 0;
}

gboolean whelk_notifications_is_owner(WhelkNotifications *self) {
  g_return_val_if_fail(WHELK_IS_NOTIFICATIONS(self), FALSE);

  return self->owner;
}

GPtrArray *whelk_notifications_snapshot(WhelkNotifications *self) {
  g_return_val_if_fail(WHELK_IS_NOTIFICATIONS(self), NULL);

  return self->notifications;
}

guint whelk_notifications_count(WhelkNotifications *self) {
  g_return_val_if_fail(WHELK_IS_NOTIFICATIONS(self), 0);

  return self->notifications->len;
}

gboolean whelk_notifications_has_dnd(WhelkNotifications *self) {
  g_return_val_if_fail(WHELK_IS_NOTIFICATIONS(self), FALSE);

  return self->dnd;
}

void whelk_notifications_set_dnd(WhelkNotifications *self, gboolean dnd) {
  g_return_if_fail(WHELK_IS_NOTIFICATIONS(self));


  if (self->dnd == dnd)
    return;

  self->dnd = dnd;

  LOG_C_INFO("Notifications", "do not disturb: %s", dnd ? "on" : "off");

  g_signal_emit(self, signals[SIGNAL_DND_CHANGED], 0);
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

void whelk_notifications_close(WhelkNotifications *self, guint id) {
  g_return_if_fail(WHELK_IS_NOTIFICATIONS(self));


  remove_notification(self, find_notification(self, id),
                      WHELK_CLOSED_DISMISSED);
}

void whelk_notifications_expire(WhelkNotifications *self, guint id) {
  g_return_if_fail(WHELK_IS_NOTIFICATIONS(self));


  remove_notification(self, find_notification(self, id),
                      WHELK_CLOSED_EXPIRED);
}

void whelk_notifications_close_all(WhelkNotifications *self) {
  g_return_if_fail(WHELK_IS_NOTIFICATIONS(self));


  /* Copy the ids first: closing emits signals, and a listener may well decide
   * to close something itself. */
  GArray *ids = g_array_new(FALSE, FALSE, sizeof(guint));

  for (guint i = 0; i < self->notifications->len; i++)
    g_array_append_val(ids,
                       ((WhelkNotification *)g_ptr_array_index(
                            self->notifications, i))
                           ->id);

  for (guint i = 0; i < ids->len; i++)
    remove_notification(
        self, find_notification(self, g_array_index(ids, guint, i)),
        WHELK_CLOSED_DISMISSED);

  g_array_unref(ids);
}

void whelk_notifications_invoke(WhelkNotifications *self,
                                WhelkNotification *notification,
                                const char *action) {
  g_return_if_fail(WHELK_IS_NOTIFICATIONS(self));

  if (!notification)
    return;

  const guint id = notification->id;
  const char *key = action ? action : "default";

  if (self->connection) {
    g_dbus_connection_emit_signal(
        self->connection, NULL, WHELK_NOTIFICATIONS_OBJECT_PATH,
        "org.freedesktop.Notifications", "ActionInvoked",
        g_variant_new("(us)", id, key), NULL);
  }

  /* The specification says invoking an action closes the notification.  An
   * application that wants it to stay sends a replaces_id for it. */
  remove_notification(self, notification, WHELK_CLOSED_DISMISSED);
}

/* --- notification reading -------------------------------------------------- */

guint whelk_notification_id(WhelkNotification *notification) {
  return notification ? notification->id : 0;
}

const char *whelk_notification_app(WhelkNotification *notification) {
  return notification ? notification->app_name : "";
}

const char *whelk_notification_summary(WhelkNotification *notification) {
  return notification ? notification->summary : "";
}

const char *whelk_notification_body(WhelkNotification *notification) {
  return notification ? notification->body : "";
}

const char *whelk_notification_icon(WhelkNotification *notification) {
  if (!notification)
    return "";

  if (notification->image_path && *notification->image_path)
    return notification->image_path;

  if (notification->app_icon && *notification->app_icon)
    return notification->app_icon;

  return notification->desktop_entry ? notification->desktop_entry : "";
}

const char *whelk_notification_category(WhelkNotification *notification) {
  return notification && notification->category ? notification->category : "";
}

int whelk_notification_urgency(WhelkNotification *notification) {
  return notification ? notification->urgency : 1;
}

gboolean whelk_notification_is_transient(WhelkNotification *notification) {
  return notification ? notification->transient : FALSE;
}

gint64 whelk_notification_timestamp(WhelkNotification *notification) {
  return notification ? notification->timestamp : 0;
}

int whelk_notification_timeout(WhelkNotification *notification) {
  return notification ? notification->expire_timeout : 0;
}

char *whelk_notification_time(WhelkNotification *notification) {
  g_autoptr(GDateTime) stamp = NULL;

  if (!notification)
    return g_strdup("");

  stamp = g_date_time_new_from_unix_local(
      whelk_notification_timestamp(notification) / G_USEC_PER_SEC);

  if (!stamp)
    return g_strdup("");

  return g_date_time_format(stamp, "%H:%M");
}

guint whelk_notification_action_count(WhelkNotification *notification) {
  if (!notification || !notification->actions)
    return 0;

  return notification->actions->len;
}

static NotificationAction *action_at(WhelkNotification *notification,
                                     guint index) {
  if (!notification || !notification->actions ||
      index >= notification->actions->len)
    return NULL;

  return g_ptr_array_index(notification->actions, index);
}

const char *whelk_notification_action_label(WhelkNotification *notification,
                                            guint index) {
  NotificationAction *action = action_at(notification, index);

  return action ? action->label : "";
}

const char *whelk_notification_action_id(WhelkNotification *notification,
                                         guint index) {
  NotificationAction *action = action_at(notification, index);

  return action ? action->id : "";
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_notifications_dispose(GObject *object) {
  WhelkNotifications *self = WHELK_NOTIFICATIONS(object);

  if (self->owner_id != 0) {
    g_bus_unown_name(self->owner_id);
    self->owner_id = 0;
  }

  if (self->connection) {
    if (self->registration_id != 0)
      g_dbus_connection_unregister_object(self->connection,
                                          self->registration_id);

    g_clear_object(&self->connection);
  }

  self->registration_id = 0;
  g_clear_pointer(&self->notifications, g_ptr_array_unref);

  G_OBJECT_CLASS(whelk_notifications_parent_class)->dispose(object);
}

static void whelk_notifications_class_init(WhelkNotificationsClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_notifications_dispose;

  /**
   * WhelkNotifications::changed:
   *
   * The store changed: something was added, replaced, closed or expired.
   */
  signals[SIGNAL_CHANGED] = g_signal_new(
      "changed", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST, 0, NULL, NULL,
      NULL, G_TYPE_NONE, 0);

  /**
   * WhelkNotifications::posted:
   * @notification: the new or replaced notification.
   *
   * A notification arrived and the user is not in do-not-disturb: raise a
   * toast.  Not emitted while DND is on.
   */
  signals[SIGNAL_POSTED] = g_signal_new(
      "posted", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST, 0, NULL, NULL,
      NULL, G_TYPE_NONE, 1, G_TYPE_POINTER);

  /**
   * WhelkNotifications::closed:
   * @id: the notification's id.
   */
  signals[SIGNAL_CLOSED] = g_signal_new("closed", G_TYPE_FROM_CLASS(klass),
                                        G_SIGNAL_RUN_LAST, 0, NULL, NULL, NULL,
                                        G_TYPE_NONE, 1, G_TYPE_UINT);

  /**
   * WhelkNotifications::dnd-changed:
   *
   * Do not disturb was toggled.
   */
  signals[SIGNAL_DND_CHANGED] = g_signal_new(
      "dnd-changed", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST, 0, NULL, NULL,
      NULL, G_TYPE_NONE, 0);
}

static void whelk_notifications_init(WhelkNotifications *self) {

  self->notifications =
      g_ptr_array_new_with_free_func((GDestroyNotify)notification_free);
  self->next_id = 0;
  self->dnd = FALSE;
  self->owner = FALSE;
}

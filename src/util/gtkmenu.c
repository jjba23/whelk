// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/gtkmenu.h"

/* A menu change arrives as a batch of add/remove tuples; re-reading once after
 * the batch is cheaper and safer than applying the diffs to a rendered tree. */
#define WHELK_GTKMENU_DEBOUNCE_MS 120
#define WHELK_GTKMENU_TIMEOUT_MS 1500

struct _WhelkGtkMenuWatch {
  GDBusConnection *bus;
  char *service;
  char *path;
  WhelkChordStyle style;
  WhelkGtkMenuChangedFunc changed;
  WhelkGtkMenuInvalidatedFunc invalidated;
  gpointer user_data;

  guint subscription;
  guint debounce_id;
  GCancellable *cancellable;
  gint refs;
  gboolean cancelled;
  gboolean subscribed;
  gboolean starting;
  gboolean refresh_pending;
};

static void watch_unref(WhelkGtkMenuWatch *watch);

static WhelkGtkMenuWatch *watch_ref(WhelkGtkMenuWatch *watch) {
  g_atomic_int_inc(&watch->refs);

  return watch;
}

static GVariant *group_zero(void) {
  GVariantBuilder groups;

  g_variant_builder_init(&groups, G_VARIANT_TYPE("au"));
  g_variant_builder_add(&groups, "u", 0);

  return g_variant_new("(au)", &groups);
}

/* End is fire-and-forget: its reply carries nothing and a failure only means
 * the application already dropped the group. */
static void end_group(WhelkGtkMenuWatch *watch) {
  if (!watch->subscribed)
    return;

  watch->subscribed = FALSE;
  g_dbus_connection_call(watch->bus, watch->service, watch->path,
                         "org.gtk.Menus", "End", group_zero(), NULL,
                         G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL, NULL);
}

static void start_group(WhelkGtkMenuWatch *watch);

static void on_start(GObject *source, GAsyncResult *result, gpointer user_data) {
  WhelkGtkMenuWatch *watch = user_data;
  g_autoptr(GError) error = NULL;
  g_autoptr(GVariant) reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  watch->starting = FALSE;

  if (watch->cancelled)
    goto out;

  if (!reply) {
    watch->subscribed = FALSE;
    goto out;
  }

  g_autoptr(GVariant) menus = g_variant_get_child_value(reply, 0);
  GPtrArray *tree = whelk_menubar_from_gtk(menus, 0, 0, watch->style);

  if (watch->changed)
    watch->changed(tree, watch->user_data);
  else
    g_ptr_array_unref(tree);

  if (watch->refresh_pending) {
    watch->refresh_pending = FALSE;
    /* The debounce already waited; a second Changed during the read only asks
     * for one more read, not a second delay. */
    end_group(watch);
    start_group(watch);
  }

out:
  watch_unref(watch);
}

static void start_group(WhelkGtkMenuWatch *watch) {
  if (watch->cancelled || watch->starting)
    return;

  watch->subscribed = TRUE;
  watch->starting = TRUE;

  g_dbus_connection_call(watch->bus, watch->service, watch->path,
                         "org.gtk.Menus", "Start", group_zero(), NULL,
                         G_DBUS_CALL_FLAGS_NONE, WHELK_GTKMENU_TIMEOUT_MS,
                         watch->cancellable, on_start, watch_ref(watch));
}

static gboolean on_debounce(gpointer user_data) {
  WhelkGtkMenuWatch *watch = user_data;

  watch->debounce_id = 0;

  if (!watch->cancelled) {
    if (watch->starting) {
      watch->refresh_pending = TRUE;
    } else {
      /* Drop the old group and ask for a fresh full tree.  Sending End before
       * Start means the application never has two subscriptions for us. */
      end_group(watch);
      start_group(watch);
    }
  }

  return G_SOURCE_REMOVE;
}

static void on_changed(GDBusConnection *connection, const char *sender,
                       const char *object_path, const char *interface_name,
                       const char *signal_name, GVariant *parameters,
                       gpointer user_data) {
  WhelkGtkMenuWatch *watch = user_data;

  (void)connection;
  (void)sender;
  (void)object_path;
  (void)interface_name;
  (void)signal_name;
  (void)parameters;

  if (watch->cancelled)
    return;

  if (watch->invalidated)
    watch->invalidated(watch->user_data);

  if (watch->debounce_id != 0)
    g_source_remove(watch->debounce_id);

  watch->debounce_id = g_timeout_add_full(
      G_PRIORITY_DEFAULT, WHELK_GTKMENU_DEBOUNCE_MS, on_debounce,
      watch_ref(watch), (GDestroyNotify)watch_unref);
}

WhelkGtkMenuWatch *whelk_gtkmenu_watch_new(
    GDBusConnection *bus, const char *service, const char *path,
    WhelkChordStyle style, WhelkGtkMenuChangedFunc changed,
    WhelkGtkMenuInvalidatedFunc invalidated, gpointer user_data) {
  g_return_val_if_fail(G_IS_DBUS_CONNECTION(bus), NULL);
  g_return_val_if_fail(service && g_dbus_is_name(service), NULL);
  g_return_val_if_fail(path && g_variant_is_object_path(path), NULL);

  WhelkGtkMenuWatch *watch = g_new0(WhelkGtkMenuWatch, 1);

  watch->bus = g_object_ref(bus);
  watch->service = g_strdup(service);
  watch->path = g_strdup(path);
  watch->style = style;
  watch->changed = changed;
  watch->invalidated = invalidated;
  watch->user_data = user_data;
  watch->cancellable = g_cancellable_new();
  watch->refs = 1;

  /* Subscribe before Start: a Changed emitted between the two calls would
   * otherwise be lost, and a menu with an outdated tree would stay open. */
  watch->subscription = g_dbus_connection_signal_subscribe(
      bus, service, "org.gtk.Menus", "Changed", path, NULL,
      G_DBUS_SIGNAL_FLAGS_NONE, on_changed, watch, NULL);

  start_group(watch);

  return watch;
}

static void watch_destroy(WhelkGtkMenuWatch *watch) {
  if (watch->debounce_id != 0) {
    g_source_remove(watch->debounce_id);
    watch->debounce_id = 0;
  }

  if (watch->subscription != 0) {
    g_dbus_connection_signal_unsubscribe(watch->bus, watch->subscription);
    watch->subscription = 0;
  }

  end_group(watch);

  g_clear_object(&watch->cancellable);
  g_clear_object(&watch->bus);
  g_free(watch->service);
  g_free(watch->path);
  g_free(watch);
}

static void watch_unref(WhelkGtkMenuWatch *watch) {
  if (watch && g_atomic_int_dec_and_test(&watch->refs))
    watch_destroy(watch);
}

void whelk_gtkmenu_watch_free(WhelkGtkMenuWatch *watch) {
  if (!watch)
    return;

  watch->cancelled = TRUE;
  g_cancellable_cancel(watch->cancellable);

  if (watch->debounce_id != 0) {
    g_source_remove(watch->debounce_id);
    watch->debounce_id = 0;
  }

  if (watch->subscription != 0) {
    g_dbus_connection_signal_unsubscribe(watch->bus, watch->subscription);
    watch->subscription = 0;
  }

  end_group(watch);

  watch_unref(watch);
}

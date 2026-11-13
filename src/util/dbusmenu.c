// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/dbusmenu.h"

#include "logger.h"

/* LayoutUpdated arrives once per changed menu and ItemsPropertiesUpdated once
 * per batch of changed items; a single re-read is enough for all of them. */
#define WHELK_DBUSMENU_DEBOUNCE_MS 120
#define WHELK_DBUSMENU_TIMEOUT_MS 1500

struct _WhelkDBusMenuWatch {
  GDBusConnection *bus;
  char *service;
  char *path;
  WhelkChordStyle style;
  WhelkDBusMenuChangedFunc changed;
  WhelkDBusMenuInvalidatedFunc invalidated;
  gpointer user_data;

  guint subscription;
  guint debounce_id;
  GCancellable *cancellable;
  gint refs;
  gboolean cancelled;
};

struct _WhelkDBusMenuRequest {
  GDBusConnection *bus;
  char *service;
  char *path;
  WhelkChordStyle style;
  WhelkDBusMenuFetchedFunc fetched;
  gpointer user_data;

  GCancellable *cancellable;
  gint refs;
  gboolean cancelled;
};

static void watch_unref(WhelkDBusMenuWatch *watch);

static WhelkDBusMenuWatch *watch_ref(WhelkDBusMenuWatch *watch) {
  g_atomic_int_inc(&watch->refs);

  return watch;
}

/* =========================================================================
 * READING THE TREE AGAIN
 * ========================================================================= */

static void on_layout(GObject *source, GAsyncResult *result,
                      gpointer user_data) {
  WhelkDBusMenuWatch *watch = user_data;

  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (error)
    LOG_C_DEBUG("Menu", "GetLayout failed: %s", error->message);

  if (reply && !watch->cancelled && watch->changed) {
    GPtrArray *menus = whelk_menubar_from_dbusmenu(
        reply, watch->service, watch->path, watch->style);

    if (menus->len > 0)
      watch->changed(menus, watch->user_data);
    else
      g_ptr_array_unref(menus);
  }

  if (reply)
    g_variant_unref(reply);

  watch_unref(watch);
}

/* =========================================================================
 * ONE READ, IN THE APPLET'S ORDER
 * ========================================================================= */

static void request_unref(WhelkDBusMenuRequest *request);

static WhelkDBusMenuRequest *request_ref(WhelkDBusMenuRequest *request) {
  g_atomic_int_inc(&request->refs);

  return request;
}

static void request_unref(WhelkDBusMenuRequest *request) {
  if (!request || !g_atomic_int_dec_and_test(&request->refs))
    return;

  g_clear_object(&request->cancellable);
  g_clear_object(&request->bus);
  g_free(request->service);
  g_free(request->path);
  g_free(request);
}

static void on_fetch_layout(GObject *source, GAsyncResult *result,
                            gpointer user_data) {
  WhelkDBusMenuRequest *request = user_data;
  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (error)
    LOG_C_DEBUG("Menu", "tray GetLayout failed: %s", error->message);

  if (!request->cancelled) {
    GPtrArray *menus = NULL;

    if (reply) {
      menus = whelk_menubar_from_dbusmenu_context(
          reply, request->service, request->path, request->style);

      LOG_C_DEBUG("Menu", "tray menu from %s: %u menu(s)", request->service,
                  menus->len);

      if (menus->len == 0)
        g_clear_pointer(&menus, g_ptr_array_unref);
    }

    if (request->fetched)
      request->fetched(menus, request->user_data);
  }

  if (reply)
    g_variant_unref(reply);

  request_unref(request);
}

static void request_layout(WhelkDBusMenuRequest *request) {
  if (request->cancelled)
    return;

  g_dbus_connection_call(
      request->bus, request->service, request->path, "com.canonical.dbusmenu",
      "GetLayout", g_variant_new("(ii)", 0, -1),
      G_VARIANT_TYPE("(u(ia{sv}av))"), G_DBUS_CALL_FLAGS_NONE,
      WHELK_DBUSMENU_TIMEOUT_MS, request->cancellable, on_fetch_layout,
      request_ref(request));
}

static void on_about_to_show(GObject *source, GAsyncResult *result,
                             gpointer user_data) {
  WhelkDBusMenuRequest *request = user_data;
  g_autoptr(GError) error = NULL;
  g_autoptr(GVariant) reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  (void)reply;

  if (error)
    LOG_C_DEBUG("Menu", "tray AboutToShow failed: %s", error->message);

  request_layout(request);
  request_unref(request);
}

WhelkDBusMenuRequest *whelk_dbusmenu_fetch(
    GDBusConnection *bus, const char *service, const char *path,
    WhelkChordStyle style, WhelkDBusMenuFetchedFunc fetched,
    gpointer user_data) {
  if (!bus || !service || !path)
    return NULL;

  WhelkDBusMenuRequest *request = g_new0(WhelkDBusMenuRequest, 1);

  request->bus = g_object_ref(bus);
  request->service = g_strdup(service);
  request->path = g_strdup(path);
  request->style = style;
  request->fetched = fetched;
  request->user_data = user_data;
  request->refs = 1;
  request->cancellable = g_cancellable_new();

  g_dbus_connection_call(bus, service, path, "com.canonical.dbusmenu",
                         "AboutToShow", g_variant_new("(i)", 0),
                         G_VARIANT_TYPE("(b)"), G_DBUS_CALL_FLAGS_NONE,
                         WHELK_DBUSMENU_TIMEOUT_MS, request->cancellable,
                         on_about_to_show, request_ref(request));

  return request;
}

void whelk_dbusmenu_request_free(WhelkDBusMenuRequest *request) {
  if (!request)
    return;

  request->cancelled = TRUE;
  request->fetched = NULL;
  request->user_data = NULL;

  if (request->cancellable)
    g_cancellable_cancel(request->cancellable);

  request_unref(request);
}

static void fetch(WhelkDBusMenuWatch *watch) {
  if (watch->cancelled)
    return;

  g_dbus_connection_call(
      watch->bus, watch->service, watch->path, "com.canonical.dbusmenu",
      "GetLayout", g_variant_new("(ii)", 0, -1),
      G_VARIANT_TYPE("(u(ia{sv}av))"), G_DBUS_CALL_FLAGS_NONE,
      WHELK_DBUSMENU_TIMEOUT_MS, watch->cancellable, on_layout,
      watch_ref(watch));
}

static gboolean on_debounce(gpointer user_data) {
  WhelkDBusMenuWatch *watch = user_data;

  watch->debounce_id = 0;
  fetch(watch);
  watch_unref(watch);

  return G_SOURCE_REMOVE;
}

static void schedule_refresh(WhelkDBusMenuWatch *watch) {
  if (watch->cancelled || watch->debounce_id != 0)
    return;

  watch->debounce_id = g_timeout_add_full(
      G_PRIORITY_DEFAULT, WHELK_DBUSMENU_DEBOUNCE_MS, on_debounce,
      watch_ref(watch), NULL);
}

/* =========================================================================
 * THE TWO SIGNALS
 * ========================================================================= */

static void on_signal(GDBusConnection *connection, const char *sender_name,
                      const char *object_path, const char *interface_name,
                      const char *signal_name, GVariant *parameters,
                      gpointer user_data) {
  (void)connection;
  (void)sender_name;
  (void)object_path;
  (void)interface_name;
  (void)parameters;

  WhelkDBusMenuWatch *watch = user_data;

  if (watch->cancelled)
    return;

  if (g_strcmp0(signal_name, "LayoutUpdated") == 0) {
    /* Ids may already have moved: close what was drawn from the old tree now,
     * before the read.  A property change does not move ids, so it only costs
     * a re-read. */
    if (watch->invalidated)
      watch->invalidated(watch->user_data);

    schedule_refresh(watch);
  } else if (g_strcmp0(signal_name, "ItemsPropertiesUpdated") == 0) {
    schedule_refresh(watch);
  }
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void watch_unref(WhelkDBusMenuWatch *watch) {
  if (!watch || !g_atomic_int_dec_and_test(&watch->refs))
    return;

  if (watch->subscription != 0)
    g_dbus_connection_signal_unsubscribe(watch->bus, watch->subscription);

  g_clear_object(&watch->cancellable);
  g_clear_object(&watch->bus);
  g_free(watch->service);
  g_free(watch->path);
  g_free(watch);
}

WhelkDBusMenuWatch *whelk_dbusmenu_watch_new(
    GDBusConnection *bus, const char *service, const char *path,
    WhelkChordStyle style, WhelkDBusMenuChangedFunc changed,
    WhelkDBusMenuInvalidatedFunc invalidated, gpointer user_data) {
  if (!bus || !service || !path)
    return NULL;

  WhelkDBusMenuWatch *watch = g_new0(WhelkDBusMenuWatch, 1);

  watch->bus = g_object_ref(bus);
  watch->service = g_strdup(service);
  watch->path = g_strdup(path);
  watch->style = style;
  watch->changed = changed;
  watch->invalidated = invalidated;
  watch->user_data = user_data;
  watch->refs = 1;
  watch->cancellable = g_cancellable_new();

  watch->subscription = g_dbus_connection_signal_subscribe(
      bus, service, "com.canonical.dbusmenu", NULL, path, NULL,
      G_DBUS_SIGNAL_FLAGS_NONE, on_signal, watch, NULL);

  LOG_C_DEBUG("Menu", "watching DBusMenu at %s %s", service, path);

  return watch;
}

void whelk_dbusmenu_watch_free(WhelkDBusMenuWatch *watch) {
  if (!watch)
    return;

  /* Cancelled before the source and the subscription go: a reply already on
   * its way must find nobody to tell. */
  watch->cancelled = TRUE;
  watch->changed = NULL;
  watch->invalidated = NULL;
  watch->user_data = NULL;

  if (watch->cancellable)
    g_cancellable_cancel(watch->cancellable);

  if (watch->subscription != 0) {
    g_dbus_connection_signal_unsubscribe(watch->bus, watch->subscription);
    watch->subscription = 0;
  }

  if (watch->debounce_id != 0) {
    g_source_remove(watch->debounce_id);
    watch->debounce_id = 0;
    /* The source held a reference of its own. */
    watch_unref(watch);
  }

  watch_unref(watch);
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/atspimenus.h"

#include "logger.h"

#include <string.h>

#define WHELK_ATSPI_TIMEOUT_MS 250
#define WHELK_ATSPI_MAX_DEPTH 9
#define WHELK_ATSPI_MAX_NODES 600
#define WHELK_ATSPI_MAX_CHILDREN 120

#define WHELK_ATSPI_ROOT_SERVICE "org.a11y.atspi.Registry"
#define WHELK_ATSPI_ROOT_PATH "/org/a11y/atspi/accessible/root"
#define WHELK_ATSPI_REGISTRY_PATH "/org/a11y/atspi/registry"
#define WHELK_ATSPI_EVENT_OBJECT "org.a11y.atspi.Event.Object"
#define WHELK_ATSPI_EVENT_REGISTRATION "Object:"
#define WHELK_ATSPI_CHANGE_DELAY_MS 150

/* The wire values from atspi-constants.h.  They are part of AT-SPI's ABI, and
 * spelling them here keeps libatspi out of the build: the bus is the API. */
#define WHELK_ATSPI_ROLE_MENU 33
#define WHELK_ATSPI_ROLE_MENU_BAR 34
#define WHELK_ATSPI_ROLE_MENU_ITEM 35
#define WHELK_ATSPI_ROLE_CHECK_MENU_ITEM 8
#define WHELK_ATSPI_ROLE_RADIO_MENU_ITEM 45
#define WHELK_ATSPI_ROLE_SEPARATOR 50

#define WHELK_ATSPI_STATE_ENABLED 8
#define WHELK_ATSPI_STATE_SENSITIVE 24

typedef struct {
  char *service;
  char *path;
} AtspiRef;

typedef struct {
  GDBusConnection *bus;
  GCancellable *cancel;
  WhelkChordStyle style;
  guint budget;
} AtspiWalk;

static AtspiRef *atspi_ref_new(const char *service, const char *path) {
  AtspiRef *ref = g_new0(AtspiRef, 1);

  ref->service = g_strdup(service);
  ref->path = g_strdup(path);
  return ref;
}

static void atspi_ref_free(gpointer data) {
  AtspiRef *ref = data;

  if (!ref)
    return;

  g_free(ref->service);
  g_free(ref->path);
  g_free(ref);
}

static GPtrArray *atspi_refs_new(void) {
  return g_ptr_array_new_with_free_func(atspi_ref_free);
}

static AtspiRef *atspi_ref_copy(const AtspiRef *ref) {
  return ref ? atspi_ref_new(ref->service, ref->path) : NULL;
}

/* =========================================================================
 * TALKING TO THE BUS
 * ========================================================================= */

static GVariant *atspi_call(GDBusConnection *bus, const char *service,
                            const char *path, const char *interface,
                            const char *method, GVariant *parameters,
                            GCancellable *cancel) {
  if (!bus || !service || !path)
    return NULL;

  g_autoptr(GError) error = NULL;
  GVariant *reply = g_dbus_connection_call_sync(
      bus, service, path, interface, method, parameters, NULL,
      G_DBUS_CALL_FLAGS_NO_AUTO_START, WHELK_ATSPI_TIMEOUT_MS, cancel, &error);

  if (!reply && error &&
      !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_CANCELLED))
    LOG_C_DEBUG("AT-SPI", "%s.%s on %s failed: %s", interface, method, path,
                error->message);

  return reply;
}

static char *atspi_property_string(GDBusConnection *bus, const AtspiRef *ref,
                                   const char *interface, const char *property,
                                   GCancellable *cancel) {
  g_autoptr(GVariant) reply = atspi_call(
      bus, ref->service, ref->path, "org.freedesktop.DBus.Properties", "Get",
      g_variant_new("(ss)", interface, property), cancel);

  if (!reply)
    return NULL;

  g_autoptr(GVariant) boxed = g_variant_get_child_value(reply, 0);
  g_autoptr(GVariant) value = g_variant_get_variant(boxed);

  if (!g_variant_is_of_type(value, G_VARIANT_TYPE_STRING))
    return NULL;

  return g_variant_dup_string(value, NULL);
}

static char *atspi_name(GDBusConnection *bus, const AtspiRef *ref,
                        GCancellable *cancel) {
  return atspi_property_string(bus, ref, "org.a11y.atspi.Accessible", "Name",
                               cancel);
}

static guint atspi_role(GDBusConnection *bus, const AtspiRef *ref,
                        GCancellable *cancel) {
  g_autoptr(GVariant) reply =
      atspi_call(bus, ref->service, ref->path, "org.a11y.atspi.Accessible",
                 "GetRole", NULL, cancel);

  if (!reply)
    return 0;

  guint role = 0;

  g_variant_get(reply, "(u)", &role);
  return role;
}

static GPtrArray *atspi_children(GDBusConnection *bus, const AtspiRef *ref,
                                 GCancellable *cancel) {
  GPtrArray *children = atspi_refs_new();
  g_autoptr(GVariant) reply =
      atspi_call(bus, ref->service, ref->path, "org.a11y.atspi.Accessible",
                 "GetChildren", NULL, cancel);

  if (!reply)
    return children;

  g_autoptr(GVariant) list = g_variant_get_child_value(reply, 0);
  guint count = g_variant_n_children(list);

  if (count > WHELK_ATSPI_MAX_CHILDREN)
    count = WHELK_ATSPI_MAX_CHILDREN;

  GVariantIter iter;

  g_variant_iter_init(&iter, list);

  for (guint i = 0; i < count; i++) {
    g_autoptr(GVariant) entry = g_variant_iter_next_value(&iter);

    if (!entry || g_variant_n_children(entry) < 2)
      continue;

    g_autoptr(GVariant) service = g_variant_get_child_value(entry, 0);
    g_autoptr(GVariant) path = g_variant_get_child_value(entry, 1);
    const char *service_text =
        g_variant_is_of_type(service, G_VARIANT_TYPE_STRING)
            ? g_variant_get_string(service, NULL)
            : NULL;
    const char *path_text =
        (g_variant_is_of_type(path, G_VARIANT_TYPE_OBJECT_PATH) ||
         g_variant_is_of_type(path, G_VARIANT_TYPE_STRING))
            ? g_variant_get_string(path, NULL)
            : NULL;

    if (service_text && path_text && *service_text && *path_text)
      g_ptr_array_add(children, atspi_ref_new(service_text, path_text));
  }

  return children;
}

static gboolean atspi_state(GDBusConnection *bus, const AtspiRef *ref,
                            GCancellable *cancel, guint state) {
  g_autoptr(GVariant) reply =
      atspi_call(bus, ref->service, ref->path, "org.a11y.atspi.Accessible",
                 "GetState", NULL, cancel);

  if (!reply)
    return TRUE; /* no state to say otherwise */

  g_autoptr(GVariant) list = g_variant_get_child_value(reply, 0);
  const guint count = g_variant_n_children(list);
  guint64 bits[2] = {0, 0};

  for (guint i = 0; i < count && i < 2; i++) {
    g_autoptr(GVariant) word = g_variant_get_child_value(list, i);

    bits[i] = g_variant_get_uint32(word);
  }

  const guint word = state / 32;
  const guint bit = state % 32;

  return word < 2 && (bits[word] & (G_GUINT64_CONSTANT(1) << bit)) != 0;
}

static char *atspi_key_binding(GDBusConnection *bus, const AtspiRef *ref,
                               GCancellable *cancel) {
  g_autoptr(GVariant) reply =
      atspi_call(bus, ref->service, ref->path, "org.a11y.atspi.Action",
                 "GetKeyBinding", g_variant_new("(i)", 0), cancel);

  if (!reply)
    return NULL;

  const char *binding = NULL;

  g_variant_get(reply, "(&s)", &binding);

  return (binding && *binding) ? g_strdup(binding) : NULL;
}

static gboolean atspi_pid_is(GDBusConnection *bus, const char *service, gint pid,
                             GCancellable *cancel) {
  if (pid <= 0 || !service)
    return FALSE;

  g_autoptr(GVariant) reply = atspi_call(
      bus, "org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "GetConnectionUnixProcessID",
      g_variant_new("(s)", service), cancel);

  if (!reply)
    return FALSE;

  guint owner_pid = 0;

  g_variant_get(reply, "(u)", &owner_pid);
  return owner_pid == (guint)pid;
}

static gboolean names_match(const char *candidate, const char *wanted) {
  if (!candidate || !wanted || !*candidate || !*wanted)
    return FALSE;

  if (g_ascii_strcasecmp(candidate, wanted) == 0)
    return TRUE;

  /* A desktop entry says "Firefox"; some toolkits say "Mozilla Firefox". */
  return g_strrstr(candidate, wanted) != NULL ||
         g_strrstr(wanted, candidate) != NULL;
}

/* =========================================================================
 * THE TREE
 * ========================================================================= */

static AtspiRef *atspi_application(GDBusConnection *bus, const char *app_name,
                                   gint pid, GCancellable *cancel) {
  AtspiRef *root =
      atspi_ref_new(WHELK_ATSPI_ROOT_SERVICE, WHELK_ATSPI_ROOT_PATH);
  GPtrArray *children = atspi_children(bus, root, cancel);
  AtspiRef *by_name = NULL;
  AtspiRef *chosen = NULL;

  for (guint i = 0; i < children->len && !chosen; i++) {
    AtspiRef *candidate = g_ptr_array_index(children, i);

    if (atspi_pid_is(bus, candidate->service, pid, cancel)) {
      chosen = atspi_ref_copy(candidate);
      break;
    }

    if (!by_name && app_name && *app_name) {
      g_autofree char *name = atspi_name(bus, candidate, cancel);

      if (names_match(name, app_name))
        by_name = atspi_ref_copy(candidate);
    }
  }

  if (!chosen && by_name)
    chosen = atspi_ref_copy(by_name);

  /* A caller with neither a pid nor a name (a harness, or a session that knows
   * nothing about the focused window) gets the first application: it is the
   * only answer the bus can give without a question. */
  if (!chosen && children->len > 0 && pid <= 0 &&
      (!app_name || *app_name == '\0'))
    chosen = atspi_ref_copy(g_ptr_array_index(children, 0));

  g_clear_pointer(&by_name, atspi_ref_free);
  g_ptr_array_unref(children);
  atspi_ref_free(root);

  return chosen;
}

static gboolean atspi_find_menu_bar(AtspiWalk *walk, const AtspiRef *node,
                                    guint depth, GPtrArray *out) {
  if (!walk || walk->budget == 0 || depth > WHELK_ATSPI_MAX_DEPTH)
    return FALSE;

  walk->budget--;

  const guint role = atspi_role(walk->bus, node, walk->cancel);

  if (role == WHELK_ATSPI_ROLE_MENU_BAR) {
    g_ptr_array_add(out, atspi_ref_copy(node));
    return TRUE;
  }

  if (depth == WHELK_ATSPI_MAX_DEPTH)
    return FALSE;

  GPtrArray *children = atspi_children(walk->bus, node, walk->cancel);

  for (guint i = 0; i < children->len; i++) {
    AtspiRef *child = g_ptr_array_index(children, i);

    if (atspi_find_menu_bar(walk, child, depth + 1, out)) {
      g_ptr_array_unref(children);
      return TRUE;
    }

    if (walk->budget == 0)
      break;
  }

  g_ptr_array_unref(children);
  return FALSE;
}

static void atspi_append_items(AtspiWalk *walk, WhelkMenu *menu,
                               const AtspiRef *container, guint depth);

static void atspi_append_item(AtspiWalk *walk, WhelkMenu *menu,
                              const AtspiRef *item, guint depth) {
  g_autofree char *label = atspi_name(walk->bus, item, walk->cancel);

  if (!label || *label == '\0')
    return;

  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  entry->enabled = atspi_state(walk->bus, item, walk->cancel,
                               WHELK_ATSPI_STATE_ENABLED) ||
                   atspi_state(walk->bus, item, walk->cancel,
                               WHELK_ATSPI_STATE_SENSITIVE);

  g_autofree char *binding = atspi_key_binding(walk->bus, item, walk->cancel);

  if (binding && *binding)
    entry->accel = whelk_chord_sequence_display(binding, walk->style);

  entry->atspi_service = g_strdup(item->service);
  entry->atspi_path = g_strdup(item->path);
  entry->atspi_action = 0;

  if (depth >= WHELK_ATSPI_MAX_DEPTH || walk->budget == 0)
    return;

  GPtrArray *children = atspi_children(walk->bus, item, walk->cancel);

  for (guint i = 0; i < children->len; i++) {
    AtspiRef *child = g_ptr_array_index(children, i);
    const guint role = atspi_role(walk->bus, child, walk->cancel);

    if (role == WHELK_ATSPI_ROLE_MENU) {
      entry->submenu = whelk_menu_new(label);
      atspi_append_items(walk, entry->submenu, child, depth + 1);
      break;
    }
  }

  if (entry->submenu && entry->submenu->entries->len == 0) {
    whelk_menu_free(entry->submenu);
    entry->submenu = NULL;
  }

  g_ptr_array_unref(children);
}

static void atspi_append_items(AtspiWalk *walk, WhelkMenu *menu,
                               const AtspiRef *container, guint depth) {
  if (!walk || walk->budget == 0 || depth > WHELK_ATSPI_MAX_DEPTH)
    return;

  GPtrArray *children = atspi_children(walk->bus, container, walk->cancel);

  for (guint i = 0; i < children->len; i++) {
    AtspiRef *child = g_ptr_array_index(children, i);

    if (walk->budget == 0)
      break;

    walk->budget--;

    const guint role = atspi_role(walk->bus, child, walk->cancel);

    if (role == WHELK_ATSPI_ROLE_SEPARATOR) {
      if (menu->entries->len > 0)
        whelk_menu_add_separator(menu);
    } else if (role == WHELK_ATSPI_ROLE_MENU) {
      /* A Qt or GTK wrapper menu: its items belong to the menu that holds
       * it, not to an unnamed submenu. */
      atspi_append_items(walk, menu, child, depth + 1);
    } else if (role == WHELK_ATSPI_ROLE_MENU_ITEM ||
               role == WHELK_ATSPI_ROLE_CHECK_MENU_ITEM ||
               role == WHELK_ATSPI_ROLE_RADIO_MENU_ITEM) {
      atspi_append_item(walk, menu, child, depth + 1);
    }
  }

  g_ptr_array_unref(children);
}

static GPtrArray *atspi_menubar(AtspiWalk *walk, const AtspiRef *application) {
  GPtrArray *bar = whelk_menubar_new();
  GPtrArray *menu_bars = atspi_refs_new();

  if (!atspi_find_menu_bar(walk, application, 0, menu_bars) ||
      menu_bars->len == 0) {
    g_ptr_array_unref(menu_bars);
    return bar;
  }

  AtspiRef *root = g_ptr_array_index(menu_bars, 0);
  GPtrArray *tops = atspi_children(walk->bus, root, walk->cancel);

  for (guint i = 0; i < tops->len; i++) {
    AtspiRef *top = g_ptr_array_index(tops, i);
    g_autofree char *label = atspi_name(walk->bus, top, walk->cancel);

    if (!label || *label == '\0')
      continue;

    WhelkMenu *menu = whelk_menu_new(label);
    const guint role = atspi_role(walk->bus, top, walk->cancel);

    if (role == WHELK_ATSPI_ROLE_MENU) {
      atspi_append_items(walk, menu, top, 1);
    } else {
      atspi_append_item(walk, menu, top, 1);
    }

    if (menu->entries->len > 0)
      g_ptr_array_add(bar, menu);
    else
      whelk_menu_free(menu);
  }

  g_ptr_array_unref(tops);
  g_ptr_array_unref(menu_bars);

  return bar;
}

static GPtrArray *atspi_menubar_for_application(GDBusConnection *bus,
                                                const AtspiRef *application,
                                                WhelkChordStyle style,
                                                GCancellable *cancel) {
  AtspiWalk walk = {
      .bus = bus,
      .cancel = cancel,
      .style = style,
      .budget = WHELK_ATSPI_MAX_NODES,
  };

  return atspi_menubar(&walk, application);
}

GPtrArray *whelk_menubar_from_atspi_connection(GDBusConnection *bus,
                                               const char *app_name, gint pid,
                                               WhelkChordStyle style) {
  if (!bus)
    return whelk_menubar_new();

  g_autoptr(GCancellable) cancel = g_cancellable_new();
  AtspiRef *application = atspi_application(bus, app_name, pid, cancel);

  if (!application) {
    LOG_C_DEBUG("AT-SPI", "no application matches '%s' (pid %d)",
                app_name ? app_name : "", pid);
    return whelk_menubar_new();
  }

  GPtrArray *bar =
      atspi_menubar_for_application(bus, application, style, cancel);

  LOG_C_DEBUG("AT-SPI", "read %u menu(s) from %s", bar->len,
              app_name ? app_name : application->service);

  atspi_ref_free(application);

  return bar;
}

/* =========================================================================
 * THE LIVE WATCH
 * ========================================================================= */

struct _WhelkAtspiMenuWatch {
  gint refs;
  gint cancelled;
  GCancellable *cancel;
  GMainContext *context;
  GMainLoop *loop;
  GDBusConnection *bus;
  AtspiRef *application;
  char *app_name;
  gint pid;
  WhelkChordStyle style;
  WhelkAtspiMenuFunc changed;
  gpointer user_data;
  guint refresh_id;
  guint subscriptions[4];
};

typedef struct {
  WhelkAtspiMenuWatch *watch;
  GPtrArray *menus;
} AtspiDelivery;

static void atspi_watch_unref(WhelkAtspiMenuWatch *watch) {
  if (!watch)
    return;

  if (g_atomic_int_dec_and_test(&watch->refs)) {
    g_clear_object(&watch->bus);
    g_clear_object(&watch->cancel);
    atspi_ref_free(watch->application);
    g_free(watch->app_name);
    g_main_loop_unref(watch->loop);
    g_main_context_unref(watch->context);
    g_free(watch);
  }
}

static gboolean atspi_deliver(gpointer data) {
  AtspiDelivery *delivery = data;
  WhelkAtspiMenuWatch *watch = delivery->watch;

  if (!g_atomic_int_get(&watch->cancelled) && watch->changed)
    watch->changed(delivery->menus, watch->user_data);
  else
    g_ptr_array_unref(delivery->menus);

  atspi_watch_unref(watch);
  g_free(delivery);

  return G_SOURCE_REMOVE;
}

static void atspi_deliver_menus(WhelkAtspiMenuWatch *watch, GPtrArray *menus) {
  if (!menus)
    menus = whelk_menubar_new();

  if (g_atomic_int_get(&watch->cancelled)) {
    g_ptr_array_unref(menus);
    return;
  }

  AtspiDelivery *delivery = g_new0(AtspiDelivery, 1);

  delivery->watch = watch;
  delivery->menus = menus;
  g_atomic_int_inc(&watch->refs);
  g_main_context_invoke(NULL, atspi_deliver, delivery);
}

static GDBusConnection *atspi_connect(GCancellable *cancel) {
  g_autoptr(GError) error = NULL;
  g_autoptr(GDBusConnection) session =
      g_bus_get_sync(G_BUS_TYPE_SESSION, cancel, &error);

  if (!session) {
    if (error && !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_CANCELLED))
      LOG_C_DEBUG("AT-SPI", "no session bus: %s", error->message);
    return NULL;
  }

  g_autoptr(GVariant) reply = atspi_call(
      session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress",
      NULL, cancel);

  if (!reply) {
    LOG_C_DEBUG("AT-SPI", "the accessibility bus is not running");
    return NULL;
  }

  const char *address = NULL;

  g_variant_get(reply, "(&s)", &address);

  if (!address || *address == '\0')
    return NULL;

  g_autofree char *copy = g_strdup(address);

  GDBusConnection *a11y = g_dbus_connection_new_for_address_sync(
      copy,
      G_DBUS_CONNECTION_FLAGS_AUTHENTICATION_CLIENT |
          G_DBUS_CONNECTION_FLAGS_MESSAGE_BUS_CONNECTION,
      NULL, cancel, &error);

  if (!a11y && error &&
      !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_CANCELLED))
    LOG_C_DEBUG("AT-SPI", "could not connect to the accessibility bus: %s",
                error->message);

  return a11y;
}

/* Tell the registry which object events this watch cares about.  The
 * registration is for this application's unique bus name, so unrelated
 * applications do not wake the worker.  A registry that does not implement
 * the call is not fatal: some test buses and older stacks still emit the
 * events, and the subscription below is what actually receives them. */
static void atspi_register_events(GDBusConnection *bus,
                                  const AtspiRef *application,
                                  GCancellable *cancel, gboolean add) {
  if (!bus || !application)
    return;

  if (!add) {
    g_autoptr(GVariant) reply = atspi_call(
        bus, WHELK_ATSPI_ROOT_SERVICE, WHELK_ATSPI_REGISTRY_PATH,
        "org.a11y.atspi.Registry", "DeregisterEvent",
        g_variant_new("(ss)", WHELK_ATSPI_EVENT_REGISTRATION,
                      application->service),
        cancel);

    (void)reply;
    return;
  }

  GVariantBuilder properties;

  g_variant_builder_init(&properties, G_VARIANT_TYPE("as"));

  g_autoptr(GVariant) reply = atspi_call(
      bus, WHELK_ATSPI_ROOT_SERVICE, WHELK_ATSPI_REGISTRY_PATH,
      "org.a11y.atspi.Registry", "RegisterEvent",
      g_variant_new("(sass)", WHELK_ATSPI_EVENT_REGISTRATION, &properties,
                    application->service),
      cancel);

  (void)reply;
}

static gboolean atspi_watch_refresh(gpointer data) {
  WhelkAtspiMenuWatch *watch = data;

  watch->refresh_id = 0;

  if (g_atomic_int_get(&watch->cancelled))
    return G_SOURCE_REMOVE;

  GPtrArray *menus = atspi_menubar_for_application(
      watch->bus, watch->application, watch->style, watch->cancel);

  atspi_deliver_menus(watch, menus);

  return G_SOURCE_REMOVE;
}

/* One event is a menu change; a burst (a rebuild adds several children) is
 * one refresh.  The timer runs on the worker's context, where the bus
 * delivers its signals. */
static void on_atspi_object_event(GDBusConnection *connection,
                                  const char *sender, const char *path,
                                  const char *interface, const char *member,
                                  GVariant *parameters, gpointer user_data) {
  (void)connection;
  (void)sender;
  (void)path;
  (void)interface;
  (void)member;
  (void)parameters;

  WhelkAtspiMenuWatch *watch = user_data;

  if (g_atomic_int_get(&watch->cancelled) || watch->refresh_id != 0)
    return;

  GSource *source = g_timeout_source_new(WHELK_ATSPI_CHANGE_DELAY_MS);

  g_source_set_callback(source, atspi_watch_refresh, watch, NULL);
  watch->refresh_id = g_source_attach(source, watch->context);
  g_source_unref(source);
}

static gpointer atspi_watch_thread(gpointer data) {
  WhelkAtspiMenuWatch *watch = data;
  GDBusConnection *bus = NULL;
  AtspiRef *application = NULL;
  gboolean subscribed = FALSE;

  g_main_context_push_thread_default(watch->context);

  bus = atspi_connect(watch->cancel);

  if (!bus || g_atomic_int_get(&watch->cancelled))
    goto done;

  application = atspi_application(bus, watch->app_name, watch->pid,
                                  watch->cancel);

  if (!application)
    goto done;

  watch->bus = bus;
  watch->application = application;
  bus = NULL;
  application = NULL;

  atspi_register_events(watch->bus, watch->application, watch->cancel, TRUE);

  static const char *members[] = {
      "ChildrenChanged",
      "PropertyChange",
      "StateChanged",
      "VisibleDataChanged",
  };

  for (guint i = 0; i < G_N_ELEMENTS(members); i++)
    watch->subscriptions[i] = g_dbus_connection_signal_subscribe(
        watch->bus, watch->application->service, WHELK_ATSPI_EVENT_OBJECT,
        members[i], NULL, NULL, G_DBUS_SIGNAL_FLAGS_NONE,
        on_atspi_object_event, watch, NULL);

  subscribed = TRUE;

  /* Register and subscribe before the first delivery so a change racing the
   * first read is followed rather than lost. */
  GPtrArray *menus = atspi_menubar_for_application(
      watch->bus, watch->application, watch->style, watch->cancel);

  atspi_deliver_menus(watch, menus);

  if (!g_atomic_int_get(&watch->cancelled))
    g_main_loop_run(watch->loop);

done:
  if (subscribed)
    for (guint i = 0; i < G_N_ELEMENTS(watch->subscriptions); i++) {
      if (watch->subscriptions[i] != 0)
        g_dbus_connection_signal_unsubscribe(watch->bus,
                                             watch->subscriptions[i]);

      watch->subscriptions[i] = 0;
    }

  atspi_register_events(watch->bus, watch->application, watch->cancel, FALSE);
  g_clear_object(&watch->bus);
  atspi_ref_free(watch->application);
  watch->application = NULL;
  g_clear_object(&bus);
  atspi_ref_free(application);
  g_main_context_pop_thread_default(watch->context);
  atspi_watch_unref(watch);

  return NULL;
}

WhelkAtspiMenuWatch *whelk_atspi_menubar_watch_new(
    const char *app_name, gint pid, WhelkChordStyle style,
    WhelkAtspiMenuFunc changed, gpointer user_data) {
  WhelkAtspiMenuWatch *watch = g_new0(WhelkAtspiMenuWatch, 1);

  watch->refs = 1;
  watch->cancel = g_cancellable_new();
  watch->context = g_main_context_new();
  watch->loop = g_main_loop_new(watch->context, FALSE);
  watch->app_name = g_strdup(app_name);
  watch->pid = pid;
  watch->style = style;
  watch->changed = changed;
  watch->user_data = user_data;

  /* The worker owns a reference until it has stopped following; the caller
   * may free its own reference immediately after starting the watch. */
  g_atomic_int_inc(&watch->refs);

  GThread *thread = g_thread_new("whelk-atspi-watch", atspi_watch_thread,
                                 watch);

  g_thread_unref(thread);

  return watch;
}

void whelk_atspi_menubar_watch_free(WhelkAtspiMenuWatch *watch) {
  if (!watch)
    return;

  g_atomic_int_set(&watch->cancelled, 1);
  g_cancellable_cancel(watch->cancel);
  g_main_loop_quit(watch->loop);
  atspi_watch_unref(watch);
}

/* =========================================================================
 * ACTIVATION
 * ========================================================================= */

gboolean whelk_atspi_activate(const char *service, const char *path,
                              int action) {
  if (!service || !*service || !path || !*path)
    return FALSE;

  g_autoptr(GCancellable) cancel = g_cancellable_new();
  GDBusConnection *bus = atspi_connect(cancel);

  if (!bus)
    return FALSE;

  g_autoptr(GVariant) reply =
      atspi_call(bus, service, path, "org.a11y.atspi.Action", "DoAction",
                 g_variant_new("(i)", action), cancel);
  gboolean activated = FALSE;

  if (reply)
    g_variant_get(reply, "(b)", &activated);

  g_object_unref(bus);

  return activated;
}

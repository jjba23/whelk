// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/atspimenus.h"
#include "util/menutree.h"

#include <gio/gio.h>
#include <glib.h>
#include <string.h>

/* A tiny application on a private bus: an application accessible, a menu bar,
 * two menus, an item with a submenu and a disabled item.  The shape is the one
 * GTK and Qt publish; the producer cannot tell the difference. */

typedef struct {
  char *address;
  char *unique_name;
  gboolean registry_acquired;
  gboolean bus_acquired;
  gint registrations;
} FakeServer;

/* Changed by the live-menu test between two reads of the fake tree. */
static gboolean fake_extra_item = FALSE;

static const char introspection_xml[] =
    "<node>"
    "  <interface name='org.a11y.atspi.Accessible'>"
    "    <method name='GetRole'>"
    "      <arg type='u' direction='out'/>"
    "    </method>"
    "    <method name='GetChildren'>"
    "      <arg type='a(so)' direction='out'/>"
    "    </method>"
    "    <method name='GetState'>"
    "      <arg type='au' direction='out'/>"
    "    </method>"
    "    <property name='Name' type='s' access='read'/>"
    "  </interface>"
    "  <interface name='org.a11y.atspi.Action'>"
    "    <method name='GetKeyBinding'>"
    "      <arg type='i' direction='in'/>"
    "      <arg type='s' direction='out'/>"
    "    </method>"
    "    <method name='DoAction'>"
    "      <arg type='i' direction='in'/>"
    "      <arg type='b' direction='out'/>"
    "    </method>"
    "    <property name='NActions' type='i' access='read'/>"
    "  </interface>"
    "  <interface name='org.a11y.atspi.Registry'>"
    "    <method name='RegisterEvent'>"
    "      <arg type='s' direction='in'/>"
    "      <arg type='as' direction='in'/>"
    "      <arg type='s' direction='in'/>"
    "    </method>"
    "    <method name='DeregisterEvent'>"
    "      <arg type='s' direction='in'/>"
    "      <arg type='s' direction='in'/>"
    "    </method>"
    "  </interface>"
    "  <interface name='org.a11y.Bus'>"
    "    <method name='GetAddress'>"
    "      <arg type='s' direction='out'/>"
    "    </method>"
    "  </interface>"
    "</node>";

static const char *fake_role(const char *path) {
  if (g_strcmp0(path, "/menubar") == 0)
    return "menu-bar";

  if (g_strcmp0(path, "/file") == 0 || g_strcmp0(path, "/edit") == 0 ||
      g_strcmp0(path, "/recent_menu") == 0)
    return "menu";

  if (g_strcmp0(path, "/separator") == 0)
    return "separator";

  return "menu-item";
}

static guint role_number(const char *path) {
  if (g_strcmp0(fake_role(path), "menu-bar") == 0)
    return 34;

  if (g_strcmp0(fake_role(path), "menu") == 0)
    return 33;

  if (g_strcmp0(fake_role(path), "separator") == 0)
    return 50;

  if (g_strcmp0(path, "/paste") == 0)
    return 35;

  if (g_strcmp0(path, "/prefs") == 0)
    return 8;

  if (g_strcmp0(path, "/copy") == 0)
    return 45;

  return 35;
}

static const char *fake_name(const char *path) {
  if (g_strcmp0(path, "/org/a11y/atspi/accessible/root") == 0 || g_strcmp0(path, "/app") == 0)
    return "Fake App";

  if (g_strcmp0(path, "/menubar") == 0)
    return "Menu Bar";

  if (g_strcmp0(path, "/file") == 0)
    return "File";

  if (g_strcmp0(path, "/edit") == 0)
    return "Edit";

  if (g_strcmp0(path, "/quit") == 0)
    return "Quit";

  if (g_strcmp0(path, "/prefs") == 0)
    return "Preferences";

  if (g_strcmp0(path, "/recent") == 0)
    return "Recent";

  if (g_strcmp0(path, "/recent_menu") == 0)
    return "Recent";

  if (g_strcmp0(path, "/document") == 0)
    return "Document";

  if (g_strcmp0(path, "/copy") == 0)
    return "Copy";

  if (g_strcmp0(path, "/paste") == 0)
    return "Paste";

  if (g_strcmp0(path, "/export") == 0)
    return "Export";

  return "";
}

static const char *fake_binding(const char *path) {
  if (g_strcmp0(path, "/quit") == 0)
    return "<Primary>q";

  if (g_strcmp0(path, "/copy") == 0)
    return "Ctrl+C";

  if (g_strcmp0(path, "/export") == 0)
    return "Ctrl+E";

  return "";
}

static gboolean fake_enabled(const char *path) {
  return g_strcmp0(path, "/paste") != 0;
}

static const char **fake_children(const char *path) {
  static const char *root[] = {"/app", NULL};
  static const char *app[] = {"/menubar", NULL};
  static const char *menubar[] = {"/file", "/edit", NULL};
  static const char *file[] = {"/quit", "/separator", "/recent", "/prefs", NULL};
  static const char *file_extra[] = {"/quit", "/separator", "/recent",
                                     "/prefs", "/export", NULL};
  static const char *edit[] = {"/copy", "/paste", NULL};
  static const char *recent[] = {"/recent_menu", NULL};
  static const char *recent_menu[] = {"/document", NULL};

  if (g_strcmp0(path, "/org/a11y/atspi/accessible/root") == 0)
    return root;

  if (g_strcmp0(path, "/app") == 0)
    return app;

  if (g_strcmp0(path, "/menubar") == 0)
    return menubar;

  if (g_strcmp0(path, "/file") == 0)
    return fake_extra_item ? file_extra : file;

  if (g_strcmp0(path, "/edit") == 0)
    return edit;

  if (g_strcmp0(path, "/recent") == 0)
    return recent;

  if (g_strcmp0(path, "/recent_menu") == 0)
    return recent_menu;

  return NULL;
}

static GVariant *children_value(const FakeServer *server, const char *path) {
  GVariantBuilder builder;

  g_variant_builder_init(&builder, G_VARIANT_TYPE("a(so)"));

  const char **children = fake_children(path);

  for (guint i = 0; children && children[i]; i++)
    g_variant_builder_add(&builder, "(so)", server->unique_name, children[i]);

  return g_variant_new("(a(so))", &builder);
}

static void on_method_call(GDBusConnection *connection, const char *sender,
                           const char *object_path, const char *interface_name,
                           const char *method_name, GVariant *parameters,
                           GDBusMethodInvocation *invocation,
                           gpointer user_data) {
  (void)connection;
  (void)sender;
  (void)parameters;

  FakeServer *server = user_data;

  if (g_strcmp0(interface_name, "org.a11y.atspi.Accessible") == 0) {
    if (g_strcmp0(method_name, "GetRole") == 0) {
      g_dbus_method_invocation_return_value(
          invocation, g_variant_new("(u)", role_number(object_path)));
      return;
    }

    if (g_strcmp0(method_name, "GetChildren") == 0) {
      g_dbus_method_invocation_return_value(
          invocation, children_value(server, object_path));
      return;
    }

    if (g_strcmp0(method_name, "GetState") == 0) {
      GVariantBuilder states;

      g_variant_builder_init(&states, G_VARIANT_TYPE("au"));

      /* enabled and sensitive, unless the fake item is meant to be disabled. */
      g_variant_builder_add(&states, "u",
                            fake_enabled(object_path)
                                ? ((1u << 8) | (1u << 24))
                                : 0u);
      g_dbus_method_invocation_return_value(invocation,
                                            g_variant_new("(au)", &states));
      return;
    }
  }

  if (g_strcmp0(interface_name, "org.a11y.atspi.Action") == 0) {
    if (g_strcmp0(method_name, "GetKeyBinding") == 0) {
      g_dbus_method_invocation_return_value(
          invocation, g_variant_new("(s)", fake_binding(object_path)));
      return;
    }

    if (g_strcmp0(method_name, "DoAction") == 0) {
      g_dbus_method_invocation_return_value(invocation,
                                            g_variant_new("(b)", TRUE));
      return;
    }
  }

  if (g_strcmp0(interface_name, "org.a11y.atspi.Registry") == 0) {
    if (g_strcmp0(method_name, "RegisterEvent") == 0) {
      server->registrations++;
      g_dbus_method_invocation_return_value(invocation, NULL);
      return;
    }

    if (g_strcmp0(method_name, "DeregisterEvent") == 0) {
      g_dbus_method_invocation_return_value(invocation, NULL);
      return;
    }
  }

  if (g_strcmp0(interface_name, "org.a11y.Bus") == 0 &&
      g_strcmp0(method_name, "GetAddress") == 0) {
    g_dbus_method_invocation_return_value(
        invocation, g_variant_new("(s)", server->address));
    return;
  }

  g_dbus_method_invocation_return_error(invocation, G_IO_ERROR,
                                        G_IO_ERROR_NOT_SUPPORTED,
                                        "no such method");
}

static GVariant *on_get_property(GDBusConnection *connection, const char *sender,
                                 const char *object_path,
                                 const char *interface_name,
                                 const char *property_name, GError **error,
                                 gpointer user_data) {
  (void)connection;
  (void)sender;
  (void)user_data;
  (void)error;

  if (g_strcmp0(interface_name, "org.a11y.atspi.Accessible") == 0 &&
      g_strcmp0(property_name, "Name") == 0)
    return g_variant_new_string(fake_name(object_path));

  if (g_strcmp0(interface_name, "org.a11y.atspi.Action") == 0 &&
      g_strcmp0(property_name, "NActions") == 0)
    return g_variant_new_int32(1);

  return NULL;
}

static const GDBusInterfaceVTable vtable = {
    .method_call = on_method_call,
    .get_property = on_get_property,
    .set_property = NULL,
};

static void on_name_acquired(GDBusConnection *connection, const char *name,
                             gpointer user_data) {
  (void)connection;
  FakeServer *server = user_data;

  if (g_strcmp0(name, "org.a11y.atspi.Registry") == 0)
    server->registry_acquired = TRUE;
  else if (g_strcmp0(name, "org.a11y.Bus") == 0)
    server->bus_acquired = TRUE;
}

static FakeServer *fake_server_start(GDBusConnection **out_connection) {
  g_autoptr(GError) error = NULL;
  GTestDBus *dbus = g_test_dbus_new(G_TEST_DBUS_NONE);

  g_test_dbus_up(dbus);

  g_autofree char *address = g_strdup(g_test_dbus_get_bus_address(dbus));
  GDBusConnection *connection = g_dbus_connection_new_for_address_sync(
      address,
      G_DBUS_CONNECTION_FLAGS_AUTHENTICATION_CLIENT |
          G_DBUS_CONNECTION_FLAGS_MESSAGE_BUS_CONNECTION,
      NULL, NULL, &error);

  g_assert_no_error(error);
  g_assert_nonnull(connection);

  FakeServer *server = g_new0(FakeServer, 1);

  server->address = g_strdup(address);
  server->unique_name = g_strdup(g_dbus_connection_get_unique_name(connection));

  /* The watch asks the session bus for the accessibility-bus address, then
   * connects to that address.  Point both at this test bus. */
  g_setenv("DBUS_SESSION_BUS_ADDRESS", address, TRUE);

  g_bus_own_name_on_connection(connection, "org.a11y.atspi.Registry",
                               G_BUS_NAME_OWNER_FLAGS_NONE, on_name_acquired,
                               NULL, server, NULL);
  g_bus_own_name_on_connection(connection, "org.a11y.Bus",
                               G_BUS_NAME_OWNER_FLAGS_NONE, on_name_acquired,
                               NULL, server, NULL);

  while (!server->registry_acquired || !server->bus_acquired)
    g_main_context_iteration(NULL, TRUE);

  g_autoptr(GDBusNodeInfo) node =
      g_dbus_node_info_new_for_xml(introspection_xml, &error);

  g_assert_no_error(error);

  GDBusInterfaceInfo *accessible =
      g_dbus_node_info_lookup_interface(node, "org.a11y.atspi.Accessible");
  GDBusInterfaceInfo *action =
      g_dbus_node_info_lookup_interface(node, "org.a11y.atspi.Action");
  GDBusInterfaceInfo *registry =
      g_dbus_node_info_lookup_interface(node, "org.a11y.atspi.Registry");
  GDBusInterfaceInfo *a11y_bus =
      g_dbus_node_info_lookup_interface(node, "org.a11y.Bus");
  static const char *paths[] = {"/org/a11y/atspi/accessible/root",        "/app",
                                "/menubar",     "/file",
                                "/edit",        "/recent",
                                "/recent_menu", "/quit",
                                "/separator",   "/prefs",
                                "/document",    "/copy",
                                "/paste",       "/export",
                                NULL};

  for (guint i = 0; paths[i]; i++) {
    g_dbus_connection_register_object(connection, paths[i], accessible, &vtable,
                                      server, NULL, &error);
    g_assert_no_error(error);
    g_dbus_connection_register_object(connection, paths[i], action, &vtable,
                                      server, NULL, &error);
    g_assert_no_error(error);
  }

  g_dbus_connection_register_object(connection, "/org/a11y/atspi/registry",
                                    registry, &vtable, server, NULL, &error);
  g_assert_no_error(error);
  g_dbus_connection_register_object(connection, "/org/a11y/bus", a11y_bus,
                                    &vtable, server, NULL, &error);
  g_assert_no_error(error);

  *out_connection = connection;
  return server;
}

typedef struct {
  GDBusConnection *connection;
  const char *app_name;
  GPtrArray *menus;
  gint done;
} ReadJob;

static gpointer read_thread(gpointer data) {
  ReadJob *job = data;

  job->menus = whelk_menubar_from_atspi_connection(
      job->connection, job->app_name, 0, WHELK_CHORD_CUA);
  g_atomic_int_set(&job->done, 1);
  return NULL;
}

/* The producer makes synchronous D-Bus calls, so it must not run on the same
 * main context as the fake application that answers them.  Production runs it
 * on a worker thread for the same reason: the UI has to keep drawing. */
static GPtrArray *read_menus(GDBusConnection *connection, const char *app_name) {
  ReadJob job = {
      .connection = connection,
      .app_name = app_name,
  };
  GThread *thread = g_thread_new("fake-atspi-reader", read_thread, &job);

  while (!g_atomic_int_get(&job.done)) {
    g_main_context_iteration(NULL, FALSE);
    g_usleep(1000);
  }

  g_thread_join(thread);
  return job.menus;
}

static void test_reads_menu_bar(void) {
  GDBusConnection *connection = NULL;
  FakeServer *server = fake_server_start(&connection);

  g_autoptr(GPtrArray) menus = read_menus(connection, "Fake App");

  g_assert_cmpuint(menus->len, ==, 2);

  WhelkMenu *file = g_ptr_array_index(menus, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 4);

  WhelkMenuEntry *quit = g_ptr_array_index(file->entries, 0);

  g_assert_cmpstr(quit->label, ==, "Quit");
  g_assert_cmpstr(quit->accel, ==, "Ctrl+Q");
  g_assert_cmpstr(quit->atspi_path, ==, "/quit");
  g_assert_cmpstr(quit->atspi_service, ==, server->unique_name);

  WhelkMenuEntry *separator = g_ptr_array_index(file->entries, 1);

  g_assert_cmpint(separator->kind, ==, WHELK_MENU_SEPARATOR);

  WhelkMenuEntry *recent = g_ptr_array_index(file->entries, 2);

  g_assert_nonnull(recent->submenu);
  g_assert_cmpuint(recent->submenu->entries->len, ==, 1);
  g_assert_cmpstr(
      ((WhelkMenuEntry *)g_ptr_array_index(recent->submenu->entries, 0))->label,
      ==, "Document");

  WhelkMenu *edit = g_ptr_array_index(menus, 1);

  g_assert_cmpstr(edit->label, ==, "Edit");
  g_assert_cmpuint(edit->entries->len, ==, 2);

  WhelkMenuEntry *copy = g_ptr_array_index(edit->entries, 0);
  WhelkMenuEntry *paste = g_ptr_array_index(edit->entries, 1);

  g_assert_cmpstr(copy->accel, ==, "Ctrl+C");
  g_assert_true(copy->enabled);
  g_assert_false(paste->enabled);

  g_object_unref(connection);
  g_free(server->address);
  g_free(server->unique_name);
  g_free(server);
}

static void test_unknown_application_is_empty(void) {
  GDBusConnection *connection = NULL;
  FakeServer *server = fake_server_start(&connection);

  g_autoptr(GPtrArray) menus = read_menus(connection, "Nothing Here");

  g_assert_cmpuint(menus->len, ==, 0);

  g_object_unref(connection);
  g_free(server->address);
  g_free(server->unique_name);
  g_free(server);
}

typedef struct {
  GMainLoop *loop;
  guint calls;
  guint target;
  gboolean timed_out;
  guint timeout_id;
  GPtrArray *menus;
} WatchJob;

static void on_watch_menus(GPtrArray *menus, gpointer user_data) {
  WatchJob *job = user_data;

  g_clear_pointer(&job->menus, g_ptr_array_unref);
  job->menus = menus;
  job->calls++;

  if (job->calls >= job->target)
    g_main_loop_quit(job->loop);
}

static gboolean on_watch_timeout(gpointer user_data) {
  WatchJob *job = user_data;

  job->timed_out = TRUE;
  job->timeout_id = 0;
  g_main_loop_quit(job->loop);
  return G_SOURCE_REMOVE;
}

static void wait_for_watch(WatchJob *job, guint target) {
  job->target = target;

  if (job->calls >= target)
    return;

  GSource *timeout = g_timeout_source_new_seconds(5);

  g_source_set_callback(timeout, on_watch_timeout, job, NULL);
  job->timeout_id = g_source_attach(timeout, NULL);
  g_source_unref(timeout);
  g_main_loop_run(job->loop);

  if (job->timeout_id != 0) {
    g_source_remove(job->timeout_id);
    job->timeout_id = 0;
  }

  g_assert_false(job->timed_out);
}

static void emit_children_changed(GDBusConnection *connection,
                                  const FakeServer *server) {
  GVariantBuilder properties;

  g_variant_builder_init(&properties, G_VARIANT_TYPE("a{sv}"));
  g_autoptr(GError) error = NULL;
  gboolean sent = g_dbus_connection_emit_signal(
      connection, NULL, "/menubar", "org.a11y.atspi.Event.Object",
      "ChildrenChanged",
      g_variant_new("(siiva{sv})", "insert", 4, 0,
                    g_variant_new("(so)", server->unique_name, "/export"),
                    &properties),
      &error);

  g_assert_no_error(error);
  g_assert_true(sent);
}

static void test_follows_menu_changes(void) {
  GDBusConnection *connection = NULL;
  FakeServer *server = fake_server_start(&connection);
  WatchJob job = {
      .loop = g_main_loop_new(NULL, FALSE),
  };

  fake_extra_item = FALSE;

  WhelkAtspiMenuWatch *watch = whelk_atspi_menubar_watch_new(
      "Fake App", 0, WHELK_CHORD_CUA, on_watch_menus, &job);

  wait_for_watch(&job, 1);

  g_assert_cmpuint(job.menus->len, ==, 2);

  WhelkMenu *file = g_ptr_array_index(job.menus, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 4);

  /* The fake application rebuilds File; its ChildrenChanged must make the
   * watch walk the tree again and deliver the new row. */
  fake_extra_item = TRUE;
  emit_children_changed(connection, server);
  wait_for_watch(&job, 2);

  g_assert_cmpint(server->registrations, >, 0);
  g_assert_cmpuint(job.menus->len, ==, 2);

  file = g_ptr_array_index(job.menus, 0);
  g_assert_cmpuint(file->entries->len, ==, 5);

  WhelkMenuEntry *export = g_ptr_array_index(file->entries, 4);

  g_assert_cmpstr(export->label, ==, "Export");
  g_assert_cmpstr(export->accel, ==, "Ctrl+E");

  whelk_atspi_menubar_watch_free(watch);
  g_clear_pointer(&job.menus, g_ptr_array_unref);
  g_main_loop_unref(job.loop);
  fake_extra_item = FALSE;
  g_object_unref(connection);
  g_free(server->address);
  g_free(server->unique_name);
  g_free(server);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/atspi/menubar", test_reads_menu_bar);
  g_test_add_func("/atspi/unknown", test_unknown_application_is_empty);
  g_test_add_func("/atspi/live-menu-changes", test_follows_menu_changes);

  return g_test_run();
}

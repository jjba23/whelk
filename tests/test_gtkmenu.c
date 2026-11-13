// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/gtkmenu.h"

#include <gio/gio.h>
#include <glib.h>

/* An org.gtk.Menus service on a private bus: Start answers a menubar, Changed
 * says one of its menus moved, and the watcher has to ask Start again instead
 * of keeping the tree it drew. */

static const char menu_xml[] =
    "<node>"
    "  <interface name='org.gtk.Menus'>"
    "    <method name='Start'>"
    "      <arg type='au' direction='in'/>"
    "      <arg type='a(uuaa{sv})' direction='out'/>"
    "    </method>"
    "    <method name='End'>"
    "      <arg type='au' direction='in'/>"
    "    </method>"
    "    <signal name='Changed'>"
    "      <arg type='a(uuuuaa{sv})'/>"
    "    </signal>"
    "  </interface>"
    "</node>";

typedef struct {
  char *name;
  GDBusConnection *bus;
  guint registration;
  guint start_calls;
  guint end_calls;
  guint revision;
} FakeMenus;

typedef struct {
  GMainLoop *loop;
  guint calls;
  guint expected;
  guint invalidated;
  char *first_label;
  char *last_label;
  gboolean timed_out;
} Capture;

static GDBusInterfaceInfo *menus_interface(void) {
  static GDBusNodeInfo *node = NULL;

  if (!node) {
    g_autoptr(GError) error = NULL;

    node = g_dbus_node_info_new_for_xml(menu_xml, &error);
    g_assert_no_error(error);
  }

  return g_dbus_node_info_lookup_interface(node, "org.gtk.Menus");
}

static GVariant *fake_start_reply(FakeMenus *fake) {
  GVariantBuilder menus;
  GVariantBuilder root_items;
  GVariantBuilder file_attrs;
  GVariantBuilder open_attrs;
  GVariantBuilder open_items;

  g_variant_builder_init(&menus, G_VARIANT_TYPE("a(uuaa{sv})"));
  g_variant_builder_init(&root_items, G_VARIANT_TYPE("aa{sv}"));

  g_variant_builder_init(&file_attrs, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&file_attrs, "{sv}", "label",
                        g_variant_new_string("File"));
  g_variant_builder_add(&file_attrs, "{sv}", ":submenu",
                        g_variant_new("(uu)", 0, 1));
  g_variant_builder_add_value(&root_items,
                              g_variant_builder_end(&file_attrs));

  g_variant_builder_add(&menus, "(uu@aa{sv})", 0, 0,
                        g_variant_builder_end(&root_items));

  g_variant_builder_init(&open_items, G_VARIANT_TYPE("aa{sv}"));
  g_variant_builder_init(&open_attrs, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&open_attrs, "{sv}", "label",
                        g_variant_new_string(fake->revision == 0
                                                 ? "Open"
                                                 : "Open Recent"));
  g_variant_builder_add(&open_attrs, "{sv}", "accel",
                        g_variant_new_string("<Ctrl>O"));
  g_variant_builder_add_value(&open_items,
                              g_variant_builder_end(&open_attrs));

  g_variant_builder_add(&menus, "(uu@aa{sv})", 0, 1,
                        g_variant_builder_end(&open_items));

  return g_variant_new("(@a(uuaa{sv}))", g_variant_builder_end(&menus));
}

static void on_method_call(GDBusConnection *connection, const char *sender,
                           const char *object_path,
                           const char *interface_name,
                           const char *method_name, GVariant *parameters,
                           GDBusMethodInvocation *invocation,
                           gpointer user_data) {
  (void)connection;
  (void)sender;
  (void)object_path;
  (void)interface_name;
  (void)parameters;

  FakeMenus *fake = user_data;

  if (g_strcmp0(method_name, "End") == 0) {
    fake->end_calls++;
    g_dbus_method_invocation_return_value(invocation, NULL);
    return;
  }

  if (g_strcmp0(method_name, "Start") != 0)
    return;

  fake->start_calls++;
  g_dbus_method_invocation_return_value(invocation, fake_start_reply(fake));
}

static const GDBusInterfaceVTable menus_vtable = {
    .method_call = on_method_call,
    .get_property = NULL,
    .set_property = NULL,
};

static FakeMenus *fake_menus_new(const char *name) {
  FakeMenus *fake = g_new0(FakeMenus, 1);

  fake->name = g_strdup(name);

  g_autoptr(GError) error = NULL;

  fake->bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);
  g_assert_no_error(error);

  GVariant *reply = g_dbus_connection_call_sync(
      fake->bus, "org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "RequestName",
      g_variant_new("(su)", fake->name, 0), G_VARIANT_TYPE("(u)"),
      G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);

  g_assert_no_error(error);
  g_variant_unref(reply);

  fake->registration = g_dbus_connection_register_object(
      fake->bus, "/org/example/App", menus_interface(), &menus_vtable, fake,
      NULL, &error);

  g_assert_no_error(error);
  g_assert_cmpuint(fake->registration, >, 0);

  return fake;
}

static void fake_menus_free(FakeMenus *fake) {
  g_dbus_connection_unregister_object(fake->bus, fake->registration);
  g_clear_object(&fake->bus);
  g_free(fake->name);
  g_free(fake);
}

static void on_changed(GPtrArray *menus, gpointer user_data) {
  Capture *capture = user_data;

  capture->calls++;

  if (menus->len > 0) {
    WhelkMenu *file = g_ptr_array_index(menus, 0);

    if (file->entries->len > 0) {
      WhelkMenuEntry *entry = g_ptr_array_index(file->entries, 0);

      if (!capture->first_label)
        capture->first_label = g_strdup(entry->label);

      g_free(capture->last_label);
      capture->last_label = g_strdup(entry->label);
    }
  }

  g_ptr_array_unref(menus);

  if (capture->calls >= capture->expected)
    g_main_loop_quit(capture->loop);
}

static void on_invalidated(gpointer user_data) {
  Capture *capture = user_data;

  capture->invalidated++;
}

static gboolean on_watchdog(gpointer user_data) {
  Capture *capture = user_data;

  capture->timed_out = TRUE;
  g_main_loop_quit(capture->loop);

  return G_SOURCE_REMOVE;
}

static void run_until_captured(Capture *capture) {
  const guint watchdog = g_timeout_add(3000, on_watchdog, capture);

  g_main_loop_run(capture->loop);

  if (!capture->timed_out)
    g_source_remove(watchdog);
}

static void emit_changed(FakeMenus *fake) {
  GVariant *changes =
      g_variant_new_array(G_VARIANT_TYPE("(uuuuaa{sv})"), NULL, 0);

  g_dbus_connection_emit_signal(
      fake->bus, NULL, "/org/example/App", "org.gtk.Menus", "Changed",
      g_variant_new("(@a(uuuuaa{sv}))", changes), NULL);
}

static void test_changed_rereads_the_tree(void) {
  FakeMenus *fake = fake_menus_new("org.example.GtkMenus");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE)};

  /* Start's first reply is the tree the probe already read, and the second is
   * the one after Changed. */
  capture.expected = 1;
  WhelkGtkMenuWatch *watch = whelk_gtkmenu_watch_new(
      fake->bus, fake->name, "/org/example/App", WHELK_CHORD_CUA, on_changed,
      on_invalidated, &capture);

  g_assert_nonnull(watch);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 1);
  g_assert_cmpstr(capture.first_label, ==, "Open");
  g_assert_cmpstr(capture.last_label, ==, "Open");
  g_assert_cmpuint(fake->start_calls, ==, 1);

  /* The menu changes while its popover is open: close it, then ask again. */
  capture.expected = 2;
  fake->revision = 1;
  emit_changed(fake);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 2);
  g_assert_cmpuint(fake->start_calls, ==, 2);
  g_assert_cmpuint(capture.invalidated, ==, 1);
  /* The second Start answered with the new entry, not a cached tree. */
  g_assert_cmpstr(capture.last_label, ==, "Open Recent");

  whelk_gtkmenu_watch_free(watch);

  /* Ending the watch releases the group it subscribed. */
  g_assert_cmpuint(fake->end_calls, >=, 1);

  g_free(capture.first_label);
  g_free(capture.last_label);
  g_main_loop_unref(capture.loop);
  fake_menus_free(fake);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  if (!g_find_program_in_path("dbus-daemon")) {
    g_test_skip("dbus-daemon is not installed");
    return 0;
  }

  GTestDBus *bus = g_test_dbus_new(G_TEST_DBUS_NONE);

  g_test_dbus_up(bus);
  g_test_add_func("/gtkmenu/changed-rereads", test_changed_rereads_the_tree);

  const int status = g_test_run();

  g_test_dbus_down(bus);
  g_object_unref(bus);

  return status;
}

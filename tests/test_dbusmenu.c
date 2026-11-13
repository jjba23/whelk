// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/dbusmenu.h"

#include <gio/gio.h>
#include <glib.h>

/* A live DBusMenu, on a private bus: the watcher has to notice a changed
 * layout, read the tree again, and hand back ids from the new tree rather than
 * the ones it first saw. */

static const char menu_xml[] =
    "<node>"
    "  <interface name='com.canonical.dbusmenu'>"
    "    <method name='GetLayout'>"
    "      <arg type='i' direction='in'/>"
    "      <arg type='i' direction='in'/>"
    "      <arg type='u' direction='out'/>"
    "      <arg type='(ia{sv}av)' direction='out'/>"
    "    </method>"
    "    <method name='AboutToShow'>"
    "      <arg type='i' direction='in'/>"
    "      <arg type='b' direction='out'/>"
    "    </method>"
    "    <signal name='LayoutUpdated'>"
    "      <arg type='u'/>"
    "      <arg type='i'/>"
    "    </signal>"
    "    <signal name='ItemsPropertiesUpdated'>"
    "      <arg type='a(ia{sv})'/>"
    "      <arg type='a(ias)'/>"
    "    </signal>"
    "  </interface>"
    "</node>";

typedef struct {
  char *name;
  GDBusConnection *bus;
  guint registration;
  guint revision;
  guint get_layout_calls;
  guint about_to_show_calls;
  gboolean layout_without_show;
  gboolean context;
} FakeMenu;

typedef struct {
  GMainLoop *loop;
  guint calls;
  guint expected;
  guint last_id;
  guint invalidated;
  GPtrArray *menus;
  gboolean timed_out;
} Capture;

static GDBusInterfaceInfo *menu_interface(void) {
  static GDBusNodeInfo *node = NULL;

  if (!node) {
    g_autoptr(GError) error = NULL;

    node = g_dbus_node_info_new_for_xml(menu_xml, &error);
    g_assert_no_error(error);
  }

  return g_dbus_node_info_lookup_interface(node, "com.canonical.dbusmenu");
}

/* A menubar with one menu and one entry; the entry's id changes with the
 * revision, exactly the way a real menu's ids change when it is rebuilt. */
static GVariant *fake_layout(FakeMenu *fake) {
  GVariantBuilder item_props;
  GVariantBuilder item_children;
  GVariantBuilder file_props;
  GVariantBuilder file_children;
  GVariantBuilder root_props;
  GVariantBuilder root_children;

  g_variant_builder_init(&item_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&item_props, "{sv}", "label",
                        g_variant_new_string("Open"));
  g_variant_builder_init(&item_children, G_VARIANT_TYPE("av"));

  g_variant_builder_init(&file_children, G_VARIANT_TYPE("av"));
  g_variant_builder_add_value(
      &file_children,
      g_variant_new_variant(g_variant_new(
          "(i@a{sv}@av)", (gint)(100 + fake->revision * 10),
          g_variant_builder_end(&item_props),
          g_variant_builder_end(&item_children))));

  g_variant_builder_init(&file_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&file_props, "{sv}", "label",
                        g_variant_new_string("File"));
  g_variant_builder_add(&file_props, "{sv}", "children-display",
                        g_variant_new_string("submenu"));

  g_variant_builder_init(&root_children, G_VARIANT_TYPE("av"));
  g_variant_builder_add_value(
      &root_children,
      g_variant_new_variant(g_variant_new(
          "(i@a{sv}@av)", 1, g_variant_builder_end(&file_props),
          g_variant_builder_end(&file_children))));

  g_variant_builder_init(&root_props, G_VARIANT_TYPE("a{sv}"));

  return g_variant_new("(i@a{sv}@av)", 0,
                       g_variant_builder_end(&root_props),
                       g_variant_builder_end(&root_children));
}

/* A tray context menu: the root's children are the entries, not menus. */
static GVariant *fake_context_layout(void) {
  GVariantBuilder children;
  GVariantBuilder root_props;

  g_variant_builder_init(&children, G_VARIANT_TYPE("av"));

  for (gint i = 0; i < 3; i++) {
    GVariantBuilder props;
    GVariantBuilder kids;
    gint32 id = i == 2 ? 202 : 200;
    const char *label = i == 0 ? "Show" : (i == 2 ? "Quit" : NULL);

    g_variant_builder_init(&props, G_VARIANT_TYPE("a{sv}"));
    g_variant_builder_init(&kids, G_VARIANT_TYPE("av"));

    if (i == 1)
      g_variant_builder_add(&props, "{sv}", "type",
                            g_variant_new_string("separator"));
    else
      g_variant_builder_add(&props, "{sv}", "label",
                            g_variant_new_string(label));

    g_variant_builder_add_value(
        &children,
        g_variant_new_variant(g_variant_new(
            "(i@a{sv}@av)", id, g_variant_builder_end(&props),
            g_variant_builder_end(&kids))));
  }

  g_variant_builder_init(&root_props, G_VARIANT_TYPE("a{sv}"));

  return g_variant_new("(i@a{sv}@av)", 0,
                       g_variant_builder_end(&root_props),
                       g_variant_builder_end(&children));
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

  FakeMenu *fake = user_data;

  if (g_strcmp0(method_name, "AboutToShow") == 0) {
    fake->about_to_show_calls++;
    g_dbus_method_invocation_return_value(invocation,
                                          g_variant_new("(b)", TRUE));
    return;
  }

  if (g_strcmp0(method_name, "GetLayout") != 0)
    return;

  if (fake->about_to_show_calls == 0)
    fake->layout_without_show = TRUE;

  fake->get_layout_calls++;
  g_dbus_method_invocation_return_value(
      invocation,
      g_variant_new("(u@(ia{sv}av))", fake->revision,
                    fake->context ? fake_context_layout() : fake_layout(fake)));
}

static const GDBusInterfaceVTable menu_vtable = {
    .method_call = on_method_call,
    .get_property = NULL,
    .set_property = NULL,
};

static FakeMenu *fake_menu_new(const char *name) {
  FakeMenu *fake = g_new0(FakeMenu, 1);

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
      fake->bus, "/MenuBar/1", menu_interface(), &menu_vtable, fake, NULL,
      &error);

  g_assert_no_error(error);
  g_assert_cmpuint(fake->registration, >, 0);

  return fake;
}

static void fake_menu_free(FakeMenu *fake) {
  if (!fake)
    return;

  g_dbus_connection_unregister_object(fake->bus, fake->registration);
  g_clear_object(&fake->bus);
  g_free(fake->name);
  g_free(fake);
}

static void on_changed(GPtrArray *menus, gpointer user_data) {
  Capture *capture = user_data;

  g_clear_pointer(&capture->menus, g_ptr_array_unref);
  capture->menus = menus;
  capture->calls++;

  if (menus->len > 0) {
    WhelkMenu *menu = g_ptr_array_index(menus, 0);

    if (menu->entries->len > 0) {
      WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, 0);

      capture->last_id = (guint)entry->dbus_item;
    }
  }

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

  /* The watchdog removes itself when it fires; removing it again is a GLib
   * critical, and a critical in a test is fatal. */
  if (!capture->timed_out)
    g_source_remove(watchdog);
}

static void emit_layout_updated(FakeMenu *fake, gint parent) {
  g_dbus_connection_emit_signal(
      fake->bus, NULL, "/MenuBar/1", "com.canonical.dbusmenu",
      "LayoutUpdated", g_variant_new("(ui)", fake->revision, parent), NULL);
}

static void emit_properties_updated(FakeMenu *fake) {
  GVariant *updated =
      g_variant_new_array(G_VARIANT_TYPE("(ia{sv})"), NULL, 0);
  GVariant *removed = g_variant_new_array(G_VARIANT_TYPE("(ias)"), NULL, 0);

  g_dbus_connection_emit_signal(
      fake->bus, NULL, "/MenuBar/1", "com.canonical.dbusmenu",
      "ItemsPropertiesUpdated",
      g_variant_new("(@a(ia{sv})@a(ias))", updated, removed), NULL);
}

static void test_layout_updated_reads_new_ids(void) {
  FakeMenu *fake = fake_menu_new("org.example.MenuLayout");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE), .expected = 2};
  WhelkDBusMenuWatch *watch = whelk_dbusmenu_watch_new(
      fake->bus, fake->name, "/MenuBar/1", WHELK_CHORD_CUA, on_changed,
      on_invalidated, &capture);

  g_assert_nonnull(watch);

  /* The first tree's ids are not the second's: a click has to carry the id
   * from the tree it was drawn from, which is why the whole tree is re-read. */
  capture.expected = 1;
  fake->revision = 0;
  emit_layout_updated(fake, 0);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 1);
  g_assert_cmpuint(fake->get_layout_calls, ==, 1);
  g_assert_cmpuint(capture.last_id, ==, 100);
  /* Told before the read: what was open is closed on the signal itself. */
  g_assert_cmpuint(capture.invalidated, ==, 1);

  capture.expected = 2;
  fake->revision = 1;
  emit_layout_updated(fake, 0);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 2);
  g_assert_cmpuint(fake->get_layout_calls, ==, 2);
  g_assert_cmpuint(capture.last_id, ==, 110);
  g_assert_cmpuint(capture.invalidated, ==, 2);

  whelk_dbusmenu_watch_free(watch);
  g_clear_pointer(&capture.menus, g_ptr_array_unref);
  g_main_loop_unref(capture.loop);
  fake_menu_free(fake);
}

static void test_items_properties_updated_reads_the_tree(void) {
  FakeMenu *fake = fake_menu_new("org.example.MenuProperties");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE), .expected = 1};
  WhelkDBusMenuWatch *watch = whelk_dbusmenu_watch_new(
      fake->bus, fake->name, "/MenuBar/1", WHELK_CHORD_CUA, on_changed,
      on_invalidated, &capture);

  g_assert_nonnull(watch);

  fake->revision = 2;
  emit_properties_updated(fake);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 1);
  g_assert_cmpuint(fake->get_layout_calls, ==, 1);
  g_assert_cmpuint(capture.last_id, ==, 120);
  /* Property changes do not move ids, so nothing is closed early. */
  g_assert_cmpuint(capture.invalidated, ==, 0);

  whelk_dbusmenu_watch_free(watch);
  g_clear_pointer(&capture.menus, g_ptr_array_unref);
  g_main_loop_unref(capture.loop);
  fake_menu_free(fake);
}

static void test_freeing_the_watch_cancels_the_read(void) {
  FakeMenu *fake = fake_menu_new("org.example.MenuCancel");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE), .expected = 1};
  WhelkDBusMenuWatch *watch = whelk_dbusmenu_watch_new(
      fake->bus, fake->name, "/MenuBar/1", WHELK_CHORD_CUA, on_changed,
      on_invalidated, &capture);

  g_assert_nonnull(watch);

  emit_layout_updated(fake, 0);
  whelk_dbusmenu_watch_free(watch);

  /* Long enough for the debounce to have fired had the watch still been
   * alive; nobody may be told, and no read may go out. */
  const guint watchdog = g_timeout_add(300, on_watchdog, &capture);

  g_main_loop_run(capture.loop);

  if (!capture.timed_out)
    g_source_remove(watchdog);

  g_assert_true(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 0);
  g_assert_cmpuint(fake->get_layout_calls, ==, 0);

  g_main_loop_unref(capture.loop);
  fake_menu_free(fake);
}

static void on_fetched(GPtrArray *menus, gpointer user_data) {
  Capture *capture = user_data;

  g_clear_pointer(&capture->menus, g_ptr_array_unref);
  capture->menus = menus;
  capture->calls++;

  if (menus && menus->len > 0) {
    WhelkMenu *menu = g_ptr_array_index(menus, 0);

    if (menu->entries->len > 0) {
      WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, 0);

      capture->last_id = (guint)entry->dbus_item;
    }
  }

  g_main_loop_quit(capture->loop);
}

/* A tray icon's menu is built while it opens.  The applet has to be told
 * first; a GetLayout that comes before AboutToShow hands back ids from the
 * previous layout. */
static void test_fetch_asks_about_to_show_then_reads(void) {
  FakeMenu *fake = fake_menu_new("org.example.MenuFetch");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE)};

  fake->revision = 3;
  fake->context = TRUE;

  WhelkDBusMenuRequest *request = whelk_dbusmenu_fetch(
      fake->bus, fake->name, "/MenuBar/1", WHELK_CHORD_CUA, on_fetched,
      &capture);

  g_assert_nonnull(request);
  run_until_captured(&capture);

  g_assert_false(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 1);
  g_assert_cmpuint(fake->about_to_show_calls, ==, 1);
  g_assert_cmpuint(fake->get_layout_calls, ==, 1);
  g_assert_false(fake->layout_without_show);
  g_assert_cmpuint(capture.last_id, ==, 200);

  whelk_dbusmenu_request_free(request);
  g_clear_pointer(&capture.menus, g_ptr_array_unref);
  g_main_loop_unref(capture.loop);
  fake_menu_free(fake);
}

static void test_freeing_a_fetch_calls_nobody(void) {
  FakeMenu *fake = fake_menu_new("org.example.MenuFetchCancel");
  Capture capture = {.loop = g_main_loop_new(NULL, FALSE)};

  WhelkDBusMenuRequest *request = whelk_dbusmenu_fetch(
      fake->bus, fake->name, "/MenuBar/1", WHELK_CHORD_CUA, on_fetched,
      &capture);

  whelk_dbusmenu_request_free(request);

  const guint watchdog = g_timeout_add(300, on_watchdog, &capture);

  g_main_loop_run(capture.loop);

  if (!capture.timed_out)
    g_source_remove(watchdog);

  g_assert_true(capture.timed_out);
  g_assert_cmpuint(capture.calls, ==, 0);

  g_main_loop_unref(capture.loop);
  fake_menu_free(fake);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  if (!g_find_program_in_path("dbus-daemon")) {
    g_test_skip("dbus-daemon is not installed");
    return 0;
  }

  GTestDBus *bus = g_test_dbus_new(G_TEST_DBUS_NONE);

  g_test_dbus_up(bus);

  g_test_add_func("/dbusmenu/layout-updated",
                  test_layout_updated_reads_new_ids);
  g_test_add_func("/dbusmenu/items-properties-updated",
                  test_items_properties_updated_reads_the_tree);
  g_test_add_func("/dbusmenu/free-cancels", test_freeing_the_watch_cancels_the_read);
  g_test_add_func("/dbusmenu/fetch-tells-then-reads",
                  test_fetch_asks_about_to_show_then_reads);
  g_test_add_func("/dbusmenu/fetch-free-cancels",
                  test_freeing_a_fetch_calls_nobody);

  const int status = g_test_run();

  g_test_dbus_down(bus);
  g_object_unref(bus);

  return status;
}

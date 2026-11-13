// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menuprobe.h"

#include <glib.h>

/* The X11 registrar is a fallback, not the first answer.  These tests pin the
 * order: the exact _GTK_* menubar path first, then the window and app-menu
 * paths, then the conventional GTK paths, with the registrar available after
 * them and DBusMenu after org.gtk.Menus. */

static void assert_path(GPtrArray *paths, guint index, const char *expected) {
  g_assert_cmpuint(paths->len, >, index);
  g_assert_cmpstr(g_ptr_array_index(paths, index), ==, expected);
}

static WhelkFocusedPaths *x11_paths(void) {
  WhelkFocusedPaths *paths = g_new0(WhelkFocusedPaths, 1);

  paths->window_id = 0x3a00007;
  paths->bus_name = g_strdup("org.example.App");
  paths->app_path = g_strdup("/org/example/App");
  paths->window_path = g_strdup("/org/example/App/window/1");
  paths->menubar_path = g_strdup("/org/example/App/menus/MenuBar");
  paths->appmenu_path = g_strdup("/org/example/App/menus/AppMenu");

  return paths;
}

static void free_paths(WhelkFocusedPaths *paths) {
  whelk_focused_paths_clear(paths);
  g_free(paths);
}

static void test_x11_registrar_comes_after_exact_gtk_paths(void) {
  WhelkFocusedPaths *paths = x11_paths();
  WhelkMenuProbeOrder *order =
      whelk_menu_probe_order_new(paths, TRUE, "/org/example/App");

  /* Exact X11 paths, in specificity order. */
  assert_path(order->gtk_paths, 0, "/org/example/App/menus/MenuBar");
  assert_path(order->gtk_paths, 1, "/org/example/App/window/1/menus/menubar");
  assert_path(order->gtk_paths, 2, "/org/example/App/menus/AppMenu");

  /* Conventional guesses only after the exact ones. */
  assert_path(order->gtk_paths, 3, "/org/example/App/menus/menubar");
  assert_path(order->gtk_paths, 4, "/org/example/App/menus/AppMenu");
  assert_path(order->gtk_paths, 5, "/org/example/App/window/0/menus/menubar");
  assert_path(order->gtk_paths, 6,
              "/org/gtk/Application/anonymous/menus/menubar");
  g_assert_cmpuint(order->gtk_paths->len, ==, 7);

  /* The registrar is available for the X11 window, and DBusMenu is the final
   * protocol, not a replacement for the GTK attempts. */
  g_assert_true(order->ask_registrar);
  assert_path(order->dbus_paths, 0, "/MenuBar/1");
  assert_path(order->dbus_paths, 1, "/MenuBar/0");
  assert_path(order->dbus_paths, 2, "/com/canonical/dbusmenu");

  /* When the registrar answers, its exact path is what DBusMenu tries first;
   * the conventional paths remain behind it as a fallback. */
  whelk_menu_probe_use_registrar_path(order->dbus_paths,
                                      "/org/example/App/MenuBar");
  assert_path(order->dbus_paths, 0, "/org/example/App/MenuBar");
  assert_path(order->dbus_paths, 1, "/MenuBar/1");
  assert_path(order->dbus_paths, 2, "/MenuBar/0");
  assert_path(order->dbus_paths, 3, "/com/canonical/dbusmenu");
  g_assert_cmpuint(order->dbus_paths->len, ==, 4);

  whelk_menu_probe_order_free(order);
  free_paths(paths);
}

static void test_registrar_needs_an_x11_window(void) {
  WhelkFocusedPaths *no_window = x11_paths();

  no_window->window_id = 0;

  WhelkMenuProbeOrder *order =
      whelk_menu_probe_order_new(no_window, TRUE, "/org/example/App");

  g_assert_false(order->ask_registrar);
  g_assert_cmpuint(order->gtk_paths->len, ==, 7);

  whelk_menu_probe_order_free(order);
  free_paths(no_window);
}

static void test_wayland_uses_conventional_paths_only(void) {
  WhelkFocusedPaths empty = {0};
  WhelkMenuProbeOrder *order =
      whelk_menu_probe_order_new(&empty, TRUE, "/org/example/App");

  g_assert_false(order->ask_registrar);
  g_assert_cmpuint(order->gtk_paths->len, ==, 4);
  assert_path(order->gtk_paths, 0, "/org/example/App/menus/menubar");
  assert_path(order->gtk_paths, 1, "/org/example/App/menus/AppMenu");
  assert_path(order->gtk_paths, 2, "/org/example/App/window/0/menus/menubar");
  assert_path(order->gtk_paths, 3,
              "/org/gtk/Application/anonymous/menus/menubar");

  whelk_menu_probe_order_free(order);
}

static void test_registrar_can_rescue_an_invalid_bus_name(void) {
  WhelkFocusedPaths *paths = x11_paths();
  WhelkMenuProbeOrder *order =
      whelk_menu_probe_order_new(paths, FALSE, "/org/example/App");

  /* No org.gtk.Menus call may be addressed to a WM_CLASS.  The registrar is
   * still worth asking because it takes a window id, not a bus name. */
  g_assert_cmpuint(order->gtk_paths->len, ==, 0);
  g_assert_true(order->ask_registrar);
  g_assert_cmpuint(order->dbus_paths->len, ==, 3);

  whelk_menu_probe_order_free(order);
  free_paths(paths);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/menuprobe/registrar/x11-order",
                  test_x11_registrar_comes_after_exact_gtk_paths);
  g_test_add_func("/menuprobe/registrar/needs-window",
                  test_registrar_needs_an_x11_window);
  g_test_add_func("/menuprobe/registrar/wayland",
                  test_wayland_uses_conventional_paths_only);
  g_test_add_func("/menuprobe/registrar/invalid-bus",
                  test_registrar_can_rescue_an_invalid_bus_name);

  return g_test_run();
}

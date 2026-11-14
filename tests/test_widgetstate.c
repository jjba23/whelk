// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// Dragging a widget is arithmetic before it is a gesture: the position snaps
// to the desktop grid and stays inside the monitor.  The saved position is
// runtime state keyed by widget, so a reload puts the card back where it was
// left.  Both halves are tested here without a display.

#include "util/widgetstate.h"

#include <glib.h>
#include <glib/gstdio.h>

static void test_snap(void) {
  g_assert_cmpint(whelk_widget_snap(0, 8), ==, 0);
  g_assert_cmpint(whelk_widget_snap(3, 8), ==, 0);
  g_assert_cmpint(whelk_widget_snap(4, 8), ==, 8);
  g_assert_cmpint(whelk_widget_snap(33, 8), ==, 32);
  g_assert_cmpint(whelk_widget_snap(36, 8), ==, 40);
  g_assert_cmpint(whelk_widget_snap(-10, 8), ==, 0);
  g_assert_cmpint(whelk_widget_snap(37, 0), ==, 37);
}

static void test_clamp(void) {
  int x = 500;
  int y = -4;

  whelk_widget_clamp(&x, &y, 300, 200, 1920, 1080);
  g_assert_cmpint(x, ==, 500);
  g_assert_cmpint(y, ==, 0);

  x = 1800;
  y = 1000;
  whelk_widget_clamp(&x, &y, 300, 200, 1920, 1080);
  g_assert_cmpint(x, ==, 1620);
  g_assert_cmpint(y, ==, 880);

  x = 50;
  y = 60;
  whelk_widget_clamp(&x, &y, 400, 300, 300, 200);
  g_assert_cmpint(x, ==, 0);
  g_assert_cmpint(y, ==, 0);
}

static void test_positions(void) {
  g_autofree char *directory = g_dir_make_tmp("whelk-widgetstate-XXXXXX", NULL);
  g_autofree char *path =
      g_build_filename(directory, "widgets-settings.ini", NULL);
  int x = 0;
  int y = 0;

  g_assert_nonnull(directory);
  g_assert_false(whelk_widget_state_load_position(path, "widget-clock-0", &x,
                                                  &y));

  g_assert_true(
      whelk_widget_state_save_position(path, "widget-clock-0", 32, 64));
  g_assert_true(
      whelk_widget_state_save_position(path, "widget-notes-1", 400, 120));
  g_assert_true(whelk_widget_state_load_position(path, "widget-clock-0", &x,
                                                 &y));
  g_assert_cmpint(x, ==, 32);
  g_assert_cmpint(y, ==, 64);
  g_assert_true(
      whelk_widget_state_load_position(path, "widget-notes-1", &x, &y));
  g_assert_cmpint(x, ==, 400);
  g_assert_cmpint(y, ==, 120);

  g_assert_true(
      whelk_widget_state_save_position(path, "widget-clock-0", 48, 72));
  g_assert_true(whelk_widget_state_load_position(path, "widget-clock-0", &x,
                                                 &y));
  g_assert_cmpint(x, ==, 48);
  g_assert_cmpint(y, ==, 72);

  g_assert_true(g_file_set_contents(path, "not a key file\n", -1, NULL));
  g_assert_false(whelk_widget_state_load_position(path, "widget-clock-0", &x,
                                                  &y));

  g_unlink(path);
  g_rmdir(directory);
}

static void test_widget_state(void) {
  g_autofree char *directory = g_dir_make_tmp("whelk-widgetstate-XXXXXX", NULL);
  g_autofree char *path =
      g_build_filename(directory, "widgets-settings.ini", NULL);
  g_autofree char *value = whelk_widget_state_get(
      path, "widget-reminders-2", "items");

  g_assert_nonnull(directory);
  g_assert_null(value);

  g_assert_true(whelk_widget_state_set(path, "widget-reminders-2", "items",
                                       "[x] ship it\nwater the plants"));
  g_assert_true(whelk_widget_state_set(path, "widget-reminders-3", "items",
                                       "read the manual"));

  value = whelk_widget_state_get(path, "widget-reminders-2", "items");
  g_assert_cmpstr(value, ==, "[x] ship it\nwater the plants");
  g_clear_pointer(&value, g_free);

  g_assert_true(whelk_widget_state_set(path, "widget-reminders-2", "items",
                                       NULL));
  value = whelk_widget_state_get(path, "widget-reminders-2", "items");
  g_assert_null(value);

  value = whelk_widget_state_get(path, "widget-reminders-3", "items");
  g_assert_cmpstr(value, ==, "read the manual");

  g_unlink(path);
  g_rmdir(directory);
}

static void test_state_path(void) {
  g_autofree char *path = whelk_widget_state_path("/tmp/settings.scm");

  g_assert_nonnull(path);
  g_assert_true(g_str_has_suffix(path, "widgets-settings.ini"));
  g_assert_true(strstr(path, "/whelk/") != NULL ||
                g_str_has_suffix(path, "/whelk/widgets-settings.ini"));
  g_assert_null(whelk_widget_state_path(NULL));
  g_assert_null(whelk_widget_state_path(""));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/widgetstate/snap", test_snap);
  g_test_add_func("/widgetstate/clamp", test_clamp);
  g_test_add_func("/widgetstate/positions", test_positions);
  g_test_add_func("/widgetstate/state", test_widget_state);
  g_test_add_func("/widgetstate/path", test_state_path);

  return g_test_run();
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// A desktop widget crosses the same bridge a bar does: Scheme holds the kind,
// placement and options, and C receives plain structs it can outlive the call
// with.  This test is the contract between the two.

#include "config.h"
#include "guile_bridge.h"

#include <glib.h>
#include <glib/gstdio.h>

static void test_widget_bridge(void) {
  g_autofree char *directory =
      g_dir_make_tmp("whelk-widgets-XXXXXX", NULL);
  g_assert_nonnull(directory);

  g_autofree char *path =
      g_build_filename(directory, "settings.scm", NULL);
  const char *settings =
      "(configure-shell!\n"
      " #:widgets\n"
      " (list (desktop-widget 'clock #:x 20 #:y 30 #:width 400 #:height 300\n"
      "                       #:monitor \"DP-1\"\n"
      "                       #:options '((style . digital)))\n"
      "       (desktop-widget 'notes #:x 12 #:y 14\n"
      "                       #:options '((text . \"hello\")))))\n";

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  GPtrArray *widgets = whelk_bridge_widgets(path);

  g_assert_cmpuint(widgets->len, ==, 2);

  WhelkWidgetConfig *clock = g_ptr_array_index(widgets, 0);

  g_assert_cmpstr(clock->spec->kind, ==, "clock");
  g_assert_cmpint(clock->x, ==, 20);
  g_assert_cmpint(clock->y, ==, 30);
  g_assert_cmpint(clock->width, ==, 400);
  g_assert_cmpint(clock->height, ==, 300);
  g_assert_cmpstr(clock->monitor, ==, "DP-1");
  g_assert_cmpstr(whelk_spec_str(clock->spec, "style", ""), ==, "digital");

  WhelkWidgetConfig *notes = g_ptr_array_index(widgets, 1);

  g_assert_cmpstr(notes->spec->kind, ==, "notes");
  g_assert_cmpstr(whelk_spec_str(notes->spec, "text", ""), ==, "hello");

  /* The copy is deep enough for the widget to outlive the bridge's array. */
  WhelkWidgetConfig *copy = whelk_widget_config_copy(clock);

  whelk_config_free_widgets(widgets);
  g_assert_cmpstr(copy->spec->kind, ==, "clock");
  g_assert_cmpstr(copy->monitor, ==, "DP-1");
  whelk_widget_config_free(copy);

  g_unlink(path);
  g_rmdir(directory);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  g_assert_true(whelk_bridge_use_core());

  g_test_add_func("/widgets/bridge", test_widget_bridge);

  return g_test_run();
}

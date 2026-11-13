// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include <glib.h>
#include <glib/gstdio.h>

#include "util/trayicon.h"

/* JetBrains Toolbox publishes its own colour icon under IconThemePath, while a
 * desktop icon theme may offer a monochrome icon with the same name.  The app's
 * directory has to be searched first. */

static void test_finds_app_icon(void) {
  g_autofree char *dir = g_dir_make_tmp("whelk-trayicon-XXXXXX", NULL);

  g_assert_nonnull(dir);

  g_autofree char *png =
      g_build_filename(dir, "toolbox-tray-color.png", NULL);
  g_autofree char *svg =
      g_build_filename(dir, "toolbox-tray-color.svg", NULL);
  g_autofree char *found = NULL;

  g_assert_true(g_file_set_contents(png, "png", 3, NULL));
  g_assert_true(g_file_set_contents(svg, "svg", 3, NULL));

  found = whelk_tray_icon_file(dir, "toolbox-tray-color");
  g_assert_cmpstr(found, ==, png);

  g_assert_cmpint(g_remove(png), ==, 0);
  g_clear_pointer(&found, g_free);
  found = whelk_tray_icon_file(dir, "toolbox-tray-color");
  g_assert_cmpstr(found, ==, svg);

  g_assert_cmpint(g_remove(svg), ==, 0);
  g_clear_pointer(&found, g_free);
  g_assert_null(whelk_tray_icon_file(dir, "toolbox-tray-color"));

  g_assert_cmpint(g_rmdir(dir), ==, 0);
}

static void test_rejects_relative_and_escaping_names(void) {
  g_autofree char *dir = g_dir_make_tmp("whelk-trayicon-XXXXXX", NULL);

  g_assert_nonnull(dir);
  g_assert_null(whelk_tray_icon_file(dir, "../toolbox-tray-color"));
  g_assert_null(whelk_tray_icon_file(dir, "/tmp/toolbox-tray-color"));
  g_assert_null(whelk_tray_icon_file("relative-path", "toolbox-tray-color"));
  g_assert_null(whelk_tray_icon_file(dir, ""));
  g_assert_null(whelk_tray_icon_file(NULL, "toolbox-tray-color"));

  g_assert_cmpint(g_rmdir(dir), ==, 0);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/trayicon/finds-app-icon", test_finds_app_icon);
  g_test_add_func("/trayicon/rejects-unsafe-paths",
                  test_rejects_relative_and_escaping_names);

  return g_test_run();
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/desktop.h"

#include <glib/gstdio.h>

/* The desktop database is read from XDG_DATA_DIRS, which is a test's only way
 * in: a temporary applications/ directory stands in for the machine's.  GIO
 * reads that environment once per process, so the cases share one test. */

static void write_entry(const char *directory, const char *file_name,
                        const char *name, const char *extra) {
  g_autofree char *path = g_build_filename(directory, file_name, NULL);
  /* Exec has to be a program that exists: GIO refuses an entry it cannot
   * resolve, which is why the fixture names /bin/true and not the app. */
  g_autofree char *contents =
      g_strdup_printf("[Desktop Entry]\nType=Application\nName=%s\n"
                      "Exec=/bin/true\nNoDisplay=true\n%s",
                      name, extra ? extra : "");

  g_assert_true(g_file_set_contents(path, contents, -1, NULL));
}

static void test_resolves_app_id(void) {
  g_autofree char *root = g_dir_make_tmp("whelk-desktop-XXXXXX", NULL);
  g_autofree char *apps = g_build_filename(root, "applications", NULL);

  g_assert_nonnull(root);
  g_assert_cmpint(g_mkdir_with_parents(apps, 0700), ==, 0);

  write_entry(apps, "Bar.desktop", "Bar", NULL);
  write_entry(apps, "org.example.Foo.desktop", "Foo",
              "Actions=New\n[Desktop Action New]\nName=New Window\n"
              "Exec=/bin/true --new\n");

  g_setenv("XDG_DATA_HOME", root, TRUE);
  g_setenv("XDG_DATA_DIRS", root, TRUE);

  /* The two obvious names. */
  g_autoptr(GDesktopAppInfo) exact = whelk_desktop_for_app_id("Bar");

  g_assert_nonnull(exact);
  g_assert_cmpstr(g_desktop_app_info_get_filename(exact), !=, NULL);

  /* The case the bar actually meets: a window id that is not the file's name,
   * whose entry still carries the actions the File menu needs. */
  g_autoptr(GDesktopAppInfo) reverse_dns =
      whelk_desktop_for_app_id("Foo");

  g_assert_nonnull(reverse_dns);
  g_autofree char *id = g_strdup(g_desktop_app_info_get_string(reverse_dns,
                                                              "Name"));
  g_assert_cmpstr(id, ==, "Foo");

  const char *const *actions = g_desktop_app_info_list_actions(reverse_dns);
  g_assert_nonnull(actions);
  g_assert_cmpstr(actions[0], ==, "New");
  g_assert_null(actions[1]);

  g_autofree char *action_name =
      g_desktop_app_info_get_action_name(reverse_dns, "New");
  g_assert_cmpstr(action_name, ==, "New Window");

  /* Nothing on this desktop is called that; a search must not invent one. */
  g_assert_null(whelk_desktop_for_app_id("Nope"));
  g_assert_null(whelk_desktop_for_app_id(""));
  g_assert_null(whelk_desktop_for_app_id(NULL));

  g_autofree char *bar_desktop = g_build_filename(apps, "Bar.desktop", NULL);
  g_autofree char *foo_desktop =
      g_build_filename(apps, "org.example.Foo.desktop", NULL);

  g_remove(bar_desktop);
  g_remove(foo_desktop);
  g_rmdir(apps);
  g_rmdir(root);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/desktop/app-id", test_resolves_app_id);

  return g_test_run();
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/shortcutmaps.h"

#include <glib.h>

static WhelkMenu *menu_at(GPtrArray *bar, guint index) {
  return g_ptr_array_index(bar, index);
}

static WhelkMenuEntry *entry_at(WhelkMenu *menu, guint index) {
  return g_ptr_array_index(menu->entries, index);
}

static void point_at_fixture(void) {
  const char *tests = g_getenv("WHELK_TEST_DIR");

  g_assert_nonnull(tests);
  g_autofree char *dir = g_build_filename(tests, "fixtures", "shortcutmaps", NULL);

  g_setenv("WHELK_SHORTCUT_MAPS", dir, TRUE);
  whelk_shortcutmaps_reset();
}

static void test_imports_menus_and_nested_submenus(void) {
  point_at_fixture();

  g_autoptr(GPtrArray) bar = whelk_shortcutmaps_menubar_for_app(
      "org.example.Demo", "wtype", WHELK_CHORD_CUA);

  g_assert_cmpuint(bar->len, ==, 2);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 7);

  WhelkMenuEntry *new_tab = entry_at(file, 0);

  g_assert_cmpstr(new_tab->label, ==, "New");
  g_assert_cmpstr(new_tab->accel, ==, "Ctrl+N");
  g_assert_cmpstr(new_tab->command, ==, "wtype -M ctrl -k n -m ctrl");

  WhelkMenuEntry *recent = entry_at(file, 1);

  g_assert_cmpstr(recent->label, ==, "Recent");
  g_assert_nonnull(recent->submenu);
  g_assert_cmpuint(recent->submenu->entries->len, ==, 2);

  WhelkMenuEntry *save = entry_at(recent->submenu, 1);

  g_assert_cmpstr(save->accel, ==, "Ctrl+X Ctrl+S");
  g_assert_cmpstr(save->command, ==,
                  "wtype -M ctrl -k x -m ctrl && "
                  "wtype -M ctrl -k s -m ctrl");

  g_assert_cmpint(entry_at(file, 2)->kind, ==, WHELK_MENU_SEPARATOR);

  /* An action without a shortcut is still usable through org.gtk.Actions;
   * an item with neither is honest about being unavailable. */
  WhelkMenuEntry *action = entry_at(file, 3);

  g_assert_cmpstr(action->label, ==, "Needs a menu");
  g_assert_cmpstr(action->action, ==, "app.open-menu");
  g_assert_true(action->enabled);

  /* Orbit's GIMP mappings name actions without a prefix: they belong to the
   * application object the focused window published. */
  WhelkMenuEntry *plain = entry_at(file, 4);

  g_assert_cmpstr(plain->action, ==, "file-open");
  g_assert_true(plain->enabled);

  /* An integer target is not representable in whelk's menu model yet; the row
   * stays for discoverability but must not claim it can activate it. */
  WhelkMenuEntry *targeted = entry_at(file, 5);

  g_assert_cmpstr(targeted->action, ==, NULL);
  g_assert_false(targeted->enabled);

  WhelkMenuEntry *unavailable = entry_at(file, 6);

  g_assert_cmpstr(unavailable->label, ==, "Unavailable");
  g_assert_false(unavailable->enabled);
}

static void test_imports_punctuation_and_sequences(void) {
  point_at_fixture();

  g_autoptr(GPtrArray) bar = whelk_shortcutmaps_menubar_for_app(
      "demo", "wtype", WHELK_CHORD_CUA);
  WhelkMenu *edit = menu_at(bar, 1);

  g_assert_cmpstr(entry_at(edit, 0)->accel, ==, "Ctrl+]");
  g_assert_cmpstr(entry_at(edit, 0)->command, ==,
                  "wtype -M ctrl -k bracketright -m ctrl");
  g_assert_cmpstr(entry_at(edit, 1)->accel, ==, ":");
  g_assert_cmpstr(entry_at(edit, 1)->command, ==, "wtype -k colon");
  g_assert_cmpstr(entry_at(edit, 2)->accel, ==, "<");
  g_assert_cmpstr(entry_at(edit, 2)->command, ==, "wtype -k less");
}

static void test_unknown_app_and_no_tool_are_empty(void) {
  point_at_fixture();

  g_autoptr(GPtrArray) unknown = whelk_shortcutmaps_menubar_for_app(
      "org.example.Unknown", "wtype", WHELK_CHORD_CUA);

  g_assert_cmpuint(unknown->len, ==, 0);

  /* There is nothing to type with, so a mapping full of shortcuts is not
   * offered as a set of dead entries. */
  g_autoptr(GPtrArray) untooled =
      whelk_shortcutmaps_menubar_for_app("demo", NULL, WHELK_CHORD_CUA);

  g_assert_cmpuint(untooled->len, ==, 0);
}

static void test_desktop_suffix_finds_the_same_mapping(void) {
  point_at_fixture();

  g_autoptr(GPtrArray) with_suffix = whelk_shortcutmaps_menubar_for_app(
      "org.example.Demo.desktop", "xdotool", WHELK_CHORD_CUA);

  g_assert_cmpuint(with_suffix->len, ==, 2);
  g_assert_cmpstr(entry_at(menu_at(with_suffix, 0), 0)->command, ==,
                  "xdotool key ctrl+n");
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/shortcutmaps/import", test_imports_menus_and_nested_submenus);
  g_test_add_func("/shortcutmaps/punctuation",
                  test_imports_punctuation_and_sequences);
  g_test_add_func("/shortcutmaps/empty", test_unknown_app_and_no_tool_are_empty);
  g_test_add_func("/shortcutmaps/desktop-suffix",
                  test_desktop_suffix_finds_the_same_mapping);

  return g_test_run();
}

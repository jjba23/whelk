// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"
#include "util/menutree.h"

#include <glib.h>
#include <string.h>

/* scheme/whelk/orbit.scm is generated data, so these tests read the shipped
 * module the way the shell does.  They check the parts that can silently rot
 * during a conversion: the identifiers, a shortcut, a nested menu and a blank
 * application. */

static void start_bridge(void) {
  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
}

static WhelkMenu *find_menu(GPtrArray *bar, const char *label) {
  for (guint i = 0; i < bar->len; i++) {
    WhelkMenu *menu = g_ptr_array_index(bar, i);

    if (g_strcmp0(menu->label, label) == 0)
      return menu;
  }

  return NULL;
}

static WhelkMenuEntry *find_entry(WhelkMenu *menu, const char *label) {
  if (!menu)
    return NULL;

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_ENTRY &&
        g_strcmp0(entry->label, label) == 0)
      return entry;
  }

  return NULL;
}

static void test_known_app(void) {
  char *name = NULL;
  GPtrArray *bar = whelk_bridge_orbit_menubar(
      "org.kde.akregator", "wtype", WHELK_CHORD_CUA, &name);

  g_assert_cmpstr(name, ==, "Akregator");
  g_assert_cmpuint(bar->len, >=, 8);

  WhelkMenu *file = find_menu(bar, "File");
  WhelkMenuEntry *print = find_entry(file, "Print…");

  g_assert_nonnull(print);
  g_assert_cmpstr(print->accel, ==, "Ctrl+P");
  g_assert_nonnull(print->command);
  g_assert_true(g_str_has_prefix(print->command, "wtype "));
  g_assert_true(print->enabled);

  g_free(name);
  g_ptr_array_unref(bar);
}

static void test_nested_submenu(void) {
  char *name = NULL;
  GPtrArray *bar = whelk_bridge_orbit_menubar(
      "xfce4-terminal", "wtype", WHELK_CHORD_CUA, &name);

  g_assert_cmpstr(name, ==, "Xfce Terminal");

  WhelkMenuEntry *zoom = NULL;

  for (guint i = 0; i < bar->len && !zoom; i++) {
    WhelkMenu *menu = g_ptr_array_index(bar, i);

    for (guint j = 0; j < menu->entries->len && !zoom; j++) {
      WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, j);

      if (entry->submenu && g_strcmp0(entry->label, "Zoom") == 0)
        zoom = entry;
    }
  }

  g_assert_nonnull(zoom);
  g_assert_nonnull(zoom->submenu);
  g_assert_nonnull(find_entry(zoom->submenu, "Zoom In"));

  g_free(name);
  g_ptr_array_unref(bar);
}

static void test_blank_and_unknown(void) {
  char *name = NULL;
  GPtrArray *blank = whelk_bridge_orbit_menubar(
      "vicinae", "wtype", WHELK_CHORD_CUA, &name);

  g_assert_cmpuint(blank->len, ==, 0);
  g_assert_cmpstr(name, ==, "Vicinae");
  g_free(name);
  g_ptr_array_unref(blank);

  name = NULL;
  GPtrArray *missing = whelk_bridge_orbit_menubar(
      "org.example.Nothing", "wtype", WHELK_CHORD_CUA, &name);

  g_assert_cmpuint(missing->len, ==, 0);
  g_assert_null(name);
  g_ptr_array_unref(missing);
}

static void test_needs_a_typing_tool(void) {
  char *name = NULL;
  GPtrArray *bar = whelk_bridge_orbit_menubar(
      "org.kde.akregator", NULL, WHELK_CHORD_CUA, &name);

  g_assert_cmpuint(bar->len, ==, 0);
  g_assert_null(name);
  g_ptr_array_unref(bar);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  start_bridge();

  g_test_add_func("/orbit/known-app", test_known_app);
  g_test_add_func("/orbit/nested-submenu", test_nested_submenu);
  g_test_add_func("/orbit/blank-and-unknown", test_blank_and_unknown);
  g_test_add_func("/orbit/needs-tool", test_needs_a_typing_tool);

  return g_test_run();
}

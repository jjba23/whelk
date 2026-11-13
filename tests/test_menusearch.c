// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menusearch.h"

#include "util/menuaction.h"

#include <glib.h>

/* The menu search reads the same trees the bar renders; what it adds is a path
 * and a chord per clickable entry, and a copy that survives the next focus
 * change. */

static WhelkMenu *add_menu(GPtrArray *bar, const char *label) {
  WhelkMenu *menu = whelk_menu_new(label);

  g_ptr_array_add(bar, menu);
  return menu;
}

static void test_flattens_paths_and_chords(void) {
  g_autoptr(GPtrArray) bar = whelk_menubar_new();
  WhelkMenu *file = add_menu(bar, "File");
  WhelkMenuEntry *new_window = whelk_menu_add_entry(file, "New Window");

  new_window->action = g_strdup("app.new-window");
  new_window->accel = g_strdup("Ctrl+N");

  WhelkMenu *export = whelk_menu_add_submenu(file, "Export");
  WhelkMenuEntry *pdf = whelk_menu_add_entry(export, "PDF");

  pdf->command = g_strdup("echo pdf");
  pdf->accel = g_strdup("Ctrl+Shift+E");

  WhelkMenu *view = add_menu(bar, "View");
  WhelkMenuEntry *sidebar = whelk_menu_add_entry(view, "Sidebar");

  sidebar->action = g_strdup("win.show-sidebar");
  sidebar->target = g_strdup("true");

  WhelkMenuActionContext context = {0};

  whelk_menu_action_context_set(&context, "org.example.App",
                                "/org/example/App", NULL);
  whelk_menu_search_publish("Example", &context, bar);

  GPtrArray *items = whelk_menu_search_snapshot();

  g_assert_cmpstr(whelk_menu_search_app_name(), ==, "Example");
  g_assert_cmpuint(items->len, ==, 3);

  WhelkMenuSearchItem *first = g_ptr_array_index(items, 0);

  g_assert_cmpstr(first->path, ==, "File > New Window");
  g_assert_cmpstr(first->label, ==, "New Window");
  g_assert_cmpstr(first->accel, ==, "Ctrl+N");
  g_assert_cmpstr(first->entry->action, ==, "app.new-window");
  g_assert_cmpstr(first->context.bus_name, ==, "org.example.App");

  WhelkMenuSearchItem *second = g_ptr_array_index(items, 1);

  /* A submenu is part of the path, not a row of its own. */
  g_assert_cmpstr(second->path, ==, "File > Export > PDF");
  g_assert_cmpstr(second->accel, ==, "Ctrl+Shift+E");
  g_assert_cmpstr(second->entry->command, ==, "echo pdf");

  WhelkMenuSearchItem *third = g_ptr_array_index(items, 2);

  g_assert_cmpstr(third->path, ==, "View > Sidebar");
  g_assert_cmpstr(third->entry->target, ==, "true");
  g_assert_null(third->accel);

  whelk_menu_action_context_clear(&context);
}

static void test_rules_and_disabled_rows_are_not_results(void) {
  g_autoptr(GPtrArray) bar = whelk_menubar_new();
  WhelkMenu *file = add_menu(bar, "File");
  WhelkMenuEntry *save = whelk_menu_add_entry(file, "Save");

  save->action = g_strdup("app.save");
  whelk_menu_add_separator(file);

  WhelkMenuEntry *print = whelk_menu_add_entry(file, "Print");

  print->action = g_strdup("app.print");
  print->enabled = FALSE;

  /* An entry that does nothing is not a search result either. */
  whelk_menu_add_entry(file, "Nothing");

  whelk_menu_search_publish("Example", NULL, bar);

  GPtrArray *items = whelk_menu_search_snapshot();

  g_assert_cmpuint(items->len, ==, 1);
  g_assert_cmpstr(((WhelkMenuSearchItem *)g_ptr_array_index(items, 0))->label,
                  ==, "Save");
}

static void test_cleared_when_no_menu_is_up(void) {
  g_autoptr(GPtrArray) bar = whelk_menubar_new();
  WhelkMenu *file = add_menu(bar, "File");
  WhelkMenuEntry *save = whelk_menu_add_entry(file, "Save");

  save->action = g_strdup("app.save");
  whelk_menu_search_publish("Example", NULL, bar);
  whelk_menu_search_clear();

  g_assert_cmpuint(whelk_menu_search_snapshot()->len, ==, 0);
  g_assert_cmpstr(whelk_menu_search_app_name(), ==, "");
}

static void test_a_copy_survives_a_new_publish(void) {
  g_autoptr(GPtrArray) bar = whelk_menubar_new();
  WhelkMenu *file = add_menu(bar, "File");
  WhelkMenuEntry *save = whelk_menu_add_entry(file, "Save As");

  save->action = g_strdup("app.save-as");
  save->target = g_strdup("pdf");

  whelk_menu_search_publish("Example", NULL, bar);

  WhelkMenuSearchItem *copy = whelk_menu_search_item_copy(
      g_ptr_array_index(whelk_menu_search_snapshot(), 0));

  whelk_menu_search_clear();

  g_assert_cmpstr(copy->path, ==, "File > Save As");
  g_assert_cmpstr(copy->entry->target, ==, "pdf");

  whelk_menu_search_item_free(copy);
}

static void test_action_names_and_paths(void) {
  g_assert_cmpstr(whelk_menu_action_name("win.copy"), ==, "copy");
  g_assert_cmpstr(whelk_menu_action_name("app.new-window"), ==, "new-window");
  g_assert_cmpstr(whelk_menu_action_name("quit"), ==, "quit");

  g_autofree char *window =
      whelk_menu_action_object_path("win.copy", "/org/example/App");

  g_assert_cmpstr(window, ==, "/org/example/App/windows/0");

  g_autofree char *application =
      whelk_menu_action_object_path("app.quit", "/org/example/App");

  g_assert_cmpstr(application, ==, "/org/example/App");
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/menusearch/paths-and-chords",
                  test_flattens_paths_and_chords);
  g_test_add_func("/menusearch/skips-rules-and-disabled",
                  test_rules_and_disabled_rows_are_not_results);
  g_test_add_func("/menusearch/clear", test_cleared_when_no_menu_is_up);
  g_test_add_func("/menusearch/copy", test_a_copy_survives_a_new_publish);
  g_test_add_func("/menusearch/action-resolution",
                  test_action_names_and_paths);

  return g_test_run();
}

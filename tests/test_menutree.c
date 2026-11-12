// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menutree.h"

#include "guile_bridge.h"
#include "util/menudict.h"

#include <glib.h>

/* The menubar's two halves: what an application exports over D-Bus, and what
 * the shell makes up for the applications that export nothing. */

static WhelkMenuEntry *entry_at(WhelkMenu *menu, guint index) {
  return g_ptr_array_index(menu->entries, index);
}

static WhelkMenu *menu_at(GPtrArray *bar, guint index) {
  return g_ptr_array_index(bar, index);
}

static void test_synthesize_file_menus(void) {
  const char *ids[] = {"new-window", "new-private"};
  const char *labels[] = {"New Window", "New Private Window"};
  const char *source = "not asked";

  /* An application nobody wrote down, and no program that can type: File is
   * all it gets, from what its desktop entry declares. */
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.example.Unknown", ids, labels, 2, "niri msg action close-window",
      NULL, &source);

  g_assert_cmpuint(bar->len, ==, 1);
  g_assert_null(source);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 4);

  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
  g_assert_cmpstr(entry_at(file, 0)->action, ==, "desktop:new-window");
  g_assert_cmpstr(entry_at(file, 1)->label, ==, "New Private Window");
  g_assert_cmpint(entry_at(file, 2)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(file, 3)->label, ==, "Close Window");
  g_assert_cmpstr(entry_at(file, 3)->command, ==,
                  "niri msg action close-window");
}

static void test_synthesize_shortcuts(void) {
  g_autoptr(GPtrArray) bar =
      whelk_menubar_for_app(NULL, NULL, NULL, 0, NULL, "wtype", NULL);

  /* Nothing of its own to say, so Edit and View only. */
  g_assert_cmpuint(bar->len, ==, 2);
  g_assert_cmpstr(menu_at(bar, 0)->label, ==, "Edit");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->label, ==, "Undo");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->accel, ==, "Ctrl+Z");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->command, ==,
                  "wtype -M ctrl -k z -m ctrl");

  WhelkMenu *view = menu_at(bar, 1);

  g_assert_cmpstr(view->label, ==, "View");

  /* Full Screen has no modifier, and its command says so. */
  WhelkMenuEntry *full = entry_at(view, view->entries->len - 1);

  g_assert_cmpstr(full->accel, ==, "F11");
  g_assert_cmpstr(full->command, ==, "wtype -k F11");
}

static void test_synthesize_without_tool(void) {
  /* No program can type a chord, and the application declares nothing, so
   * there is no menu that could work -- and no empty File button either. */
  g_autoptr(GPtrArray) bar =
      whelk_menubar_for_app("org.example.Unknown", NULL, NULL, 0, NULL, NULL,
                            NULL);

  g_assert_cmpuint(bar->len, ==, 0);
}

static void test_synthesize_without_close_command(void) {
  /* A session that cannot close windows gets no dead entry, and no rule with
   * nothing under it. */
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};

  g_autoptr(GPtrArray) bar =
      whelk_menubar_for_app(NULL, ids, labels, 1, NULL, NULL, NULL);
  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpuint(file->entries->len, ==, 1);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
}

static void test_dictionary_menus(void) {
  const char *source = NULL;

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.mozilla.firefox", NULL, NULL, 0, "niri msg action close-window",
      "wtype", &source);

  /* The dictionary answers for Firefox: File, Edit and View of its own. */
  g_assert_cmpstr(source, ==, "Firefox");
  g_assert_cmpuint(bar->len, ==, 3);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Tab");
  g_assert_cmpstr(entry_at(file, 0)->accel, ==, "Ctrl+T");
  g_assert_cmpstr(entry_at(file, 0)->command, ==,
                  "wtype -M ctrl -k t -m ctrl");

  /* The compositor's Close Window is the last thing in File, after a rule. */
  g_assert_cmpstr(entry_at(file, file->entries->len - 1)->label, ==,
                  "Close Window");
  g_assert_cmpint(entry_at(file, file->entries->len - 2)->kind, ==,
                  WHELK_MENU_SEPARATOR);

  g_assert_cmpstr(menu_at(bar, 1)->label, ==, "Edit");
  g_assert_cmpstr(menu_at(bar, 2)->label, ==, "View");
  g_assert_cmpstr(entry_at(menu_at(bar, 2), 0)->label, ==, "Reload");
}

static void test_dictionary_deduplicates_file(void) {
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "chromium", ids, labels, 1, NULL, "wtype", NULL);
  WhelkMenu *file = menu_at(bar, 0);
  guint found = 0;

  for (guint i = 0; i < file->entries->len; i++)
    if (g_strcmp0(entry_at(file, i)->label, "New Window") == 0)
      found++;

  /* The desktop entry's action is the real one; the dictionary's copy is not
   * a second line of the menu. */
  g_assert_cmpuint(found, ==, 1);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
  g_assert_cmpstr(entry_at(file, 0)->action, ==, "desktop:new-window");
}

static void test_dictionary_xdotool(void) {
  /* The same menu on X11: one xdotool call per chord. */
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "firefox", NULL, NULL, 0, NULL, "xdotool", NULL);

  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->command, ==,
                  "xdotool key ctrl+t");
}

static void test_dictionary_needs_a_tool(void) {
  const char *source = "not asked";
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};

  /* With nothing to type with, the dictionary's menus are not offered: an
   * entry that cannot work is worse than an absent one. */
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "firefox", ids, labels, 1, NULL, NULL, &source);

  g_assert_null(source);
  g_assert_cmpuint(bar->len, ==, 1);
  g_assert_cmpstr(menu_at(bar, 0)->label, ==, "File");
  g_assert_cmpuint(menu_at(bar, 0)->entries->len, ==, 1);
}

static void test_shortcut_command(void) {
  g_autofree char *chord = whelk_shortcut_command("wtype", "ctrl shift", "p");

  g_assert_cmpstr(chord, ==, "wtype -M ctrl shift -k p -m ctrl shift");
  g_assert_null(whelk_shortcut_command(NULL, "ctrl", "p"));
  g_assert_null(whelk_shortcut_command("wtype", "ctrl", ""));

  /* On X11 the same chord is one xdotool call. */
  g_autofree char *x11 = whelk_shortcut_command("xdotool", "ctrl shift", "z");

  g_assert_cmpstr(x11, ==, "xdotool key ctrl+shift+z");

  g_autofree char *plain = whelk_shortcut_command("xdotool", NULL, "F11");

  g_assert_cmpstr(plain, ==, "xdotool key F11");

  /* A tool that is not xdotool is wtype, and wtype presses and releases. */
  g_autofree char *bare = whelk_shortcut_command("/usr/bin/wtype", NULL, "F5");

  g_assert_cmpstr(bare, ==, "/usr/bin/wtype -k F5");
}

static GVariant *parse(const char *text) {
  g_autoptr(GError) error = NULL;
  GVariant *value = g_variant_parse(G_VARIANT_TYPE("a(uuaa{sv})"), text, NULL,
                                    NULL, &error);

  g_assert_no_error(error);
  g_assert_nonnull(value);

  return value;
}

static void test_gtk_menubar(void) {
  /* A menubar is a section of menus, each menu a submenu link. */
  g_autoptr(GVariant) reply = parse(
      "[ (uint32 0, uint32 0, [ {':section': <(uint32 0, uint32 1)>} ]),"
      "  (uint32 0, uint32 1, [ {'label': <'File'>, ':submenu': <(uint32 0, uint32 2)>},"
      "                         {'label': <'Edit'>, ':submenu': <(uint32 0, uint32 3)>} ]),"
      "  (uint32 0, uint32 2, [ {'label': <'New'>, 'action': <'app.new'>},"
      "                         {'label': <'Quit'>, 'action': <'app.quit'>, 'enabled': <false>} ]),"
      "  (uint32 0, uint32 3, [ {'label': <'Copy'>, 'action': <'win.copy'>, 'target': <'all'>} ]) ]");

  g_autoptr(GPtrArray) bar = whelk_menubar_from_gtk(reply, 0, 0);

  g_assert_cmpuint(bar->len, ==, 2);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 2);
  g_assert_cmpstr(entry_at(file, 0)->action, ==, "app.new");
  g_assert_true(entry_at(file, 0)->enabled);
  g_assert_false(entry_at(file, 1)->enabled);

  WhelkMenu *edit = menu_at(bar, 1);

  g_assert_cmpstr(entry_at(edit, 0)->action, ==, "win.copy");
  g_assert_cmpstr(entry_at(edit, 0)->target, ==, "all");
}

static void test_gtk_sections_inside_a_menu(void) {
  g_autoptr(GVariant) reply = parse(
      "[ (uint32 0, uint32 0, [ {'label': <'File'>, ':submenu': <(uint32 0, uint32 1)>} ]),"
      "  (uint32 0, uint32 1, [ {'label': <'New'>, 'action': <'app.new'>},"
      "                         {':section': <(uint32 0, uint32 2)>} ]),"
      "  (uint32 0, uint32 2, [ {'label': <'Close'>, 'action': <'win.close'>} ]) ]");

  g_autoptr(GPtrArray) bar = whelk_menubar_from_gtk(reply, 0, 0);
  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpuint(file->entries->len, ==, 3);
  g_assert_cmpint(entry_at(file, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(file, 2)->label, ==, "Close");
}

static void test_gtk_nothing(void) {
  g_autoptr(GPtrArray) none = whelk_menubar_from_gtk(NULL, 0, 0);

  g_assert_cmpuint(none->len, ==, 0);

  g_autoptr(GVariant) wrong = g_variant_new_string("not a menu");

  g_autoptr(GPtrArray) also_none =
      whelk_menubar_from_gtk(wrong, 0, 0);

  g_assert_cmpuint(also_none->len, ==, 0);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  /* The dictionary is Scheme's; load it the way the shell does, so these tests
   * see the menus an installed whelk would show. */
  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  whelk_menudict_load(whelk_bridge_menudict());

  g_test_add_func("/menutree/synthesize/file", test_synthesize_file_menus);
  g_test_add_func("/menutree/synthesize/shortcuts", test_synthesize_shortcuts);
  g_test_add_func("/menutree/synthesize/no-tool", test_synthesize_without_tool);
  g_test_add_func("/menutree/synthesize/no-close",
                  test_synthesize_without_close_command);
  g_test_add_func("/menutree/dictionary/menus", test_dictionary_menus);
  g_test_add_func("/menutree/dictionary/dedupe",
                  test_dictionary_deduplicates_file);
  g_test_add_func("/menutree/dictionary/xdotool", test_dictionary_xdotool);
  g_test_add_func("/menutree/dictionary/no-tool",
                  test_dictionary_needs_a_tool);
  g_test_add_func("/menutree/shortcut-command", test_shortcut_command);
  g_test_add_func("/menutree/gtk/menubar", test_gtk_menubar);
  g_test_add_func("/menutree/gtk/sections", test_gtk_sections_inside_a_menu);
  g_test_add_func("/menutree/gtk/nothing", test_gtk_nothing);

  return g_test_run();
}

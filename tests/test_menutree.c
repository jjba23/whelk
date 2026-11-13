// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menutree.h"

#include "guile_bridge.h"
#include "util/menudict.h"

#include <glib.h>
#include <glib/gstdio.h>

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
   * what its desktop entry declares, and Window is what the compositor can
   * do to it. */
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.example.Unknown", ids, labels, 2, NULL,
      &(WhelkSessionWindowActions){.close = "niri msg action close-window",
                                   .fullscreen =
                                       "niri msg action fullscreen-window"},
      NULL, WHELK_CHORD_CUA, &source);

  g_assert_cmpuint(bar->len, ==, 2);
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

  WhelkMenu *window = menu_at(bar, 1);

  g_assert_cmpstr(window->label, ==, "Window");
  g_assert_cmpuint(window->entries->len, ==, 1);
  g_assert_cmpstr(entry_at(window, 0)->label, ==, "Full Screen");
  g_assert_cmpstr(entry_at(window, 0)->command, ==,
                  "niri msg action fullscreen-window");
}

static void test_synthesize_shortcuts(void) {
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      NULL, NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_CUA, NULL);

  /* Nothing of its own to say, so Edit and View only. */
  g_assert_cmpuint(bar->len, ==, 2);
  g_assert_cmpstr(menu_at(bar, 0)->label, ==, "Edit");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->label, ==, "Undo");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->accel, ==, "Ctrl+Z");
  g_assert_cmpstr(entry_at(menu_at(bar, 0), 0)->command, ==,
                  "wtype -M ctrl -k z -m ctrl");

  WhelkMenu *edit = menu_at(bar, 0);
  gboolean saw_match_style = FALSE;
  gboolean saw_find_next = FALSE;
  gboolean saw_find_previous = FALSE;
  gboolean saw_replace = FALSE;

  for (guint i = 0; i < edit->entries->len; i++) {
    WhelkMenuEntry *entry = entry_at(edit, i);

    if (g_strcmp0(entry->label, "Paste and Match Style") == 0)
      saw_match_style = TRUE;
    if (g_strcmp0(entry->label, "Find Next") == 0)
      saw_find_next = TRUE;
    if (g_strcmp0(entry->label, "Find Previous") == 0)
      saw_find_previous = TRUE;
    if (g_strcmp0(entry->label, "Replace") == 0)
      saw_replace = TRUE;
  }

  g_assert_true(saw_match_style);
  g_assert_true(saw_find_next);
  g_assert_true(saw_find_previous);
  g_assert_true(saw_replace);

  WhelkMenu *view = menu_at(bar, 1);

  g_assert_cmpstr(view->label, ==, "View");
  g_assert_cmpstr(entry_at(view, 0)->label, ==, "Reload");

  /* Full Screen has no modifier, and its command says so. */
  WhelkMenuEntry *full = entry_at(view, view->entries->len - 1);

  g_assert_cmpstr(full->accel, ==, "F11");
  g_assert_cmpstr(full->command, ==, "wtype -k F11");
}

static void test_synthesize_new_window_and_verbs(void) {
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};
  WhelkSessionWindowActions actions = {
      .close = "close", .fullscreen = "fullscreen", .maximize = "maximize",
      .float_window = "float", .move_left = "left", .move_right = "right",
      .move_up = "up", .move_down = "down"};
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.example.Unknown", ids, labels, 1, "app --new-window", &actions,
      NULL, WHELK_CHORD_CUA, NULL);

  WhelkMenu *file = menu_at(bar, 0);

  /* Exec's New Window, the desktop action with the same job is not repeated
   * underneath it, and File ends with the compositor's close verb. */
  g_assert_cmpuint(file->entries->len, ==, 3);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
  g_assert_cmpstr(entry_at(file, 0)->command, ==, "app --new-window");
  g_assert_cmpint(entry_at(file, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(file, 2)->label, ==, "Close Window");
  g_assert_cmpstr(entry_at(file, 2)->command, ==, "close");

  WhelkMenu *window = menu_at(bar, 1);
  g_assert_cmpstr(entry_at(window, 0)->label, ==, "Maximize / Restore");
  g_assert_cmpstr(entry_at(window, 0)->command, ==, "maximize");
  g_assert_cmpstr(entry_at(window, 1)->label, ==, "Toggle Floating");
  g_assert_cmpint(entry_at(window, 2)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(window, 3)->label, ==, "Move Left");

  gboolean saw_fullscreen = FALSE;

  for (guint i = 0; i < window->entries->len; i++) {
    WhelkMenuEntry *entry = entry_at(window, i);

    if (g_strcmp0(entry->label, "Full Screen") == 0 &&
        g_strcmp0(entry->command, "fullscreen") == 0)
      saw_fullscreen = TRUE;
  }

  g_assert_true(saw_fullscreen);
}

static void test_app_menu_window_actions(void) {
  /* The application menu is the same compositor verbs for every application:
   * arranging and moving the focused window, then closing it. */
  WhelkSessionWindowActions actions = {
      .close = "close", .fullscreen = "fullscreen", .maximize = "maximize",
      .float_window = "float", .move_left = "left", .move_right = "right",
      .move_up = "up", .move_down = "down"};
  WhelkMenu *menu = whelk_app_menu(&actions);

  g_assert_cmpstr(entry_at(menu, 0)->label, ==, "Maximize / Restore");
  g_assert_cmpstr(entry_at(menu, 0)->command, ==, "maximize");
  g_assert_cmpstr(entry_at(menu, 1)->label, ==, "Toggle Floating");
  g_assert_cmpstr(entry_at(menu, 1)->command, ==, "float");

  gboolean saw_left = FALSE, saw_fullscreen = FALSE, saw_close = FALSE;

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = entry_at(menu, i);

    if (g_strcmp0(entry->label, "Move Left") == 0)
      saw_left = TRUE;
    else if (g_strcmp0(entry->label, "Full Screen") == 0)
      saw_fullscreen = TRUE;
    else if (g_strcmp0(entry->label, "Close Window") == 0) {
      saw_close = TRUE;
      g_assert_cmpstr(entry->command, ==, "close");
      g_assert_cmpint(entry_at(menu, i - 1)->kind, ==, WHELK_MENU_SEPARATOR);
    }
  }

  g_assert_true(saw_left);
  g_assert_true(saw_fullscreen);
  g_assert_true(saw_close);

  whelk_menu_free(menu);

  WhelkMenu *empty = whelk_app_menu(NULL);

  g_assert_cmpuint(empty->entries->len, ==, 0);
  whelk_menu_free(empty);
}

static void test_synthesize_without_tool(void) {
  /* No program can type a chord, and the application declares nothing, so
   * there is no menu that could work -- and no empty File button either. */
  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.example.Unknown", NULL, NULL, 0, NULL, NULL, NULL,
      WHELK_CHORD_CUA, NULL);

  g_assert_cmpuint(bar->len, ==, 0);
}

static void test_synthesize_without_close_command(void) {
  /* A session that cannot close windows gets no dead entry, and no rule with
   * nothing under it. */
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      NULL, ids, labels, 1, NULL, NULL, NULL, WHELK_CHORD_CUA, NULL);
  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpuint(file->entries->len, ==, 1);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
}

static void test_dictionary_menus(void) {
  const char *source = NULL;

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.mozilla.firefox", NULL, NULL, 0, NULL,
      &(WhelkSessionWindowActions){.close = "niri msg action close-window"},
      "wtype", WHELK_CHORD_CUA, &source);

  /* The dictionary answers for Firefox: File, Edit and View of its own.  The
   * close verb goes in File, and no arranging verbs means no Window menu. */
  g_assert_cmpstr(source, ==, "Firefox");
  g_assert_cmpuint(bar->len, ==, 3);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Tab");
  g_assert_cmpstr(entry_at(file, 0)->accel, ==, "Ctrl+T");
  g_assert_cmpstr(entry_at(file, 0)->command, ==,
                  "wtype -M ctrl -k t -m ctrl");

  g_assert_cmpstr(menu_at(bar, 1)->label, ==, "Edit");
  g_assert_cmpstr(menu_at(bar, 2)->label, ==, "View");
  g_assert_cmpstr(entry_at(menu_at(bar, 2), 0)->label, ==, "Reload");

  /* Close is a File action; with no arranging verbs passed there is no Window
   * menu at all. */
  g_assert_cmpuint(bar->len, ==, 3);
  gboolean saw_close = FALSE;
  gint close_at = -1;
  gint quit_at = -1;

  for (guint i = 0; i < file->entries->len; i++) {
    WhelkMenuEntry *entry = entry_at(file, i);

    if (g_strcmp0(entry->label, "Close Window") == 0 &&
        g_strcmp0(entry->command, "niri msg action close-window") == 0) {
      saw_close = TRUE;
      close_at = (gint)i;
    }

    if (g_strcmp0(entry->label, "Quit") == 0)
      quit_at = (gint)i;
  }

  g_assert_true(saw_close);
  g_assert_cmpint(close_at, <, quit_at);
  g_assert_cmpstr(entry_at(file, file->entries->len - 1)->label, ==, "Quit");
}

static void test_dictionary_deduplicates_file(void) {
  const char *ids[] = {"new-window"};
  const char *labels[] = {"New Window"};

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "chromium", ids, labels, 1, NULL, NULL, "wtype", WHELK_CHORD_CUA,
      NULL);
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
      "firefox", NULL, NULL, 0, NULL, NULL, "xdotool", WHELK_CHORD_CUA,
      NULL);

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
      "firefox", ids, labels, 1, NULL, NULL, NULL, WHELK_CHORD_CUA,
      &source);

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

/* Every curated application, rendered the way the bar renders it: if one
 * chord in the table cannot be translated, its entry would be dead, and this
 * is where that is caught before a release. */
static void test_every_dictionary_app_renders(void) {
  g_assert_cmpuint(whelk_menudict_size(), >=, 90);

  for (guint i = 0; i < whelk_menudict_size(); i++) {
    const WhelkDictApp *app = whelk_menudict_at(i);
    const char *source = NULL;

    g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
        app->ids[0], NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_CUA,
        &source);

    g_assert_cmpstr(source, ==, app->name);
    g_assert_cmpuint(bar->len, >=, 1);

    for (guint m = 0; m < bar->len; m++) {
      WhelkMenu *menu = menu_at(bar, m);

      g_assert_cmpuint(menu->entries->len, >, 0);

      for (guint e = 0; e < menu->entries->len; e++) {
        WhelkMenuEntry *entry = entry_at(menu, e);

        if (entry->kind == WHELK_MENU_SEPARATOR)
          continue;

        g_assert_nonnull(entry->accel);
        g_assert_cmpuint(strlen(entry->accel), >, 0);
        g_assert_true(entry->enabled);
        g_assert_nonnull(entry->command);
      }
    }
  }
}

static GVariant *parse(const char *text);

static void test_dictionary_emacs_items(void) {
  /* The table stores this chord as S-o, the way an Emacs user writes it. */
  const char *source = NULL;

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "qutebrowser", NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_CUA,
      &source);
  WhelkMenuEntry *new_tab = entry_at(menu_at(bar, 0), 1);

  g_assert_cmpstr(source, ==, "qutebrowser");
  g_assert_cmpstr(new_tab->label, ==, "Open in New Tab");
  g_assert_cmpstr(new_tab->accel, ==, "Shift+O");
  g_assert_cmpstr(new_tab->command, ==, "wtype -M shift -k o -m shift");

  /* Asked for Emacs notation, the same entry is written C-S-t style. */
  g_autoptr(GPtrArray) emacs = whelk_menubar_for_app(
      "falkon", NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_EMACS,
      NULL);

  g_assert_cmpstr(entry_at(menu_at(emacs, 0), 0)->accel, ==, "C-t");
}

static void test_emacs_multi_key_sequence(void) {
  /* Emacs stores Save as one sequence, C-x C-s.  The menu must show both
   * chords and type both, not mistake the space for the end of the chord. */
  const char *source = NULL;

  g_autoptr(GPtrArray) bar = whelk_menubar_for_app(
      "org.gnu.emacs", NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_CUA,
      &source);

  g_assert_cmpstr(source, ==, "Emacs");
  g_assert_cmpstr(menu_at(bar, 0)->label, ==, "File");

  WhelkMenuEntry *save = entry_at(menu_at(bar, 0), 1);

  g_assert_cmpstr(save->label, ==, "Save");
  g_assert_cmpstr(save->accel, ==, "Ctrl+X Ctrl+S");
  g_assert_cmpstr(save->command, ==,
                  "wtype -M ctrl -k x -m ctrl && "
                  "wtype -M ctrl -k s -m ctrl");

  /* The same entry in Emacs notation is the spelling an Emacs user knows. */
  g_autoptr(GPtrArray) emacs = whelk_menubar_for_app(
      "emacs", NULL, NULL, 0, NULL, NULL, "wtype", WHELK_CHORD_EMACS,
      NULL);

  g_assert_cmpstr(entry_at(menu_at(emacs, 0), 1)->accel, ==, "C-x C-s");
}

static void test_gtk_accelerators(void) {
  g_autoptr(GVariant) reply = parse(
      "[ (uint32 0, uint32 0, [ {'label': <'File'>, ':submenu': <(uint32 0, uint32 1)>} ]),"
      "  (uint32 0, uint32 1, [ {'label': <'Quit'>, 'action': <'app.quit'>, 'accel': <'<Primary>q'>},"
      "                         {'label': <'Copy'>, 'action': <'win.copy'>, 'accel': <'<Shift><Control>a'>} ]) ]");

  g_autoptr(GPtrArray) cua =
      whelk_menubar_from_gtk(reply, 0, 0, WHELK_CHORD_CUA);

  g_assert_cmpstr(entry_at(menu_at(cua, 0), 0)->accel, ==, "Ctrl+Q");
  g_assert_cmpstr(entry_at(menu_at(cua, 0), 1)->accel, ==, "Ctrl+Shift+A");

  g_autoptr(GPtrArray) emacs =
      whelk_menubar_from_gtk(reply, 0, 0, WHELK_CHORD_EMACS);

  g_assert_cmpstr(entry_at(menu_at(emacs, 0), 0)->accel, ==, "C-q");
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

  g_autoptr(GPtrArray) bar =
      whelk_menubar_from_gtk(reply, 0, 0, WHELK_CHORD_CUA);

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

  g_autoptr(GPtrArray) bar =
      whelk_menubar_from_gtk(reply, 0, 0, WHELK_CHORD_CUA);
  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpuint(file->entries->len, ==, 3);
  g_assert_cmpint(entry_at(file, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(file, 2)->label, ==, "Close");
}

static void test_gtk_nothing(void) {
  g_autoptr(GPtrArray) none =
      whelk_menubar_from_gtk(NULL, 0, 0, WHELK_CHORD_CUA);

  g_assert_cmpuint(none->len, ==, 0);

  g_autoptr(GVariant) wrong = g_variant_new_string("not a menu");

  g_autoptr(GPtrArray) also_none =
      whelk_menubar_from_gtk(wrong, 0, 0, WHELK_CHORD_CUA);

  g_assert_cmpuint(also_none->len, ==, 0);
}

/* com.canonical.dbusmenu's answer: a root whose children are the menubar's
 * menus.  The two things worth pinning are the shape (menus then entries, a
 * separator kept as a separator) and the id that has to travel with an entry,
 * because choosing it is an event sent back to the object that published it. */
static GVariant *dbusmenu_node(guint id, GVariantBuilder *props,
                               GVariantBuilder *children) {
  return g_variant_new_variant(
      g_variant_new("(i@a{sv}@av)", id, g_variant_builder_end(props),
                    g_variant_builder_end(children)));
}

static void test_dbusmenu_layout(void) {
  GVariantBuilder new_props;
  GVariantBuilder new_children;
  GVariantBuilder shortcuts;
  GVariantBuilder sep_props;
  GVariantBuilder sep_children;
  GVariantBuilder quit_props;
  GVariantBuilder quit_children;
  GVariantBuilder file_props;
  GVariantBuilder file_children;
  GVariantBuilder root_props;
  GVariantBuilder root_children;
  const char *chord[] = {"Control", "N", NULL};

  g_variant_builder_init(&new_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_init(&shortcuts, G_VARIANT_TYPE("aas"));
  g_variant_builder_add(&shortcuts, "@as", g_variant_new_strv(chord, -1));
  g_variant_builder_add(&new_props, "{sv}", "label",
                        g_variant_new_string("New Window"));
  g_variant_builder_add(&new_props, "{sv}", "shortcut",
                        g_variant_builder_end(&shortcuts));
  g_variant_builder_init(&new_children, G_VARIANT_TYPE("av"));

  g_variant_builder_init(&sep_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&sep_props, "{sv}", "type",
                        g_variant_new_string("separator"));
  g_variant_builder_init(&sep_children, G_VARIANT_TYPE("av"));

  g_variant_builder_init(&quit_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&quit_props, "{sv}", "label",
                        g_variant_new_string("Quit"));
  g_variant_builder_add(&quit_props, "{sv}", "enabled",
                        g_variant_new_boolean(FALSE));
  g_variant_builder_init(&quit_children, G_VARIANT_TYPE("av"));

  g_variant_builder_init(&file_children, G_VARIANT_TYPE("av"));
  g_variant_builder_add_value(
      &file_children, dbusmenu_node(1, &new_props, &new_children));
  g_variant_builder_add_value(
      &file_children, dbusmenu_node(2, &sep_props, &sep_children));
  g_variant_builder_add_value(
      &file_children, dbusmenu_node(3, &quit_props, &quit_children));

  g_variant_builder_init(&file_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_add(&file_props, "{sv}", "label",
                        g_variant_new_string("File"));
  g_variant_builder_add(&file_props, "{sv}", "children-display",
                        g_variant_new_string("submenu"));

  g_variant_builder_init(&root_props, G_VARIANT_TYPE("a{sv}"));
  g_variant_builder_init(&root_children, G_VARIANT_TYPE("av"));
  g_variant_builder_add_value(
      &root_children, dbusmenu_node(10, &file_props, &file_children));

  g_autoptr(GVariant) layout = g_variant_ref_sink(
      g_variant_new("(i@a{sv}@av)", 0, g_variant_builder_end(&root_props),
                    g_variant_builder_end(&root_children)));
  g_autoptr(GPtrArray) bar =
      whelk_menubar_from_dbusmenu(layout, ":1.42", "/MenuBar/1",
                                  WHELK_CHORD_CUA);

  g_assert_cmpuint(bar->len, ==, 1);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 3);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
  g_assert_cmpstr(entry_at(file, 0)->accel, ==, "Ctrl+N");
  g_assert_cmpstr(entry_at(file, 0)->dbus_path, ==, "/MenuBar/1");
  g_assert_cmpint(entry_at(file, 0)->dbus_item, ==, 1);
  g_assert_cmpint(entry_at(file, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_false(entry_at(file, 2)->enabled);
}

/* The menu a configuration writes: one tab-separated line per entry, `-' for a
 * rule, and `File>Export' for a submenu. */
static void test_text_menubar(void) {
  const char *text = "File\tNew Window\tC-n\tgtk-launch foo\n"
                     "File\t-\n"
                     "File>Export\tPDF…\t\tgs 'export pdf'\n"
                     "# a comment\n"
                     "Edit\tCopy\tCtrl+C\n";
  g_autoptr(GPtrArray) bar =
      whelk_menubar_from_text(text, WHELK_CHORD_CUA);

  g_assert_cmpuint(bar->len, ==, 2);

  WhelkMenu *file = menu_at(bar, 0);

  g_assert_cmpstr(file->label, ==, "File");
  g_assert_cmpuint(file->entries->len, ==, 3);
  g_assert_cmpstr(entry_at(file, 0)->label, ==, "New Window");
  g_assert_cmpstr(entry_at(file, 0)->accel, ==, "Ctrl+N");
  g_assert_cmpstr(entry_at(file, 0)->command, ==, "gtk-launch foo");
  g_assert_cmpint(entry_at(file, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_nonnull(entry_at(file, 2)->submenu);
  g_assert_cmpuint(entry_at(file, 2)->submenu->entries->len, ==, 1);

  /* An entry with no command is shown, and disabled: it cannot do anything, so
   * it must not pretend it can. */
  g_assert_false(entry_at(menu_at(bar, 1), 0)->enabled);
}

/* The shell menu the runtime reads: Scheme units, through the bridge. */
static void test_shell_menu(void) {
  g_autoptr(GPtrArray) bar = whelk_bridge_shell_menu(NULL);

  g_assert_cmpuint(bar->len, ==, 1);

  WhelkMenu *menu = menu_at(bar, 0);

  g_assert_cmpuint(menu->entries->len, >, 0);
  g_assert_cmpstr(entry_at(menu, 0)->label, ==, "About This System");
  g_assert_cmpstr(entry_at(menu, 0)->action, ==, "shell:about");

  gboolean recent = FALSE;
  gboolean force_quit = FALSE;

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = entry_at(menu, i);

    if (entry->submenu && g_strcmp0(entry->action, "shell:recent-items") == 0)
      recent = TRUE;

    if (g_strcmp0(entry->action, "shell:force-quit") == 0)
      force_quit = TRUE;
  }

  g_assert_true(recent);
  g_assert_true(force_quit);
}

/* A settings file writes its own first menu, and the runtime reads that one
 * rather than the default: the same road a bar's modules take. */
static void test_shell_menu_from_settings(void) {
  g_autofree char *path = g_build_filename(g_get_tmp_dir(),
                                           "whelk-shell-menu-test.scm", NULL);
  g_autofree char *settings = g_strdup_printf(
      "(configure-shell!\n"
      " #:config-path \"%s\"\n"
      " #:shell-menu\n"
      " (list (menu-item \"About This Machine\" \"shell:about\")\n"
      "       (menu-separator)\n"
      "       (menu-item \"Sleep\" \"systemctl suspend\")\n"
      "       (menu-submenu \"More\" (menu-item \"Launcher\" \"shell:launcher\"))))\n",
      path);

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  g_autoptr(GPtrArray) bar = whelk_bridge_shell_menu(path);

  g_assert_cmpuint(bar->len, ==, 1);

  WhelkMenu *menu = menu_at(bar, 0);

  g_assert_cmpstr(entry_at(menu, 0)->label, ==, "About This Machine");
  g_assert_cmpstr(entry_at(menu, 0)->action, ==, "shell:about");
  g_assert_cmpint(entry_at(menu, 1)->kind, ==, WHELK_MENU_SEPARATOR);
  g_assert_cmpstr(entry_at(menu, 2)->command, ==, "systemctl suspend");
  g_assert_nonnull(entry_at(menu, 3)->submenu);
  g_assert_cmpuint(entry_at(menu, 3)->submenu->entries->len, ==, 1);
  g_assert_cmpstr(entry_at(entry_at(menu, 3)->submenu, 0)->action, ==,
                  "shell:launcher");

  g_unlink(path);
}

static void test_clipboard_settings_from_scheme(void) {
  g_autofree char *path = g_build_filename(
      g_get_tmp_dir(), "whelk-clipboard-settings-test.scm", NULL);
  g_autofree char *settings = g_strdup_printf(
      "(configure-shell!\n"
      " #:config-path \"%s\"\n"
      " #:clipboard (clipboard-settings\n"
      "               #:persist \"/tmp/whelk-vault.variant\"\n"
      "               #:persist-only-pinned? #t\n"
      "               #:clear-on-lock? #t\n"
      "               #:max-entries 12\n"
      "               #:max-entry-bytes 4096\n"
      "               #:max-total-bytes 8192\n"
      "               #:ttl-seconds 3600))\n",
      path);

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  g_autoptr(WhelkClipboardConfig) config = whelk_bridge_clipboard(path);

  g_assert_true(config->configured);
  g_assert_true(config->persist);
  g_assert_cmpstr(config->persist_path, ==, "/tmp/whelk-vault.variant");
  g_assert_true(config->persist_only_pinned);
  g_assert_true(config->clear_on_lock);
  g_assert_cmpuint(config->max_entries, ==, 12);
  g_assert_cmpuint(config->max_entry_bytes, ==, 4096);
  g_assert_cmpuint(config->max_total_bytes, ==, 8192);
  g_assert_cmpuint(config->ttl_seconds, ==, 3600);

  g_unlink(path);
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
  g_test_add_func("/menutree/synthesize/new-window-and-verbs",
                  test_synthesize_new_window_and_verbs);
  g_test_add_func("/menutree/app-menu/window-actions",
                  test_app_menu_window_actions);
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
  g_test_add_func("/menutree/dictionary/renders",
                  test_every_dictionary_app_renders);
  g_test_add_func("/menutree/dictionary/emacs-items",
                  test_dictionary_emacs_items);
  g_test_add_func("/menutree/dictionary/emacs-sequence",
                  test_emacs_multi_key_sequence);
  g_test_add_func("/menutree/gtk/accelerators", test_gtk_accelerators);
  g_test_add_func("/menutree/gtk/menubar", test_gtk_menubar);
  g_test_add_func("/menutree/gtk/sections", test_gtk_sections_inside_a_menu);
  g_test_add_func("/menutree/gtk/nothing", test_gtk_nothing);
  g_test_add_func("/menutree/dbusmenu/layout", test_dbusmenu_layout);
  g_test_add_func("/menutree/text/menubar", test_text_menubar);
  g_test_add_func("/menutree/shell-menu", test_shell_menu);
  g_test_add_func("/menutree/shell-menu/from-settings",
                  test_shell_menu_from_settings);
  g_test_add_func("/menutree/clipboard/settings",
                  test_clipboard_settings_from_scheme);

  return g_test_run();
}

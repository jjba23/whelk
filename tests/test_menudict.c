// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menudict.h"

#include "guile_bridge.h"

#include <glib.h>
#include <string.h>

/* The dictionary's two jobs: find the right entry for whatever a compositor
 * calls an application, and be complete enough to type when it is found.
 *
 * The applications themselves are Scheme (scheme/whelk/menudict.scm), so these
 * tests load the real dictionary the way the shell does -- through the bridge,
 * from the module -- and then ask it questions.  A test that invented its own
 * table would pass while the shipped one was broken. */

static void load_dictionary(void) {
  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  whelk_menudict_load(whelk_bridge_menudict());
}

static void test_empty_until_loaded(void) {
  /* Nothing is compiled in: before Scheme has been asked, there is no table. */
  whelk_menudict_load(NULL);

  g_assert_cmpuint(whelk_menudict_size(), ==, 0);
  g_assert_null(whelk_menudict_at(0));
  g_assert_null(whelk_menudict_for_app("firefox"));

  load_dictionary();
}

static void test_lookup_by_id(void) {
  const WhelkDictApp *app = whelk_menudict_for_app("org.mozilla.firefox");

  g_assert_nonnull(app);
  g_assert_cmpstr(app->name, ==, "Firefox");
  g_assert_cmpuint(app->n_menus, >=, 3);
}

static void test_lookup_by_bare_name(void) {
  /* niri reports "firefox", the desktop entry says "org.mozilla.firefox". */
  g_assert_true(whelk_menudict_for_app("firefox") ==
                whelk_menudict_for_app("org.mozilla.firefox"));
  g_assert_true(whelk_menudict_for_app("nautilus") ==
                whelk_menudict_for_app("org.gnome.Nautilus"));
}

static void test_lookup_is_case_insensitive(void) {
  /* X11's WM_CLASS is capitalised like a class, not like a bus name. */
  g_assert_nonnull(whelk_menudict_for_app("Firefox"));
  g_assert_nonnull(whelk_menudict_for_app("Alacritty"));
  g_assert_nonnull(whelk_menudict_for_app("org.gnome.TextEditor"));
}

static void test_lookup_ignores_desktop_suffix(void) {
  g_assert_true(whelk_menudict_for_app("firefox.desktop") ==
                whelk_menudict_for_app("firefox"));
}

static void test_lookup_nothing(void) {
  g_assert_null(whelk_menudict_for_app("org.example.Nothing"));
  g_assert_null(whelk_menudict_for_app(""));
  g_assert_null(whelk_menudict_for_app(NULL));
}

static void test_covers_the_desktop(void) {
  /* The applications this machine actually has: a browser, a terminal, an
   * editor, a file manager, an office suite, a mail client, a drawing tool, a
   * player, a PDF reader and a chat client, each by its real app id. */
  static const char *const installed[] = {
      "firefox",
      "com.brave.Browser",
      "google-chrome",
      "org.alacritty.Alacritty",
      "kitty",
      "org.wezfurlong.wezterm",
      "org.gnome.Console",
      "org.gnome.TextEditor",
      "com.microsoft.edit",
      "org.kde.kate",
      "jetbrains-idea",
      "org.gnome.Nautilus",
      "org.kde.dolphin",
      "libreoffice-writer",
      "libreoffice-calc",
      "libreoffice-impress",
      "org.mozilla.Thunderbird",
      "org.gnome.Evolution",
      "gimp",
      "org.inkscape.Inkscape",
      "org.kde.krita",
      "blender",
      "vlc",
      "mpv",
      "org.gnome.Totem",
      "com.spotify.Client",
      "org.gnome.Evince",
      "org.kde.okular",
      "org.signal.Signal",
      "org.telegram.desktop",
      "com.discordapp.Discord",
      "com.slack.Slack",
      "org.wireshark.Wireshark",
      "org.keepassxc.KeePassXC",
      "audacity",
      "foot",
      "qutebrowser",
      "geany",
      "pcmanfm",
      "onlyoffice-desktopeditors",
      "zathura",
      "joplin",
      "postman",
      "freecad",
      "anki",
      "org.gnome.Settings",
  };

  for (guint i = 0; i < G_N_ELEMENTS(installed); i++) {
    const WhelkDictApp *app = whelk_menudict_for_app(installed[i]);

    g_assert_nonnull(app);
    g_assert_cmpuint(app->n_menus, >=, 1);
  }
}

static void test_every_entry_is_complete(void) {
  g_assert_cmpuint(whelk_menudict_size(), >=, 90);
  g_assert_null(whelk_menudict_at(whelk_menudict_size()));

  for (guint i = 0; i < whelk_menudict_size(); i++) {
    const WhelkDictApp *app = whelk_menudict_at(i);

    g_assert_nonnull(app);
    g_assert_nonnull(app->name);
    g_assert_cmpuint(app->n_menus, >=, 1);

    /* At least one id, and the list terminated: the lookup walks it. */
    g_assert_nonnull(app->ids);
    g_assert_nonnull(app->ids[0]);

    for (guint j = 0; app->ids[j]; j++) {
      g_assert_cmpuint(strlen(app->ids[j]), >, 0);

      /* Whatever it is called, the table finds it again. */
      g_assert_true(whelk_menudict_for_app(app->ids[j]) == app);
    }

    for (guint m = 0; m < app->n_menus; m++) {
      const WhelkDictMenu *menu = &app->menus[m];

      g_assert_nonnull(menu->label);
      g_assert_cmpuint(strlen(menu->label), >, 0);
      g_assert_cmpuint(menu->n_items, >, 0);

      /* Two entries typing one chord in one menu: one of them is a typo and
       * the other is unreachable.  Compared after parsing, so the short
       * Emacs form and the long form are held to the same rule. */
      GHashTable *chords = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                 g_free, NULL);

      for (guint k = 0; k < menu->n_items; k++) {
        const WhelkDictItem *item = &menu->items[k];

        g_assert_nonnull(item->label);
        g_assert_cmpuint(strlen(item->label), >, 0);
        /* An entry without its chord would show a shortcut it cannot type. */
        g_assert_nonnull(item->accel);
        g_assert_cmpuint(strlen(item->accel), >, 0);
        g_assert_nonnull(item->key);
        g_assert_cmpuint(strlen(item->key), >, 0);
        /* The modifiers are either #f (a bare key) or a chord to hold. */
        if (item->modifiers)
          g_assert_cmpuint(strlen(item->modifiers), >, 0);

        g_autofree char *signature = g_strdup_printf(
            "%s|%s", item->modifiers ? item->modifiers : "", item->key);

        g_assert_false(g_hash_table_contains(chords, signature));
        g_hash_table_add(chords, g_steal_pointer(&signature));
      }

      g_hash_table_unref(chords);
    }
  }
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  load_dictionary();

  g_test_add_func("/menudict/table/empty", test_empty_until_loaded);
  g_test_add_func("/menudict/lookup/id", test_lookup_by_id);
  g_test_add_func("/menudict/lookup/bare-name", test_lookup_by_bare_name);
  g_test_add_func("/menudict/lookup/case", test_lookup_is_case_insensitive);
  g_test_add_func("/menudict/lookup/desktop-suffix",
                  test_lookup_ignores_desktop_suffix);
  g_test_add_func("/menudict/lookup/nothing", test_lookup_nothing);
  g_test_add_func("/menudict/table/installed", test_covers_the_desktop);
  g_test_add_func("/menudict/table/complete", test_every_entry_is_complete);

  return g_test_run();
}

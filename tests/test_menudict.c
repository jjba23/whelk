// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menudict.h"

#include <glib.h>
#include <string.h>

/* The dictionary's two jobs: find the right entry for whatever a compositor
 * calls an application, and be complete enough to type when it is found. */

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

static void test_every_entry_is_complete(void) {
  g_assert_cmpuint(whelk_menudict_size(), >=, 9);
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

      for (guint k = 0; k < menu->n_items; k++) {
        const WhelkDictItem *item = &menu->items[k];

        g_assert_nonnull(item->label);
        g_assert_cmpuint(strlen(item->label), >, 0);
        /* An entry without its chord would show a shortcut it cannot type. */
        g_assert_nonnull(item->accel);
        g_assert_cmpuint(strlen(item->accel), >, 0);
        g_assert_nonnull(item->key);
        g_assert_cmpuint(strlen(item->key), >, 0);
      }
    }
  }
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/menudict/lookup/id", test_lookup_by_id);
  g_test_add_func("/menudict/lookup/bare-name", test_lookup_by_bare_name);
  g_test_add_func("/menudict/lookup/case", test_lookup_is_case_insensitive);
  g_test_add_func("/menudict/lookup/desktop-suffix",
                  test_lookup_ignores_desktop_suffix);
  g_test_add_func("/menudict/lookup/nothing", test_lookup_nothing);
  g_test_add_func("/menudict/table/complete", test_every_entry_is_complete);

  return g_test_run();
}

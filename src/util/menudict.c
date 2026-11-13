// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menudict.h"

#include <string.h>

#include "logger.h"

/* =========================================================================
 * THE TABLE AT RUNTIME
 * =========================================================================
 * The applications are Scheme's business (see scheme/whelk/menudict.scm): this
 * file holds what the bridge handed over at startup and answers the one
 * question the global menu asks -- which entry belongs to this app id.
 */

static GPtrArray *dictionary;

/* =========================================================================
 * BUILDING THE TABLE
 * ========================================================================= */

WhelkDictItem *whelk_dict_items_new(guint n_items) {
  return g_new0(WhelkDictItem, MAX(n_items, 1u));
}

WhelkDictMenu *whelk_dict_menus_new(guint n_menus) {
  return g_new0(WhelkDictMenu, MAX(n_menus, 1u));
}

WhelkDictApp *whelk_dict_app_new(const char *name, char **ids,
                                 WhelkDictMenu *menus, guint n_menus) {
  WhelkDictApp *app = g_new0(WhelkDictApp, 1);

  app->name = name;
  app->ids = ids;
  app->menus = menus;
  app->n_menus = n_menus;

  return app;
}

GPtrArray *whelk_menudict_table_new(void) {
  return g_ptr_array_new_with_free_func(NULL);
}

static void dict_item_clear(WhelkDictItem *item) {
  g_free((char *)item->label);
  g_free((char *)item->accel);

  for (guint i = 0; i < item->n_chords; i++) {
    g_free(item->modifiers[i]);
    g_free(item->keys[i]);
  }

  g_free(item->modifiers);
  g_free(item->keys);
}

static void dict_menu_clear(WhelkDictMenu *menu) {
  for (guint i = 0; i < menu->n_items; i++)
    dict_item_clear(&menu->items[i]);

  g_free((char *)menu->label);
  g_free(menu->items);
}

static void dict_app_free(gpointer data) {
  WhelkDictApp *app = data;

  if (!app)
    return;

  for (guint i = 0; i < app->n_menus; i++)
    dict_menu_clear(&app->menus[i]);

  g_free((char *)app->name);
  g_strfreev(app->ids);
  g_free(app->menus);
  g_free(app);
}

void whelk_menudict_load(GPtrArray *table) {
  if (dictionary)
    g_ptr_array_unref(dictionary);

  dictionary = table;

  if (dictionary)
    g_ptr_array_set_free_func(dictionary, dict_app_free);

  /* What the configuration left the dictionary: the built-in table, plus what
   * a settings file added or replaced.  Nothing to do when it is empty -- an
   * installed shell without Guile still gets the shared Edit and View. */
  if (dictionary)
    LOG_C_INFO("MenuDict", "%u application(s) in the dictionary", dictionary->len);
}

/* =========================================================================
 * LOOKUP
 * =========================================================================
 * A compositor reports an application id three ways: niri and sway use the
 * Wayland app id ("org.mozilla.firefox"), X11 gives WM_CLASS ("Firefox"), and
 * a desktop entry is "firefox.desktop".  All three find the same entry: the
 * comparison ignores case, drops a trailing ".desktop", and accepts a dotted
 * suffix in either direction.
 */

static char *normalize(const char *app_id) {
  if (!app_id)
    return NULL;

  char *copy = g_ascii_strdown(app_id, -1);

  if (g_str_has_suffix(copy, ".desktop")) {
    char *shorter = g_strndup(copy, strlen(copy) - strlen(".desktop"));

    g_free(copy);
    copy = shorter;
  }

  return copy;
}

static gboolean ids_match(const char *app, const char *id) {
  if (!app || !*app || !id || !*id)
    return FALSE;

  g_autofree char *normalized = normalize(id);
  const gsize app_length = strlen(app);
  const gsize id_length = strlen(normalized);

  if (g_strcmp0(app, normalized) == 0)
    return TRUE;

  /* "firefox" answers to "org.mozilla.firefox", and the other way round. */
  if (app_length > id_length && g_str_has_suffix(app, normalized) &&
      app[app_length - id_length - 1] == '.')
    return TRUE;

  if (id_length > app_length && g_str_has_suffix(normalized, app) &&
      normalized[id_length - app_length - 1] == '.')
    return TRUE;

  return FALSE;
}

const WhelkDictApp *whelk_menudict_for_app(const char *app_id) {
  g_autofree char *app = normalize(app_id);

  if (!app || *app == '\0' || !dictionary)
    return NULL;

  for (guint i = 0; i < dictionary->len; i++) {
    const WhelkDictApp *entry = g_ptr_array_index(dictionary, i);

    for (guint j = 0; entry->ids && entry->ids[j]; j++)
      if (ids_match(app, entry->ids[j]))
        return entry;
  }

  return NULL;
}

gboolean whelk_menudict_ids_match(const char *app_id, const char *id) {
  g_autofree char *app = normalize(app_id);

  return app && *app != '\0' && ids_match(app, id);
}

guint whelk_menudict_size(void) {
  return dictionary ? dictionary->len : 0;
}

const WhelkDictApp *whelk_menudict_at(guint index) {
  if (!dictionary || index >= dictionary->len)
    return NULL;

  return g_ptr_array_index(dictionary, index);
}

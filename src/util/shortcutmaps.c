// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/shortcutmaps.h"

#include <string.h>

#include "util/json.h"
#include "util/menudict.h"

/* =========================================================================
 * THE MAPPING INDEX
 * =========================================================================
 * Orbit's corpus is a directory of JSON files, one application each.  Scanning
 * 293 files on every focus change would be wasteful, so the identifiers are
 * indexed once and the matching file is parsed into a menu tree only when an
 * application actually asks for it.  The index is invalidated explicitly by
 * the tests and by a future reload path; the files are configuration, not a
 * running service.
 */

typedef struct {
  char *path;
  char **ids;
  gboolean blank;
} ShortcutMapping;

static GPtrArray *mappings;
static gboolean scanned;

static void mapping_free(gpointer data) {
  ShortcutMapping *mapping = data;

  if (!mapping)
    return;

  g_free(mapping->path);
  g_strfreev(mapping->ids);
  g_free(mapping);
}

static void clear_mappings(void) {
  g_clear_pointer(&mappings, g_ptr_array_unref);
  scanned = FALSE;
}

void whelk_shortcutmaps_reset(void) { clear_mappings(); }

/* =========================================================================
 * READING THE INDEX
 * ========================================================================= */

static ShortcutMapping *read_mapping(const char *path) {
  g_autofree char *text = NULL;
  g_autoptr(GError) error = NULL;

  if (!g_file_get_contents(path, &text, NULL, &error))
    return NULL;

  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root)
    return NULL;

  if (whelk_json_type(root) != WHELK_JSON_OBJECT) {
    whelk_json_free(root);
    return NULL;
  }

  WhelkJson *identifiers = whelk_json_object_get(root, "identifiers");

  if (!identifiers || whelk_json_type(identifiers) != WHELK_JSON_ARRAY ||
      whelk_json_array_length(identifiers) == 0) {
    whelk_json_free(root);
    return NULL;
  }

  const guint count = whelk_json_array_length(identifiers);
  char **ids = g_new0(char *, count + 1);

  for (guint i = 0; i < count; i++) {
    const char *id = whelk_json_string(whelk_json_array_get(identifiers, i));

    ids[i] = g_strdup(id ? id : "");
  }

  ShortcutMapping *mapping = g_new0(ShortcutMapping, 1);

  mapping->path = g_strdup(path);
  mapping->ids = ids;
  mapping->blank = whelk_json_object_bool(root, "blank", FALSE);

  whelk_json_free(root);

  return mapping;
}

static void add_mapping_path(GPtrArray *paths, const char *path) {
  if (!path || !*path)
    return;

  for (guint i = 0; i < paths->len; i++)
    if (g_strcmp0(g_ptr_array_index(paths, i), path) == 0)
      return;

  g_ptr_array_add(paths, g_strdup(path));
}

static void collect_json_files(GPtrArray *paths, const char *directory) {
  GDir *dir = g_dir_open(directory, 0, NULL);

  if (!dir)
    return;

  const char *name = NULL;

  while ((name = g_dir_read_name(dir)) != NULL) {
    if (!g_str_has_suffix(name, ".json"))
      continue;

    g_ptr_array_add(paths, g_build_filename(directory, name, NULL));
  }

  g_dir_close(dir);
}

static void add_search_path(GPtrArray *paths, const char *path) {
  if (!path || !*path)
    return;

  if (g_file_test(path, G_FILE_TEST_IS_DIR))
    collect_json_files(paths, path);
  else if (g_file_test(path, G_FILE_TEST_IS_REGULAR))
    add_mapping_path(paths, path);
}

static gint compare_paths(gconstpointer a, gconstpointer b) {
  return g_strcmp0(*(const char *const *)a, *(const char *const *)b);
}

static void build_index(void) {
  if (scanned)
    return;

  scanned = TRUE;
  mappings = g_ptr_array_new_with_free_func(mapping_free);

  g_autoptr(GPtrArray) paths =
      g_ptr_array_new_with_free_func((GDestroyNotify)g_free);
  const char *override = g_getenv("WHELK_SHORTCUT_MAPS");

  if (override && *override) {
    add_search_path(paths, override);
  } else {
    g_autofree char *config =
        g_build_filename(g_get_user_config_dir(), "global-menu", "shortcuts",
                         "apps", NULL);
    g_autofree char *data =
        g_build_filename(g_get_user_data_dir(), "global-menu", "shortcuts",
                         "apps", NULL);

    add_search_path(paths, config);
    add_search_path(paths, data);
    add_search_path(paths, "/usr/share/global-menu/shortcuts/apps");
  }

  /* Deterministic winner when two directories provide the same id. */
  g_ptr_array_sort(paths, compare_paths);

  for (guint i = 0; i < paths->len; i++) {
    ShortcutMapping *mapping = read_mapping(g_ptr_array_index(paths, i));

    if (mapping)
      g_ptr_array_add(mappings, mapping);
  }
}

/* =========================================================================
 * THE SHORTCUTS IN ONE MAPPING
 * ========================================================================= */

/* Orbit stores GTK accelerator strings.  A space separates a sequence of
 * chords (": w Return", "<Ctrl>x <Ctrl>s"), which is the same shape whelk's
 * own menu model can type. */
static gboolean parse_accelerator_sequence(const char *text,
                                           WhelkChordSequence *sequence) {
  return whelk_chord_sequence_parse_gtk(text, sequence);
}

static gboolean menu_has_entries(const WhelkMenu *menu) {
  return menu && menu->entries && menu->entries->len > 0;
}

static void trim_separator(WhelkMenu *menu) {
  while (menu_has_entries(menu)) {
    WhelkMenuEntry *last = g_ptr_array_index(menu->entries,
                                             menu->entries->len - 1);

    if (last->kind != WHELK_MENU_SEPARATOR)
      break;

    g_ptr_array_remove_index(menu->entries, menu->entries->len - 1);
  }
}

static void append_separator(WhelkMenu *menu) {
  if (!menu_has_entries(menu))
    return;

  WhelkMenuEntry *last =
      g_ptr_array_index(menu->entries, menu->entries->len - 1);

  if (last->kind != WHELK_MENU_SEPARATOR)
    whelk_menu_add_separator(menu);
}

static void build_items(WhelkMenu *menu, WhelkJson *items,
                        const char *shortcut_tool, WhelkChordStyle style);

static gboolean build_submenu(WhelkMenu *menu, WhelkJson *item,
                              const char *label, const char *shortcut_tool,
                              WhelkChordStyle style) {
  WhelkJson *children = whelk_json_object_get(item, "items");

  if (!children || whelk_json_type(children) != WHELK_JSON_ARRAY)
    return FALSE;

  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  entry->submenu = whelk_menu_new(label);
  build_items(entry->submenu, children, shortcut_tool, style);

  if (!menu_has_entries(entry->submenu)) {
    g_ptr_array_remove_index(menu->entries, menu->entries->len - 1);
    return FALSE;
  }

  return TRUE;
}

static void build_item(WhelkMenu *menu, WhelkJson *item,
                       const char *shortcut_tool, WhelkChordStyle style) {
  if (!item || whelk_json_type(item) != WHELK_JSON_OBJECT)
    return;

  if (whelk_json_object_bool(item, "separator", FALSE)) {
    append_separator(menu);
    return;
  }

  const char *label = whelk_json_object_string(item, "label");

  if (!label || *label == '\0')
    return;

  if (build_submenu(menu, item, label, shortcut_tool, style))
    return;

  const char *shortcut = whelk_json_object_string(item, "shortcut");
  const char *action = whelk_json_object_string(item, "action");
  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  if (shortcut && *shortcut) {
    WhelkChordSequence sequence;

    if (parse_accelerator_sequence(shortcut, &sequence)) {
      entry->accel = whelk_chord_sequence_format(&sequence, style);
      entry->command =
          whelk_chord_sequence_command(shortcut_tool, &sequence);
      entry->enabled = entry->command != NULL;

      whelk_chord_sequence_clear(&sequence);
    } else {
      /* Keep the row and the text its author wrote: it is a mapping problem
       * to see, not an invitation to invent a different chord. */
      entry->accel = g_strdup(shortcut);
      entry->enabled = FALSE;
    }

    return;
  }

  /* An org.gtk.Actions name can be activated without a chord.  Orbit writes
   * GIMP's names without a prefix; the application object is the right target
   * for those.  A target is usually an integer the action needs, and whelk's
   * model carries only the string form, so leave such a row disabled rather
   * than sending the wrong parameter. */
  if (action && *action) {
    if (!whelk_json_object_get(item, "target"))
      entry->action = g_strdup(action);
    else
      entry->enabled = FALSE;

    return;
  }

  /* Orbit's placeholders are deliberate: an action reachable only through a
   * menu of its own or a mouse gesture.  Show it, but do not pretend it works. */
  entry->enabled = FALSE;
}

static void build_items(WhelkMenu *menu, WhelkJson *items,
                        const char *shortcut_tool, WhelkChordStyle style) {
  if (!items || whelk_json_type(items) != WHELK_JSON_ARRAY)
    return;

  for (guint i = 0; i < whelk_json_array_length(items); i++)
    build_item(menu, whelk_json_array_get(items, i), shortcut_tool, style);

  trim_separator(menu);
}

GPtrArray *whelk_shortcutmaps_menubar_for_app(const char *app_id,
                                              const char *shortcut_tool,
                                              WhelkChordStyle style) {
  GPtrArray *bar = whelk_menubar_new();

  if (!app_id || *app_id == '\0' || !shortcut_tool || *shortcut_tool == '\0')
    return bar;

  build_index();

  ShortcutMapping *match = NULL;

  for (guint i = 0; i < mappings->len && !match; i++) {
    ShortcutMapping *candidate = g_ptr_array_index(mappings, i);

    for (guint j = 0; candidate->ids[j]; j++)
      if (whelk_menudict_ids_match(app_id, candidate->ids[j])) {
        match = candidate;
        break;
      }
  }

  if (!match || match->blank)
    return bar;

  g_autofree char *text = NULL;
  g_autoptr(GError) error = NULL;

  if (!g_file_get_contents(match->path, &text, NULL, &error))
    return bar;

  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root)
    return bar;

  WhelkJson *menus = whelk_json_object_get(root, "menus");

  if (menus && whelk_json_type(menus) == WHELK_JSON_ARRAY) {
    for (guint i = 0; i < whelk_json_array_length(menus); i++) {
      WhelkJson *item = whelk_json_array_get(menus, i);

      if (!item || whelk_json_type(item) != WHELK_JSON_OBJECT)
        continue;

      const char *label = whelk_json_object_string(item, "label");
      WhelkJson *items = whelk_json_object_get(item, "items");

      if (!label || *label == '\0' || !items ||
          whelk_json_type(items) != WHELK_JSON_ARRAY)
        continue;

      WhelkMenu *menu = whelk_menu_new(label);

      build_items(menu, items, shortcut_tool, style);

      if (menu_has_entries(menu))
        g_ptr_array_add(bar, menu);
      else
        whelk_menu_free(menu);
    }
  }

  whelk_json_free(root);

  return bar;
}

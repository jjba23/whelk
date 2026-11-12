// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menutree.h"

#include "util/menudict.h"

#include <string.h>

#define WHELK_MENU_MAX_DEPTH 8

/* =========================================================================
 * THE MODEL
 * ========================================================================= */

static void menu_entry_free(WhelkMenuEntry *entry) {
  if (!entry)
    return;

  g_free(entry->label);
  g_free(entry->accel);
  g_free(entry->action);
  g_free(entry->target);
  g_free(entry->command);

  if (entry->submenu)
    whelk_menu_free(entry->submenu);

  g_free(entry);
}

WhelkMenu *whelk_menu_new(const char *label) {
  WhelkMenu *menu = g_new0(WhelkMenu, 1);

  menu->label = g_strdup(label ? label : "");
  menu->entries = g_ptr_array_new_with_free_func((GDestroyNotify)menu_entry_free);

  return menu;
}

void whelk_menu_free(WhelkMenu *menu) {
  if (!menu)
    return;

  g_free(menu->label);
  g_ptr_array_unref(menu->entries);
  g_free(menu);
}

WhelkMenuEntry *whelk_menu_add_entry(WhelkMenu *menu, const char *label) {
  WhelkMenuEntry *entry = g_new0(WhelkMenuEntry, 1);

  entry->kind = WHELK_MENU_ENTRY;
  entry->label = g_strdup(label ? label : "");
  entry->enabled = TRUE;

  g_ptr_array_add(menu->entries, entry);

  return entry;
}

WhelkMenuEntry *whelk_menu_add_separator(WhelkMenu *menu) {
  WhelkMenuEntry *entry = g_new0(WhelkMenuEntry, 1);

  entry->kind = WHELK_MENU_SEPARATOR;
  entry->enabled = TRUE;

  g_ptr_array_add(menu->entries, entry);

  return entry;
}

WhelkMenu *whelk_menu_add_submenu(WhelkMenu *menu, const char *label) {
  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  entry->submenu = whelk_menu_new(label);

  return entry->submenu;
}

GPtrArray *whelk_menubar_new(void) {
  return g_ptr_array_new_with_free_func((GDestroyNotify)whelk_menu_free);
}

/* =========================================================================
 * A MENUBAR AN APPLICATION EXPORTS
 * =========================================================================
 * `org.gtk.Menus.Start' answers with `a(uuaa{sv})': every menu of the requested
 * groups, as (group, menu, items).  Item attributes carry the label and the
 * action, `:section' points at another menu whose items belong at this point,
 * and `:submenu' points at the menu this item opens.
 */

static gchar *menu_key(guint group, guint menu) {
  return g_strdup_printf("%u:%u", group, menu);
}

static GVariant *find_menu(GHashTable *menus, guint group, guint menu) {
  g_autofree gchar *key = menu_key(group, menu);

  return g_hash_table_lookup(menus, key);
}

/* The (group, menu) an attribute of type `(uu)' points at. */
static gboolean link_target(GVariant *attrs, const char *attribute,
                            guint *group, guint *menu) {
  GVariant *link = g_variant_lookup_value(attrs, attribute,
                                          G_VARIANT_TYPE("(uu)"));

  if (!link)
    return FALSE;

  g_variant_get(link, "(uu)", group, menu);
  g_variant_unref(link);

  return TRUE;
}

static const char *item_label(GVariant *attrs) {
  const char *label = NULL;

  if (!g_variant_lookup(attrs, "label", "&s", &label))
    return NULL;

  return label;
}

static void append_items(WhelkMenu *menu, GVariant *items, GHashTable *menus,
                         guint depth);

static void append_item(WhelkMenu *menu, GVariant *attrs, GHashTable *menus,
                        guint depth) {
  if (depth > WHELK_MENU_MAX_DEPTH)
    return;

  guint group = 0;
  guint id = 0;

  /* A section is not an entry: its items are entries of the menu that holds
   * the link, and a rule between sections is what separates them. */
  if (link_target(attrs, ":section", &group, &id)) {
    GVariant *section = find_menu(menus, group, id);

    if (section) {
      if (menu->entries->len > 0)
        whelk_menu_add_separator(menu);

      append_items(menu, section, menus, depth + 1);
    }

    return;
  }

  const char *label = item_label(attrs);
  gboolean has_submenu = link_target(attrs, ":submenu", &group, &id);

  if (!label && !has_submenu)
    return;

  const char *action = NULL;
  g_variant_lookup(attrs, "action", "&s", &action);

  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label ? label : "");

  g_autofree char *action_copy = g_strdup(action);
  g_autofree char *target = NULL;
  GVariant *target_variant = g_variant_lookup_value(attrs, "target", NULL);

  if (target_variant) {
    if (g_variant_is_of_type(target_variant, G_VARIANT_TYPE_STRING))
      target = g_variant_dup_string(target_variant, NULL);

    g_variant_unref(target_variant);
  }

  entry->action = g_steal_pointer(&action_copy);
  entry->target = g_steal_pointer(&target);

  if (has_submenu) {
    GVariant *submenu_items = find_menu(menus, group, id);

    if (submenu_items) {
      entry->submenu = whelk_menu_new(entry->label);
      append_items(entry->submenu, submenu_items, menus, depth + 1);
    }
  }

  /* GTK marks an item with `enabled' only when it is disabled. */
  gboolean enabled = TRUE;

  if (g_variant_lookup(attrs, "enabled", "b", &enabled))
    entry->enabled = enabled;
}

static void append_items(WhelkMenu *menu, GVariant *items, GHashTable *menus,
                         guint depth) {
  GVariantIter iter;
  GVariant *attrs = NULL;

  g_variant_iter_init(&iter, items);

  while (g_variant_iter_next(&iter, "@a{sv}", &attrs)) {
    append_item(menu, attrs, menus, depth);
    g_variant_unref(attrs);
  }
}

/* The top of a menubar is a list of menus; a section there is a row of them. */
static void collect_top_level(GPtrArray *bar, GVariant *items,
                              GHashTable *menus, guint depth) {
  if (depth > WHELK_MENU_MAX_DEPTH)
    return;

  GVariantIter iter;
  GVariant *attrs = NULL;

  g_variant_iter_init(&iter, items);

  while (g_variant_iter_next(&iter, "@a{sv}", &attrs)) {
    guint group = 0;
    guint id = 0;

    if (link_target(attrs, ":section", &group, &id)) {
      GVariant *section = find_menu(menus, group, id);

      if (section)
        collect_top_level(bar, section, menus, depth + 1);

      g_variant_unref(attrs);
      continue;
    }

    const char *label = item_label(attrs);

    if (!label || *label == '\0') {
      g_variant_unref(attrs);
      continue;
    }

    WhelkMenu *menu = whelk_menu_new(label);

    if (link_target(attrs, ":submenu", &group, &id)) {
      GVariant *submenu_items = find_menu(menus, group, id);

      if (submenu_items)
        append_items(menu, submenu_items, menus, depth + 1);
    } else {
      /* A menubar entry that opens something itself: one entry, its own name. */
      const char *action = NULL;

      g_variant_lookup(attrs, "action", "&s", &action);

      WhelkMenuEntry *only = whelk_menu_add_entry(menu, label);

      only->action = g_strdup(action);
    }

    if (menu->entries->len > 0)
      g_ptr_array_add(bar, menu);
    else
      whelk_menu_free(menu);

    g_variant_unref(attrs);
  }
}

GPtrArray *whelk_menubar_from_gtk(GVariant *reply, guint group, guint menu) {
  GPtrArray *bar = whelk_menubar_new();

  if (!reply ||
      !g_variant_is_of_type(reply, G_VARIANT_TYPE("a(uuaa{sv})")))
    return bar;

  GHashTable *menus =
      g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                            (GDestroyNotify)g_variant_unref);

  GVariantIter iter;
  guint32 item_group = 0;
  guint32 item_menu = 0;
  GVariant *items = NULL;

  g_variant_iter_init(&iter, reply);

  while (g_variant_iter_next(&iter, "(uu@aa{sv})", &item_group, &item_menu,
                             &items))
    g_hash_table_insert(menus, menu_key(item_group, item_menu), items);

  GVariant *root = find_menu(menus, group, menu);

  if (root)
    collect_top_level(bar, root, menus, 0);

  g_hash_table_unref(menus);

  return bar;
}

/* =========================================================================
 * A MENUBAR THE SHELL MAKES
 * =========================================================================
 * An application that exports nothing still has a File menu's worth of
 * actions: the ones its desktop entry declares, plus closing its window, which
 * the compositor does itself.  Beyond that the menus come from what is known
 * about the application: the documented shortcuts of the applications in the
 * dictionary, or -- for an application nobody has written down yet -- the
 * shortcuts every application shares.
 *
 * Every entry made from a shortcut shows the chord it types on the right, and
 * is only offered when a tool can type it -- a menu entry that cannot work is
 * worse than an absent one.
 */

char *whelk_shortcut_command(const char *tool, const char *modifiers,
                             const char *key) {
  if (!tool || *tool == '\0' || !key || *key == '\0')
    return NULL;

  /* wtype presses and releases; xdotool is told the whole chord at once. */
  if (g_str_has_suffix(tool, "xdotool")) {
    g_autofree char *chord = NULL;

    if (!modifiers || *modifiers == '\0') {
      chord = g_strdup(key);
    } else {
      g_auto(GStrv) parts = g_strsplit(modifiers, " ", -1);
      GString *built = g_string_new(NULL);

      for (guint i = 0; parts[i]; i++) {
        if (*parts[i] == '\0')
          continue;

        g_string_append(built, parts[i]);
        g_string_append_c(built, '+');
      }

      g_string_append(built, key);
      chord = g_string_free(built, FALSE);
    }

    return g_strdup_printf("%s key %s", tool, chord);
  }

  if (!modifiers || *modifiers == '\0')
    return g_strdup_printf("%s -k %s", tool, key);

  return g_strdup_printf("%s -M %s -k %s -m %s", tool, modifiers, key,
                         modifiers);
}

static void add_shortcut(WhelkMenu *menu, const char *label, const char *accel,
                         const char *tool, const char *modifiers,
                         const char *key) {
  WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

  entry->accel = g_strdup(accel);
  entry->command = whelk_shortcut_command(tool, modifiers, key);
  entry->enabled = entry->command != NULL;
}

/* Is an entry of this name already in the menu?  A desktop entry and the
 * dictionary both know "New Window", and a menu says it once. */
static gboolean has_label(GPtrArray *entries, const char *label) {
  for (guint i = 0; i < entries->len; i++) {
    const WhelkMenuEntry *entry = g_ptr_array_index(entries, i);

    if (entry->label && g_ascii_strcasecmp(entry->label, label) == 0)
      return TRUE;
  }

  return FALSE;
}

/* One curated menu's items, leaving out the ones the File menu already has. */
static void append_dict_menu(WhelkMenu *menu, const WhelkDictMenu *dict,
                             const char *tool) {
  for (guint i = 0; i < dict->n_items; i++) {
    const WhelkDictItem *item = &dict->items[i];

    if (has_label(menu->entries, item->label))
      continue;

    add_shortcut(menu, item->label, item->accel, tool, item->modifiers,
                 item->key);
  }
}

/* A rule between two groups, and never two in a row or one at the end. */
static void separate(WhelkMenu *menu) {
  if (menu->entries->len == 0)
    return;

  const WhelkMenuEntry *last =
      g_ptr_array_index(menu->entries, menu->entries->len - 1);

  if (last->kind != WHELK_MENU_SEPARATOR)
    whelk_menu_add_separator(menu);
}

static gboolean has_item(WhelkMenu *menu) {
  for (guint i = 0; i < menu->entries->len; i++) {
    const WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_ENTRY)
      return TRUE;
  }

  return FALSE;
}

/* The shortcuts every application shares.  Edit and View, and nothing that
 * claims to be the application's own File menu. */
static void append_shared_menus(GPtrArray *bar, const char *tool) {
  WhelkMenu *edit = whelk_menu_new("Edit");

  add_shortcut(edit, "Undo", "Ctrl+Z", tool, "ctrl", "z");
  add_shortcut(edit, "Redo", "Ctrl+Shift+Z", tool, "ctrl shift", "z");
  whelk_menu_add_separator(edit);
  add_shortcut(edit, "Cut", "Ctrl+X", tool, "ctrl", "x");
  add_shortcut(edit, "Copy", "Ctrl+C", tool, "ctrl", "c");
  add_shortcut(edit, "Paste", "Ctrl+V", tool, "ctrl", "v");
  add_shortcut(edit, "Select All", "Ctrl+A", tool, "ctrl", "a");
  whelk_menu_add_separator(edit);
  add_shortcut(edit, "Find", "Ctrl+F", tool, "ctrl", "f");

  g_ptr_array_add(bar, edit);

  WhelkMenu *view = whelk_menu_new("View");

  add_shortcut(view, "Zoom In", "Ctrl++", tool, "ctrl", "plus");
  add_shortcut(view, "Zoom Out", "Ctrl+-", tool, "ctrl", "minus");
  add_shortcut(view, "Actual Size", "Ctrl+0", tool, "ctrl", "0");
  whelk_menu_add_separator(view);
  add_shortcut(view, "Full Screen", "F11", tool, NULL, "F11");

  g_ptr_array_add(bar, view);
}

GPtrArray *whelk_menubar_for_app(const char *app_id,
                                 const char *const *action_ids,
                                 const char *const *action_labels,
                                 guint action_count,
                                 const char *close_command,
                                 const char *shortcut_tool,
                                 const char **out_source) {
  GPtrArray *bar = whelk_menubar_new();
  const WhelkDictApp *dict = whelk_menudict_for_app(app_id);
  const gboolean can_type = shortcut_tool && *shortcut_tool != '\0';

  if (out_source)
    *out_source = (dict && can_type) ? dict->name : NULL;

  WhelkMenu *file = whelk_menu_new("File");

  for (guint i = 0; i < action_count; i++) {
    if (!action_ids[i] || *action_ids[i] == '\0')
      continue;

    WhelkMenuEntry *entry = whelk_menu_add_entry(
        file, (action_labels && action_labels[i]) ? action_labels[i]
                                                  : action_ids[i]);

    /* "desktop:<action>" is launched through the desktop entry, so the
     * application itself decides what the action does. */
    entry->action = g_strdup_printf("desktop:%s", action_ids[i]);
  }

  if (dict && can_type) {
    /* The dictionary's File is the application's own; its other menus follow
     * it in the order they are written. */
    for (guint i = 0; i < dict->n_menus; i++)
      if (g_strcmp0(dict->menus[i].label, "File") == 0) {
        separate(file);
        append_dict_menu(file, &dict->menus[i], shortcut_tool);
      }
  }

  /* Closing a window is the compositor's job, and its name differs per session;
   * a session that cannot close windows gets no entry rather than a dead one. */
  if (close_command && *close_command) {
    separate(file);

    WhelkMenuEntry *close = whelk_menu_add_entry(file, "Close Window");

    close->command = g_strdup(close_command);
  }

  /* A File with nothing in it is not a menu. */
  if (has_item(file))
    g_ptr_array_add(bar, file);
  else
    whelk_menu_free(file);

  if (dict && can_type) {
    for (guint i = 0; i < dict->n_menus; i++) {
      if (g_strcmp0(dict->menus[i].label, "File") == 0)
        continue;

      WhelkMenu *menu = whelk_menu_new(dict->menus[i].label);

      append_dict_menu(menu, &dict->menus[i], shortcut_tool);
      g_ptr_array_add(bar, menu);
    }
  } else if (!dict && can_type) {
    append_shared_menus(bar, shortcut_tool);
  }

  return bar;
}

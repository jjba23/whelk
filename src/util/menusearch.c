// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menusearch.h"

#include <string.h>

static GPtrArray *search_items = NULL;
static char *search_app_name = NULL;

static WhelkMenuEntry *entry_copy(const WhelkMenuEntry *entry) {
  WhelkMenuEntry *copy = g_new0(WhelkMenuEntry, 1);

  copy->kind = WHELK_MENU_ENTRY;
  copy->label = g_strdup(entry->label);
  copy->accel = g_strdup(entry->accel);
  copy->enabled = entry->enabled;
  copy->action = g_strdup(entry->action);
  copy->target = g_strdup(entry->target);
  copy->command = g_strdup(entry->command);
  copy->dbus_service = g_strdup(entry->dbus_service);
  copy->dbus_path = g_strdup(entry->dbus_path);
  copy->dbus_item = entry->dbus_item;

  return copy;
}

static void search_item_free(WhelkMenuSearchItem *item) {
  if (!item)
    return;

  g_free(item->path);
  g_free(item->label);
  g_free(item->accel);

  if (item->entry) {
    g_free(item->entry->label);
    g_free(item->entry->accel);
    g_free(item->entry->action);
    g_free(item->entry->target);
    g_free(item->entry->command);
    g_free(item->entry->dbus_service);
    g_free(item->entry->dbus_path);
    g_free(item->entry);
  }

  whelk_menu_action_context_clear(&item->context);
  g_free(item);
}

static void ensure_items(void) {
  if (!search_items)
    search_items = g_ptr_array_new_with_free_func(
        (GDestroyNotify)search_item_free);
}

/* Does the entry carry anything that could happen?  A row in the tree that
 * activates nothing is not a search result. */
static gboolean entry_activates(const WhelkMenuEntry *entry) {
  return (entry->command && *entry->command) ||
         (entry->action && *entry->action) ||
         (entry->dbus_service && entry->dbus_path);
}

static void append_menu(GPtrArray *items, WhelkMenu *menu,
                        const char *parent_path,
                        const WhelkMenuActionContext *context) {
  if (!menu || !menu->entries)
    return;

  g_autofree char *path =
      (parent_path && *parent_path)
          ? g_strdup_printf("%s > %s", parent_path,
                            menu->label ? menu->label : "")
          : g_strdup(menu->label ? menu->label : "");

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_SEPARATOR || !entry->enabled)
      continue;

    if (entry->submenu) {
      append_menu(items, entry->submenu, path, context);
      continue;
    }

    if (!entry->label || *entry->label == '\0' || !entry_activates(entry))
      continue;

    WhelkMenuSearchItem *item = g_new0(WhelkMenuSearchItem, 1);

    item->path = g_strdup_printf("%s > %s", path, entry->label);
    item->label = g_strdup(entry->label);
    item->accel = g_strdup(entry->accel);
    item->entry = entry_copy(entry);

    if (context) {
      whelk_menu_action_context_set(&item->context, context->bus_name,
                                    context->app_path, context->desktop_path);
    }

    g_ptr_array_add(items, item);
  }
}

void whelk_menu_search_publish(const char *app_name,
                               const WhelkMenuActionContext *context,
                               GPtrArray *menus) {
  ensure_items();
  g_ptr_array_set_size(search_items, 0);

  g_free(search_app_name);
  search_app_name = g_strdup(app_name ? app_name : "");

  if (!menus)
    return;

  for (guint i = 0; i < menus->len; i++)
    append_menu(search_items, g_ptr_array_index(menus, i), NULL, context);
}

void whelk_menu_search_clear(void) {
  ensure_items();
  g_ptr_array_set_size(search_items, 0);

  g_free(search_app_name);
  search_app_name = g_strdup("");
}

GPtrArray *whelk_menu_search_snapshot(void) {
  ensure_items();

  return search_items;
}

const char *whelk_menu_search_app_name(void) {
  return search_app_name ? search_app_name : "";
}

WhelkMenuSearchItem *whelk_menu_search_item_copy(const WhelkMenuSearchItem *item) {
  if (!item)
    return NULL;

  WhelkMenuSearchItem *copy = g_new0(WhelkMenuSearchItem, 1);

  copy->path = g_strdup(item->path);
  copy->label = g_strdup(item->label);
  copy->accel = g_strdup(item->accel);
  copy->entry = item->entry ? entry_copy(item->entry) : NULL;

  whelk_menu_action_context_set(&copy->context, item->context.bus_name,
                                item->context.app_path,
                                item->context.desktop_path);

  return copy;
}

void whelk_menu_search_item_free(WhelkMenuSearchItem *item) {
  search_item_free(item);
}

void whelk_menu_search_item_activate(const WhelkMenuSearchItem *item) {
  if (!item || !item->entry)
    return;

  whelk_menu_entry_activate(item->entry, &item->context);
}

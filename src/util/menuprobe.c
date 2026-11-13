// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menuprobe.h"

#include <string.h>

static void add_path(GPtrArray *paths, const char *path) {
  if (path && *path)
    g_ptr_array_add(paths, g_strdup(path));
}

static void add_owned_path(GPtrArray *paths, char *path) {
  if (!path)
    return;

  if (*path)
    g_ptr_array_add(paths, path);
  else
    g_free(path);
}

WhelkMenuProbeOrder *whelk_menu_probe_order_new(
    const WhelkFocusedPaths *paths, gboolean bus_name_is_valid,
    const char *app_path) {
  WhelkMenuProbeOrder *order = g_new0(WhelkMenuProbeOrder, 1);

  order->gtk_paths = g_ptr_array_new_with_free_func(g_free);
  order->dbus_paths = g_ptr_array_new_with_free_func(g_free);

  if (bus_name_is_valid) {
    /* The exact answers first.  They are absent on Wayland, and on X11 they
     * are what makes Firefox and LibreOffice work despite their WM_CLASS. */
    add_path(order->gtk_paths, paths ? paths->menubar_path : NULL);

    if (paths && paths->window_path && *paths->window_path)
      add_owned_path(order->gtk_paths,
                     g_strdup_printf("%s/menus/menubar",
                                     paths->window_path));

    add_path(order->gtk_paths, paths ? paths->appmenu_path : NULL);

    /* Then the protocol's conventional locations, application-wide and
     * window-scoped, and GTK's anonymous application. */
    if (app_path && *app_path) {
      add_owned_path(order->gtk_paths,
                     g_strdup_printf("%s/menus/menubar", app_path));
      add_owned_path(order->gtk_paths,
                     g_strdup_printf("%s/menus/AppMenu", app_path));
      add_owned_path(order->gtk_paths,
                     g_strdup_printf("%s/window/0/menus/menubar",
                                     app_path));
    }

    add_path(order->gtk_paths,
             "/org/gtk/Application/anonymous/menus/menubar");
  }

  add_path(order->dbus_paths, "/MenuBar/1");
  add_path(order->dbus_paths, "/MenuBar/0");
  add_path(order->dbus_paths, "/com/canonical/dbusmenu");

  order->ask_registrar = paths && paths->window_id != 0;

  return order;
}

void whelk_menu_probe_use_registrar_path(GPtrArray *dbus_paths,
                                         const char *path) {
  if (!dbus_paths || !path || *path == '\0')
    return;

  for (guint i = 0; i < dbus_paths->len;) {
    if (g_strcmp0(g_ptr_array_index(dbus_paths, i), path) == 0)
      g_ptr_array_remove_index(dbus_paths, i);
    else
      i++;
  }

  g_ptr_array_insert(dbus_paths, 0, g_strdup(path));
}

void whelk_menu_probe_order_free(WhelkMenuProbeOrder *order) {
  if (!order)
    return;

  g_clear_pointer(&order->gtk_paths, g_ptr_array_unref);
  g_clear_pointer(&order->dbus_paths, g_ptr_array_unref);
  g_free(order);
}

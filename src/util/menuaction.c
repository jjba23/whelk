// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/menuaction.h"

#include <gio/gdesktopappinfo.h>
#include <string.h>

#include "util/atspimenus.h"
#include "util/command.h"

void whelk_menu_action_context_set(WhelkMenuActionContext *context,
                                   const char *bus_name, const char *app_path,
                                   const char *desktop_path) {
  if (!context)
    return;

  g_free(context->bus_name);
  g_free(context->app_path);
  g_free(context->desktop_path);

  context->bus_name = g_strdup(bus_name);
  context->app_path = g_strdup(app_path);
  context->desktop_path = g_strdup(desktop_path);
}

void whelk_menu_action_context_clear(WhelkMenuActionContext *context) {
  if (!context)
    return;

  g_clear_pointer(&context->bus_name, g_free);
  g_clear_pointer(&context->app_path, g_free);
  g_clear_pointer(&context->desktop_path, g_free);
}

const char *whelk_menu_action_name(const char *action) {
  if (!action)
    return NULL;

  const char *dot = strchr(action, '.');

  return dot ? dot + 1 : action;
}

char *whelk_menu_action_object_path(const char *action, const char *app_path) {
  if (!app_path)
    return NULL;

  if (action && g_str_has_prefix(action, "win."))
    return g_strdup_printf("%s/windows/0", app_path);

  return g_strdup(app_path);
}

void whelk_menu_entry_activate(const WhelkMenuEntry *entry,
                               const WhelkMenuActionContext *context) {
  if (!entry || !entry->enabled)
    return;

  if (entry->command) {
    whelk_command_spawn(entry->command);
    return;
  }

  /* A DBusMenu entry is not an action name: it is an id on the object that
   * published the menu, and choosing it is an event sent back there. */
  if (entry->dbus_service && entry->dbus_path) {
    g_autoptr(GError) error = NULL;
    GDBusConnection *bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

    if (!bus)
      return;

    g_dbus_connection_call(bus, entry->dbus_service, entry->dbus_path,
                           "com.canonical.dbusmenu", "Event",
                           g_variant_new("(isvu)", entry->dbus_item, "clicked",
                                         g_variant_new_int32(0), 0),
                           NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL, NULL);
    g_object_unref(bus);
    return;
  }

  /* An AT-SPI entry is an accessible object, and choosing it is the action a
   * screen reader would perform.  The object lives on the accessibility bus,
   * not the session bus. */
  if (entry->atspi_service && entry->atspi_path) {
    whelk_atspi_activate(entry->atspi_service, entry->atspi_path,
                         entry->atspi_action);
    return;
  }

  if (!entry->action || !context)
    return;

  if (g_str_has_prefix(entry->action, "desktop:")) {
    if (context->desktop_path) {
      g_autoptr(GDesktopAppInfo) info =
          g_desktop_app_info_new_from_filename(context->desktop_path);

      if (info)
        g_desktop_app_info_launch_action(info, entry->action + 8, NULL);
    }

    return;
  }

  if (!context->bus_name || !context->app_path)
    return;

  g_autofree char *path =
      whelk_menu_action_object_path(entry->action, context->app_path);
  const char *name = whelk_menu_action_name(entry->action);

  if (!path || !name || !*name)
    return;

  g_autoptr(GError) error = NULL;
  g_autoptr(GDBusConnection) bus =
      g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

  if (!bus)
    return;

  GVariantBuilder parameters;
  GVariantBuilder platform;

  g_variant_builder_init(&parameters, G_VARIANT_TYPE("av"));

  if (entry->target)
    g_variant_builder_add(&parameters, "v",
                          g_variant_new_string(entry->target));

  g_variant_builder_init(&platform, G_VARIANT_TYPE("a{sv}"));

  g_dbus_connection_call(bus, context->bus_name, path, "org.gtk.Actions",
                         "Activate",
                         g_variant_new("(sava{sv})", name, &parameters,
                                       &platform),
                         NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL, NULL);
}

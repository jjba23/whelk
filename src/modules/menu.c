// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/desktop.h"
#include "util/session.h"

#include "style.h"
#include "util/command.h"
#include "util/json.h"
#include "util/menutree.h"

#include <gio/gdesktopappinfo.h>

/* =========================================================================
 * THE GLOBAL MENU
 * =========================================================================
 * The menu bar of the application in front, in the shell's own bar: File, Edit,
 * View, whichever menus the application has.
 *
 * Two producers, in that order.  An application that exports its menubar --
 * GTK's `gtk_application_set_menubar()' publishes it through `org.gtk.Menus' on
 * the application's bus name -- is read over D-Bus, so the labels and the
 * actions are its own.  Everything else gets a menubar the shell synthesizes:
 * the actions its desktop entry declares, closing its window, and the shortcuts
 * every application shares, typed by wtype when it is installed.
 *
 * The D-Bus read is a chain of asynchronous calls, so a slow or absent bus
 * costs a frame and never the bar.
 */

#define WHELK_MENU_ENTRY_POINTER "whelk-menu-entry"

typedef struct {
  char *app_id;
  char *bus_name;
  char *app_path;
  GDesktopAppInfo *desktop;
  char *shortcut_tool;
  gboolean show_shortcuts;
  GPtrArray *menus; /* of WhelkMenu */
  GtkWidget *row;
  guint generation;
} MenuData;

typedef struct {
  WhelkModule *module;
  guint generation;
  GDBusConnection *bus;
  GPtrArray *paths; /* candidate object paths, consumed */
  guint index;
} MenuProbe;

/* --- activation ----------------------------------------------------------- */

static void activate_entry(WhelkModule *module, WhelkMenuEntry *entry) {
  MenuData *data = module->data;

  if (!entry || !entry->enabled)
    return;

  if (entry->command) {
    whelk_command_spawn(entry->command);
    return;
  }

  if (!entry->action)
    return;

  if (g_str_has_prefix(entry->action, "desktop:")) {
    if (data->desktop)
      g_desktop_app_info_launch_action(data->desktop, entry->action + 8, NULL);

    return;
  }

  const char *name = strchr(entry->action, '.');
  const gboolean window_action = g_str_has_prefix(entry->action, "win.");

  name = name ? name + 1 : entry->action;

  g_autoptr(GError) error = NULL;
  GDBusConnection *bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

  if (!bus)
    return;

  g_autofree char *path =
      window_action ? g_strdup_printf("%s/windows/0", data->app_path)
                    : g_strdup(data->app_path);

  GVariantBuilder parameters;
  GVariantBuilder platform;

  g_variant_builder_init(&parameters, G_VARIANT_TYPE("av"));

  if (entry->target)
    g_variant_builder_add(&parameters, "v", g_variant_new_string(entry->target));

  g_variant_builder_init(&platform, G_VARIANT_TYPE("a{sv}"));

  g_dbus_connection_call(bus, data->bus_name, path, "org.gtk.Actions",
                         "Activate",
                         g_variant_new("(sava{sv})", name, &parameters,
                                       &platform),
                         NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL, NULL);

  g_object_unref(bus);
}

static void on_entry_clicked(GtkButton *button, gpointer user_data) {
  WhelkModule *module = user_data;
  WhelkMenuEntry *entry =
      g_object_get_data(G_OBJECT(button), WHELK_MENU_ENTRY_POINTER);

  /* A click closes the menu it came from, the way every menubar does. */
  GtkWidget *popover = gtk_widget_get_ancestor(GTK_WIDGET(button),
                                               GTK_TYPE_POPOVER);

  if (popover)
    gtk_popover_popdown(GTK_POPOVER(popover));

  activate_entry(module, entry);
}

/* --- rendering ------------------------------------------------------------ */

static void clear_row(MenuData *data) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(data->row)) != NULL)
    gtk_box_remove(GTK_BOX(data->row), child);
}

static void apply_popover_skin(WhelkModule *module, GtkPopover *popover) {
  whelk_style_popover(popover, module->bar);
}

static void fill_popover(WhelkModule *module, GtkPopover *popover,
                         WhelkMenu *menu) {
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  for (guint i = 0; i < menu->entries->len; i++) {
    WhelkMenuEntry *entry = g_ptr_array_index(menu->entries, i);

    if (entry->kind == WHELK_MENU_SEPARATOR) {
      gtk_box_append(GTK_BOX(box), gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));
      continue;
    }

    GtkWidget *button = gtk_button_new_with_label(entry->label);

    gtk_widget_add_css_class(button, "flat");
    gtk_widget_add_css_class(button, "whelk-menu-item");
    gtk_widget_set_halign(button, GTK_ALIGN_FILL);
    gtk_button_set_has_frame(GTK_BUTTON(button), FALSE);

    if (!entry->enabled)
      gtk_widget_set_sensitive(button, FALSE);

    if (entry->submenu && entry->submenu->entries->len > 0) {
      GtkWidget *submenu_button = gtk_menu_button_new();

      gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(submenu_button), FALSE);
      gtk_menu_button_set_child(GTK_MENU_BUTTON(submenu_button),
                                gtk_label_new(entry->label));
      gtk_widget_add_css_class(submenu_button, "flat");

      GtkWidget *submenu_popover = gtk_popover_new();

      apply_popover_skin(module, GTK_POPOVER(submenu_popover));
      fill_popover(module, GTK_POPOVER(submenu_popover), entry->submenu);
      gtk_menu_button_set_popover(GTK_MENU_BUTTON(submenu_button),
                                  submenu_popover);
      gtk_box_append(GTK_BOX(box), submenu_button);
      continue;
    }

    g_object_set_data(G_OBJECT(button), WHELK_MENU_ENTRY_POINTER, entry);
    g_signal_connect(button, "clicked", G_CALLBACK(on_entry_clicked), module);
    gtk_box_append(GTK_BOX(box), button);
  }

  gtk_popover_set_child(popover, box);
}

static void render(WhelkModule *module) {
  MenuData *data = module->data;

  clear_row(data);

  for (guint i = 0; i < data->menus->len; i++) {
    WhelkMenu *menu = g_ptr_array_index(data->menus, i);
    GtkWidget *button = gtk_menu_button_new();
    GtkWidget *popover = gtk_popover_new();

    /* A menubar's labels open a menu; the little arrow of a combo box is not
     * part of that vocabulary. */
    /* A child instead of a label: a menu button grows the little down arrow
     * only when it has neither, and a menubar's labels do not point at
     * anything. */
    gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(button), FALSE);
    gtk_menu_button_set_child(GTK_MENU_BUTTON(button),
                              gtk_label_new(menu->label));
    gtk_widget_add_css_class(button, "flat");
    gtk_widget_add_css_class(button, "whelk-menu-button");

    apply_popover_skin(module, GTK_POPOVER(popover));
    fill_popover(module, GTK_POPOVER(popover), menu);
    gtk_menu_button_set_popover(GTK_MENU_BUTTON(button), popover);

    gtk_box_append(GTK_BOX(data->row), button);
  }

  gtk_widget_set_visible(data->row, data->menus->len > 0);
}

/* --- what the application exports ----------------------------------------- */

static void replace_menus(WhelkModule *module, GPtrArray *menus) {
  MenuData *data = module->data;

  g_clear_pointer(&data->menus, g_ptr_array_unref);
  data->menus = menus;
  render(module);
}

static void probe_next(MenuProbe *probe);

static void probe_free(MenuProbe *probe) {
  g_clear_object(&probe->bus);
  g_clear_pointer(&probe->paths, g_ptr_array_unref);
  g_free(probe);
}

static void on_start_reply(GObject *source, GAsyncResult *result,
                           gpointer user_data) {
  MenuProbe *probe = user_data;
  MenuData *data = probe->module->data;
  gboolean adopted = FALSE;

  g_autoptr(GError) error = NULL;
  GVariant *reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  if (reply) {
    if (probe->generation == data->generation) {
      GVariant *parameters = g_variant_get_child_value(reply, 0);
      GPtrArray *menus = whelk_menubar_from_gtk(parameters, 0, 0);

      if (menus->len > 0) {
        replace_menus(probe->module, menus);
        adopted = TRUE;
      } else {
        g_ptr_array_unref(menus);
      }

      g_variant_unref(parameters);
    }

    g_variant_unref(reply);
  }

  if (adopted || probe->generation != data->generation)
    probe_free(probe);
  else
    probe_next(probe);
}

static void probe_next(MenuProbe *probe) {
  if (probe->index >= probe->paths->len) {
    probe_free(probe);
    return;
  }

  const char *path = g_ptr_array_index(probe->paths, probe->index++);
  guint32 zero = 0;
  GVariant *groups = g_variant_new_fixed_array(G_VARIANT_TYPE_UINT32, &zero, 1,
                                               sizeof(guint32));

  MenuData *data = probe->module->data;

  g_dbus_connection_call(probe->bus, data->bus_name, path, "org.gtk.Menus",
                         "Start", g_variant_new("(@au)", groups),
                         G_VARIANT_TYPE("(a(uuaa{sv}))"),
                         G_DBUS_CALL_FLAGS_NONE, 1500, NULL, on_start_reply,
                         probe);
}

static void probe_menus(WhelkModule *module) {
  MenuData *data = module->data;

  g_autoptr(GError) error = NULL;
  GDBusConnection *bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);

  if (!bus || !data->bus_name || *data->bus_name == '\0')
    return;

  /* An application id is not a bus name: sway reports alacritty's as
   * "Alacritty", which has no dot and is not a valid D-Bus name, so asking the
   * bus for it is a programming error that GLib answers with a critical.  Such
   * an application gets the synthesized menu, and the bus is not troubled. */
  if (!g_dbus_is_name(data->bus_name))
    return;

  MenuProbe *probe = g_new0(MenuProbe, 1);

  probe->module = module;
  probe->generation = data->generation;
  probe->bus = g_object_ref(bus);
  probe->paths = g_ptr_array_new_with_free_func(g_free);
  g_ptr_array_add(probe->paths,
                  g_strdup_printf("%s/menus/menubar", data->app_path));
  g_ptr_array_add(probe->paths,
                  g_strdup_printf("%s/menus/AppMenu", data->app_path));

  probe_next(probe);
}

/* --- the application in front --------------------------------------------- */

static void build_for_app(WhelkModule *module, const char *app_id) {
  MenuData *data = module->data;

  data->generation++;

  g_free(data->app_id);
  data->app_id = g_strdup(app_id ? app_id : "");

  g_clear_object(&data->desktop);
  data->desktop = whelk_desktop_for_app_id(data->app_id);

  g_free(data->bus_name);
  data->bus_name = g_strdup(data->app_id);

  g_free(data->app_path);

  if (*data->app_id) {
    g_autofree char *dots = g_strdup(data->app_id);

    for (char *cursor = dots; *cursor; cursor++)
      if (*cursor == '.')
        *cursor = '/';

    data->app_path = g_strdup_printf("/%s", dots);
  } else {
    data->app_path = g_strdup("/org/gtk/Application/anonymous");
  }

  const char *const *action_ids = NULL;
  const char *const *action_labels = NULL;

  g_auto(GStrv) ids = NULL;
  g_auto(GStrv) labels = NULL;

  if (data->desktop) {
    ids = (GStrv)g_desktop_app_info_list_actions(data->desktop);

    if (ids) {
      const guint count = g_strv_length(ids);

      labels = g_new0(char *, count + 1);

      for (guint i = 0; i < count; i++) {
        char *name = g_desktop_app_info_get_action_name(data->desktop, ids[i]);

        labels[i] = g_strdup(name ? name : ids[i]);
        g_free(name);
      }

      action_ids = (const char *const *)ids;
      action_labels = (const char *const *)labels;
    }
  }

  const guint action_count = action_ids ? g_strv_length((char **)action_ids) : 0;

  const WhelkSession session = whelk_session_detect();
  g_autofree char *close_command = whelk_session_close_window_command(session);

  replace_menus(module,
                whelk_menubar_synthesize(action_ids, action_labels,
                                         action_count, close_command,
                                         data->show_shortcuts
                                             ? data->shortcut_tool
                                             : NULL));

  probe_menus(module);
}

static void on_focused_window(WhelkModule *module, const char *text,
                              gpointer user_data) {
  (void)user_data;

  MenuData *data = module->data;
  char *title = NULL;
  char *app_id = NULL;

  if (!whelk_session_parse_focused(whelk_session_detect(), text, &title,
                                   &app_id)) {
    build_for_app(module, "");
    return;
  }

  if (g_strcmp0(app_id, data->app_id) != 0)
    build_for_app(module, app_id);

  g_free(title);
  g_free(app_id);
}

static void menu_update(WhelkModule *module) {
  g_autofree char *command =
      whelk_session_focused_window_command(whelk_session_detect());

  if (!command) {
    build_for_app(module, "");
    return;
  }

  whelk_module_command(module, command, on_focused_window, NULL);
}

/* --- lifecycle ------------------------------------------------------------ */

static void menu_destroy(WhelkModule *module) {
  MenuData *data = module->data;

  if (!data)
    return;

  g_clear_pointer(&data->menus, g_ptr_array_unref);
  g_clear_object(&data->desktop);
  g_free(data->app_id);
  g_free(data->bus_name);
  g_free(data->app_path);
  g_free(data->shortcut_tool);
  g_free(data);
}

WhelkModule *whelk_menu_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("menu", spec, bar);

  MenuData *data = g_new0(MenuData, 1);

  data->menus = whelk_menubar_new();
  data->show_shortcuts = whelk_spec_bool(spec, "shortcuts", TRUE);

  /* wtype types on Wayland, xdotool on X11; #:shortcut-tool overrides the
   * choice, and a session with neither simply gets no shortcut entries. */
  const char *requested = whelk_spec_str(spec, "shortcut-tool", NULL);
  g_autofree char *tool =
      requested ? g_find_program_in_path(requested)
                : whelk_session_shortcut_tool(whelk_session_detect());

  data->shortcut_tool = tool ? g_strdup(tool) : NULL;

  data->row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);

  gtk_widget_add_css_class(data->row, "whelk-menu");
  gtk_box_append(GTK_BOX(module->root), data->row);

  module->data = data;
  module->update = menu_update;
  module->destroy = menu_destroy;

  whelk_module_install(module, 1);

  return module;
}

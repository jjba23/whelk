// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <stdlib.h>

#include "logger.h"
#include "shell.h"
#include "util/command.h"

/* =========================================================================
 * BLUETOOTH
 * =========================================================================
 * The adapter's state and how many devices are connected, read with
 * bluetoothctl; the panel (src/network_panel.c) is where devices are connected
 * and disconnected.
 *
 * Like the Wi-Fi module, its presence is what tells the tray that a third-party
 * Bluetooth applet has nothing left to say.
 */

static const char bluetooth_command[] =
    "printf 'powered:%s\\n' \"$(bluetoothctl show 2>/dev/null | "
    "sed -n 's/^[[:space:]]*Powered: //p')\"; "
    "printf 'connected:%s\\n' \"$(bluetoothctl devices Connected 2>/dev/null | "
    "wc -l)\"";

static void on_bluetooth(WhelkModule *module, const char *text, gpointer data) {
  (void)data;

  g_auto(GStrv) lines = g_strsplit(text ? text : "", "\n", -1);
  gboolean powered = FALSE;
  int connected = 0;

  for (guint i = 0; lines[i]; i++) {
    if (g_str_has_prefix(lines[i], "powered:"))
      powered = g_strcmp0(lines[i] + 8, "yes") == 0;
    else if (g_str_has_prefix(lines[i], "connected:"))
      connected = atoi(lines[i] + 10);
  }

  whelk_module_remove_class(module, "off");
  whelk_module_remove_class(module, "connected");

  GHashTable *values = whelk_values_new();

  whelk_values_set_int(values, "count", connected);
  whelk_values_set(values, "state", powered ? "on" : "off");

  if (!powered) {
    whelk_module_add_class(module, "off");
    whelk_module_present_format(
        module, whelk_spec_str(module->spec, "disabled-format", "off"), "",
        whelk_spec_str(module->spec, "tooltip", "Bluetooth is off"), values);
  } else {
    if (connected > 0)
      whelk_module_add_class(module, "connected");

    whelk_module_present_format(
        module, whelk_spec_str(module->spec, "format", "{icon} {count}"),
        whelk_spec_str(module->spec, "icon", ""),
        whelk_spec_str(module->spec, "tooltip",
                       "{count} device(s) connected"),
        values);
  }
}

static void bluetooth_update(WhelkModule *module) {
  whelk_module_command(module, bluetooth_command, on_bluetooth, NULL);
}

static void on_click(GtkGestureClick *gesture, int n_press, double x, double y,
                     gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;
  (void)user_data;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_show_network_panel(shell, "bluetooth");
}

WhelkModule *whelk_bluetooth_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("bluetooth", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-bluetooth");
  whelk_module_add_label(module);

  if (!whelk_spec_has(spec, "on-click")) {
    GtkGesture *gesture = gtk_gesture_click_new();

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                  GDK_BUTTON_PRIMARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(on_click), NULL);
    gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(gesture));
    gtk_widget_set_cursor_from_name(module->root, "pointer");
  }

  module->update = bluetooth_update;
  whelk_module_install(module, 10);

  return module;
}

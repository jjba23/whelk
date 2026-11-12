// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <stdlib.h>
#include <string.h>

#include "logger.h"
#include "shell.h"
#include "util/command.h"

/* =========================================================================
 * WI-FI
 * =========================================================================
 * The bar's reading of the wireless link, and the way into the network panel.
 *
 * The reading is nmcli's: the radio's state and the access point that is in
 * use.  The panel (src/network_panel.c) is where a network is actually chosen,
 * because connecting needs a list and, for a new network, a passphrase -- and a
 * bar row has room for neither.
 *
 * With this module on the bar, a NetworkManager applet in the tray is a second
 * copy of the same control, so the tray hides it (see modules/tray.c).
 */

static const char wifi_command[] =
    "printf 'radio:%s\\n' \"$(nmcli radio wifi 2>/dev/null)\"; "
    "nmcli -t -f ACTIVE,SSID,SIGNAL dev wifi 2>/dev/null";

/* nmcli escapes ':' inside an SSID; a split that ignores the escape cuts
 * "Cafe:Guest" in half. */
static char *terse_field(const char **cursor) {
  GString *field = g_string_new(NULL);

  while (**cursor && **cursor != ':') {
    if (**cursor == '\\' && (*cursor)[1] != '\0') {
      g_string_append_c(field, (*cursor)[1]);
      *cursor += 2;
      continue;
    }

    g_string_append_c(field, **cursor);
    (*cursor)++;
  }

  if (**cursor == ':')
    (*cursor)++;

  return g_string_free(field, FALSE);
}

static void on_wifi(WhelkModule *module, const char *text, gpointer data) {
  (void)data;

  g_auto(GStrv) lines = g_strsplit(text ? text : "", "\n", -1);
  gboolean enabled = FALSE;
  char *ssid = NULL;
  int signal = 0;

  for (guint i = 0; lines[i]; i++) {
    if (g_str_has_prefix(lines[i], "radio:")) {
      enabled = g_strcmp0(lines[i] + 6, "enabled") == 0;
      continue;
    }

    if (*lines[i] == '\0')
      continue;

    const char *cursor = lines[i];
    g_autofree char *active = terse_field(&cursor);

    if (g_strcmp0(active, "yes") != 0)
      continue;

    g_autofree char *name = terse_field(&cursor);
    g_autofree char *strength = terse_field(&cursor);

    g_free(ssid);
    ssid = g_strdup(name);
    signal = atoi(strength);
  }

  whelk_module_remove_class(module, "off");
  whelk_module_remove_class(module, "connected");

  GHashTable *values = whelk_values_new();

  whelk_values_set(values, "ssid", ssid ? ssid : "");
  whelk_values_set_int(values, "signal", signal);
  whelk_values_set(values, "state", !enabled ? "off" : (ssid ? "connected" : "disconnected"));

  if (!enabled) {
    whelk_module_add_class(module, "off");
    whelk_module_present_format(
        module, whelk_spec_str(module->spec, "disabled-format", "off"), "",
        whelk_spec_str(module->spec, "tooltip", "Wi-Fi is off"), values);
  } else if (!ssid) {
    whelk_module_present_format(
        module, whelk_spec_str(module->spec, "disconnected-format", "no network"),
        "", whelk_spec_str(module->spec, "tooltip", "Not connected"), values);
  } else {
    whelk_module_add_class(module, "connected");
    whelk_module_present_format(
        module, whelk_spec_str(module->spec, "format", "{icon} {ssid}"),
        whelk_spec_str(module->spec, "icon", ""),
        whelk_spec_str(module->spec, "tooltip", "{ssid} · {signal}%"), values);
  }

  g_free(ssid);
}

static void wifi_update(WhelkModule *module) {
  whelk_module_command(module, wifi_command, on_wifi, NULL);
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
    whelk_shell_show_network_panel(shell, "wifi");
}

WhelkModule *whelk_wifi_module_new(const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("wifi", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-wifi");
  whelk_module_add_label(module);

  /* The module opens the panel on a left click, unless the configuration has
   * claimed that button for a command of its own. */
  if (!whelk_spec_has(spec, "on-click")) {
    GtkGesture *gesture = gtk_gesture_click_new();

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                  GDK_BUTTON_PRIMARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(on_click), NULL);
    gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(gesture));
    gtk_widget_set_cursor_from_name(module->root, "pointer");
  }

  module->update = wifi_update;
  whelk_module_install(module, 5);

  return module;
}

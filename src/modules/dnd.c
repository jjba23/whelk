// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "logger.h"
#include "notifications.h"
#include "shell.h"

/* =========================================================================
 * DO NOT DISTURB
 * =========================================================================
 * The notification service owns the state; this module is the one always-visible
 * answer to "are notifications being held back?"  By default the tile exists
 * only while the answer is yes, the way a menu bar shows the do-not-disturb
 * glyph only when it matters; #:show-off? keeps it there all the time.
 *
 * It listens to the service's `dnd-changed' rather than polling a file or a
 * daemon's mode string: the state whelk sets is the state the shell knows.
 */

typedef struct {
  WhelkNotifications *notifications;
  gulong dnd_id;
} DndState;

static void paint(WhelkModule *module) {
  DndState *state = module->data;

  if (!state || !state->notifications) {
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  const gboolean dnd = whelk_notifications_has_dnd(state->notifications);
  const gboolean show_off = whelk_spec_bool(module->spec, "show-off?", FALSE);

  if (!dnd && !show_off) {
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  whelk_module_remove_class(module, "on");
  whelk_module_remove_class(module, "off");
  whelk_module_remove_class(module, "whelk-accent");
  whelk_module_add_class(module, dnd ? "on" : "off");

  if (dnd)
    whelk_module_add_class(module, "whelk-accent");

  GHashTable *values = whelk_values_new();

  whelk_values_set(values, "state", dnd ? "on" : "off");

  const char *icon = whelk_spec_str(module->spec, dnd ? "dnd-icon" : "icon",
                                    dnd ? "\uf1f6" : "\uf0f3");
  const char *format = whelk_spec_str(module->spec, "format", "{icon}");
  const char *tooltip = whelk_spec_str(
      module->spec, "tooltip",
      dnd ? "Do not disturb is on — click to let notifications through"
          : "Do not disturb is off — click to silence notifications");

  if (dnd) {
    format = whelk_spec_str(module->spec, "dnd-format", format);
    tooltip = whelk_spec_str(module->spec, "dnd-tooltip", tooltip);
  }

  whelk_module_present_format(module, format, icon, tooltip, values);
}

static void on_dnd_changed(WhelkNotifications *notifications,
                           gpointer user_data) {
  (void)notifications;

  paint(user_data);
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
    whelk_shell_toggle_dnd(shell);
}

static void dnd_destroy(WhelkModule *module) {
  DndState *state = module->data;

  if (!state)
    return;

  if (state->notifications) {
    if (state->dnd_id != 0)
      g_signal_handler_disconnect(state->notifications, state->dnd_id);

    g_object_unref(state->notifications);
  }

  g_free(state);
  module->data = NULL;
}

WhelkModule *whelk_dnd_module_new(const WhelkModuleSpec *spec,
                                  const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("dnd", spec, bar);
  DndState *state = g_new0(DndState, 1);
  WhelkShell *shell = whelk_shell_get_default();

  gtk_widget_add_css_class(module->root, "whelk-dnd");
  whelk_module_add_label(module);

  if (shell && whelk_shell_notifications(shell)) {
    state->notifications = g_object_ref(whelk_shell_notifications(shell));
    state->dnd_id = g_signal_connect(
        state->notifications, "dnd-changed", G_CALLBACK(on_dnd_changed), module);
  } else {
    LOG_C_DEBUG("Dnd", "no notification service: the tile starts hidden");
  }

  module->data = state;
  module->destroy = dnd_destroy;

  if (!whelk_spec_has(spec, "on-click")) {
    GtkGesture *gesture = gtk_gesture_click_new();

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                  GDK_BUTTON_PRIMARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(on_click), NULL);
    gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(gesture));
    gtk_widget_set_cursor_from_name(module->root, "pointer");
  }

  module->update = NULL;
  whelk_module_install(module, 0);
  paint(module);

  return module;
}

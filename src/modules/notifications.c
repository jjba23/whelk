// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "logger.h"
#include "notifications.h"
#include "shell.h"

/* =========================================================================
 * NOTIFICATIONS
 * =========================================================================
 * The bell: how many notifications are waiting, and the way into the centre.
 * It listens to the service rather than polling it, so a notification is
 * counted the moment it arrives.
 *
 * Left click opens the centre; right click toggles do-not-disturb.  Both are
 * defaults, so a configuration that wants something else writes on-click or
 * on-click-right and keeps the shell out of the way.
 */

typedef struct {
  WhelkNotifications *notifications;
  gulong changed_id;
  gulong dnd_id;
} NotificationsState;

static void paint(WhelkModule *module, NotificationsState *state) {
  const guint count = whelk_notifications_count(state->notifications);
  const gboolean dnd = whelk_notifications_has_dnd(state->notifications);

  whelk_module_remove_class(module, "empty");
  whelk_module_remove_class(module, "dnd");

  if (dnd)
    whelk_module_add_class(module, "dnd");
  else if (count == 0)
    whelk_module_add_class(module, "empty");

  GHashTable *values = whelk_values_new();

  whelk_values_set_int(values, "count", (int)count);
  whelk_values_set(values, "state", dnd ? "dnd" : (count > 0 ? "new" : "idle"));

  /* Do not disturb wears its own bell, the way the reference bar's do-not-
   * disturb item did: the one reading on the bar that must not look like the
   * others, because it says notifications will not arrive. */
  const char *icon = whelk_spec_str(module->spec, "icon", "");
  const char *format = whelk_spec_str(module->spec, "format", "{icon}");
  const char *tooltip = whelk_spec_str(
      module->spec, "tooltip",
      dnd ? "Do not disturb is on — right-click to let notifications through"
          : "{count} notification(s) — click for the centre · right-click for "
            "do not disturb");

  if (dnd) {
    icon = whelk_spec_str(module->spec, "dnd-icon", "\uf1f6");
    format = whelk_spec_str(module->spec, "dnd-format", format);
    tooltip = whelk_spec_str(
        module->spec, "dnd-tooltip",
        "Do not disturb is on — right-click to let notifications through");
  }

  whelk_module_present_format(module, format, icon, tooltip, values);
}

static void on_service_changed(WhelkNotifications *notifications,
                               gpointer user_data) {
  (void)notifications;

  WhelkModule *module = user_data;

  paint(module, module->data);
}

static void notifications_destroy(WhelkModule *module) {
  NotificationsState *state = module->data;

  if (!state)
    return;

  if (state->notifications) {
    if (state->changed_id != 0)
      g_signal_handler_disconnect(state->notifications, state->changed_id);
    if (state->dnd_id != 0)
      g_signal_handler_disconnect(state->notifications, state->dnd_id);

    g_object_unref(state->notifications);
  }

  g_free(state);
  module->data = NULL;
}

static void on_left_click(GtkGestureClick *gesture, int n_press, double x,
                          double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;
  (void)user_data;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_toggle_notifications(shell);
}

static void on_right_click(GtkGestureClick *gesture, int n_press, double x,
                           double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;
  (void)user_data;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_toggle_dnd(shell);
}

WhelkModule *whelk_notifications_module_new(const WhelkModuleSpec *spec,
                                            const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("notifications", spec, bar);
  NotificationsState *state = g_new0(NotificationsState, 1);
  WhelkShell *shell = whelk_shell_get_default();

  gtk_widget_add_css_class(module->root, "whelk-notifications");
  whelk_module_add_label(module);

  if (shell && whelk_shell_notifications(shell)) {
    state->notifications = g_object_ref(whelk_shell_notifications(shell));
    state->changed_id = g_signal_connect(
        state->notifications, "changed", G_CALLBACK(on_service_changed), module);
    state->dnd_id = g_signal_connect(
        state->notifications, "dnd-changed", G_CALLBACK(on_service_changed),
        module);
  } else {
    LOG_C_DEBUG("Notifications",
                "no notification service yet: the bell starts empty");
  }

  module->data = state;
  module->destroy = notifications_destroy;

  if (!whelk_spec_has(spec, "on-click")) {
    GtkGesture *gesture = gtk_gesture_click_new();

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                  GDK_BUTTON_PRIMARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(on_left_click), NULL);
    gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(gesture));
    gtk_widget_set_cursor_from_name(module->root, "pointer");
  }

  if (!whelk_spec_has(spec, "on-click-right")) {
    GtkGesture *gesture = gtk_gesture_click_new();

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),
                                  GDK_BUTTON_SECONDARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(on_right_click), NULL);
    gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(gesture));
  }

  module->update = NULL;
  whelk_module_install(module, 0);

  if (state->notifications)
    paint(module, state);

  return module;
}

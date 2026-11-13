// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "clipboard.h"
#include "logger.h"
#include "shell.h"

/* =========================================================================
 * CLIPBOARD
 * =========================================================================
 * The bar's way into the clipboard picker, and its depth: the history is the
 * shell's own, so the count is a signal from the store rather than a wl-paste
 * or clipman poll.  A click opens the same picker `whelk --clipboard' opens;
 * #:on-click replaces that, as it does on every module.
 */

typedef struct {
  WhelkClipboard *clipboard;
  gulong changed_id;
} ClipboardState;

static void paint(WhelkModule *module) {
  ClipboardState *state = module->data;

  if (!state || !state->clipboard) {
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  const guint count = whelk_clipboard_entries(state->clipboard)->len;

  gtk_widget_set_visible(module->root, TRUE);

  whelk_module_remove_class(module, "empty");

  if (count == 0)
    whelk_module_add_class(module, "empty");

  GHashTable *values = whelk_values_new();

  whelk_values_set_int(values, "count", (int)count);

  const char *icon = whelk_spec_str(module->spec, "icon", "\uf328");
  const char *format = whelk_spec_str(module->spec, "format", "{icon} {count}");
  const char *tooltip = whelk_spec_str(
      module->spec, "tooltip",
      "Clipboard: {count} item(s) — click for the picker");

  whelk_module_present_format(module, format, icon, tooltip, values);
}

static void on_clipboard_changed(WhelkClipboard *clipboard,
                                 gpointer user_data) {
  (void)clipboard;

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
    whelk_shell_show_launcher(shell, "clipboard");
}

static void clipboard_destroy(WhelkModule *module) {
  ClipboardState *state = module->data;

  if (!state)
    return;

  if (state->clipboard) {
    if (state->changed_id != 0)
      g_signal_handler_disconnect(state->clipboard, state->changed_id);

    g_object_unref(state->clipboard);
  }

  g_free(state);
  module->data = NULL;
}

WhelkModule *whelk_clipboard_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("clipboard", spec, bar);
  ClipboardState *state = g_new0(ClipboardState, 1);
  WhelkShell *shell = whelk_shell_get_default();

  gtk_widget_add_css_class(module->root, "whelk-clipboard");
  whelk_module_add_label(module);

  if (shell && whelk_shell_clipboard(shell)) {
    state->clipboard = g_object_ref(whelk_shell_clipboard(shell));
    state->changed_id = g_signal_connect(
        state->clipboard, "changed", G_CALLBACK(on_clipboard_changed), module);
  } else {
    LOG_C_DEBUG("Clipboard", "no clipboard service: the reading starts empty");
  }

  module->data = state;
  module->destroy = clipboard_destroy;

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

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
  WhelkModule *module;
  GtkWidget *menu;
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
  whelk_module_remove_class(module, "paused");
  whelk_module_remove_class(module, "unavailable");

  if (count == 0)
    whelk_module_add_class(module, "empty");

  if (whelk_clipboard_is_paused(state->clipboard))
    whelk_module_add_class(module, "paused");
  else if (!whelk_clipboard_is_available(state->clipboard))
    whelk_module_add_class(module, "unavailable");

  GHashTable *values = whelk_values_new();

  whelk_values_set_int(values, "count", (int)count);

  const char *icon = whelk_spec_str(module->spec, "icon", "\uf328");
  const char *format = whelk_spec_str(module->spec, "format", "{icon} {count}");
  const char *base_tooltip = whelk_spec_str(
      module->spec, "tooltip",
      "Clipboard: {count} item(s) — click for the picker\n"
      "Right-click: history controls");
  g_autofree char *tooltip =
      g_strdup_printf("%s\n%s", base_tooltip,
                      whelk_clipboard_status(state->clipboard));

  whelk_module_present_format(module, format, icon, tooltip, values);
}

static void on_clipboard_changed(WhelkClipboard *clipboard,
                                 gpointer user_data) {
  (void)clipboard;

  paint(user_data);
}

static void clear_menu(GtkWidget *box) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(box)) != NULL)
    gtk_box_remove(GTK_BOX(box), child);
}

static GtkWidget *menu_button(const char *label, gboolean enabled) {
  GtkWidget *button = gtk_button_new_with_label(label);

  gtk_widget_add_css_class(button, "flat");
  gtk_widget_add_css_class(button, "whelk-menu-item");
  gtk_widget_set_halign(button, GTK_ALIGN_FILL);
  gtk_widget_set_sensitive(button, enabled);

  return button;
}

static void on_pause_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  ClipboardState *state = user_data;

  whelk_clipboard_set_paused(state->clipboard,
                             !whelk_clipboard_is_paused(state->clipboard));
}

static void on_clear_unpinned_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  ClipboardState *state = user_data;

  whelk_clipboard_clear_unpinned(state->clipboard);
}

static void on_clear_all_chosen(GObject *source, GAsyncResult *result,
                                gpointer user_data) {
  ClipboardState *state = user_data;
  g_autoptr(GError) error = NULL;
  const int choice =
      gtk_alert_dialog_choose_finish(GTK_ALERT_DIALOG(source), result, &error);

  if (choice == 1)
    whelk_clipboard_clear(state->clipboard);

  g_object_unref(source);
}

static void on_clear_all_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  ClipboardState *state = user_data;
  GtkAlertDialog *dialog =
      gtk_alert_dialog_new("Clear the whole clipboard history?");
  const char *buttons[] = {"Cancel", "Clear all", NULL};

  gtk_alert_dialog_set_detail(
      dialog, "Pinned copies are deleted too. This cannot be undone.");
  gtk_alert_dialog_set_buttons(dialog, buttons);
  gtk_alert_dialog_set_cancel_button(dialog, 0);
  gtk_alert_dialog_set_default_button(dialog, 0);
  gtk_alert_dialog_choose(dialog, NULL, NULL, on_clear_all_chosen, state);
}

static void on_restart_clicked(GtkButton *button, gpointer user_data) {
  (void)button;

  ClipboardState *state = user_data;

  whelk_clipboard_restart(state->clipboard);
}

static void on_open_clicked(GtkButton *button, gpointer user_data) {
  (void)button;
  (void)user_data;

  WhelkShell *shell = whelk_shell_get_default();

  if (shell)
    whelk_shell_show_launcher(shell, "clipboard");
}

static void fill_context_menu(ClipboardState *state) {
  GtkWidget *box = gtk_popover_get_child(GTK_POPOVER(state->menu));

  clear_menu(box);

  const gboolean paused = whelk_clipboard_is_paused(state->clipboard);
  GtkWidget *pause =
      menu_button(paused ? "Resume history" : "Pause history", TRUE);

  g_signal_connect(pause, "clicked", G_CALLBACK(on_pause_clicked), state);
  gtk_box_append(GTK_BOX(box), pause);

  GtkWidget *unpinned = menu_button("Clear unpinned", TRUE);

  g_signal_connect(unpinned, "clicked", G_CALLBACK(on_clear_unpinned_clicked),
                   state);
  gtk_box_append(GTK_BOX(box), unpinned);

  GtkWidget *all = menu_button("Clear all…", TRUE);

  g_signal_connect(all, "clicked", G_CALLBACK(on_clear_all_clicked), state);
  gtk_box_append(GTK_BOX(box), all);
  gtk_box_append(GTK_BOX(box),
                 gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

  GtkWidget *restart = menu_button("Restart listener", !paused);

  g_signal_connect(restart, "clicked", G_CALLBACK(on_restart_clicked), state);
  gtk_box_append(GTK_BOX(box), restart);

  GtkWidget *open = menu_button("Open picker", TRUE);

  g_signal_connect(open, "clicked", G_CALLBACK(on_open_clicked), NULL);
  gtk_box_append(GTK_BOX(box), open);
}

static void on_context_click(GtkGestureClick *gesture, int n_press, double x,
                             double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  ClipboardState *state = user_data;

  if (!state->menu) {
    state->menu = gtk_popover_new();
    gtk_popover_set_child(GTK_POPOVER(state->menu),
                          gtk_box_new(GTK_ORIENTATION_VERTICAL, 2));
    gtk_widget_set_parent(state->menu, state->module->root);
  }

  fill_context_menu(state);
  gtk_popover_popup(GTK_POPOVER(state->menu));
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

  if (state->menu) {
    gtk_widget_unparent(state->menu);
    state->menu = NULL;
  }

  g_free(state);
  module->data = NULL;
}

WhelkModule *whelk_clipboard_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("clipboard", spec, bar);
  ClipboardState *state = g_new0(ClipboardState, 1);
  WhelkShell *shell = whelk_shell_get_default();

  state->module = module;

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

  GtkGesture *context = gtk_gesture_click_new();

  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(context),
                                GDK_BUTTON_SECONDARY);
  g_signal_connect(context, "pressed", G_CALLBACK(on_context_click), state);
  gtk_widget_add_controller(module->root, GTK_EVENT_CONTROLLER(context));

  module->update = NULL;
  whelk_module_install(module, 0);
  paint(module);

  return module;
}

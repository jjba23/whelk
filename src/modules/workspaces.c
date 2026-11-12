// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "logger.h"
#include "util/command.h"
#include "util/format.h"
#include "util/json.h"

/* =========================================================================
 * WORKSPACES
 * =========================================================================
 * niri's workspaces, as buttons.  This is the one module that is a row of
 * widgets rather than one reading, so it owns its children: every refresh
 * clears the box and repaints it, which is both the simplest correct answer and
 * cheap at the size of a workspace set.
 *
 * `niri msg action focus-workspace <idx>' is the click: niri addresses its
 * workspaces by index, and the index is what the configuration's format prints.
 */

typedef struct {
  char *command;
  int index;
} WorkspaceAction;

static void workspace_action_free(gpointer data, GClosure *closure) {
  WorkspaceAction *action = data;

  (void)closure;

  if (!action)
    return;

  g_free(action->command);
  g_free(action);
}

static void on_workspace_clicked(GtkGestureClick *gesture, int n_press,
                                 double x, double y, gpointer user_data) {
  (void)gesture;
  (void)n_press;
  (void)x;
  (void)y;

  WorkspaceAction *action = user_data;

  whelk_command_spawn(action->command);
}

static void clear_children(GtkWidget *box) {
  GtkWidget *child;

  while ((child = gtk_widget_get_first_child(box)) != NULL)
    gtk_widget_unparent(child);
}

static void add_workspace_button(WhelkModule *module, const WhelkModuleSpec *spec,
                                 int index, const char *name,
                                 gboolean active, gboolean empty) {
  const char *format = whelk_spec_str(spec, "format", "{index}");
  g_autofree char *index_text = g_strdup_printf("%d", index);
  g_autofree char *label_text = whelk_str_subst(format, "index", index_text);
  g_autofree char *with_name = whelk_str_subst(label_text, "name", name);
  g_autofree char *escaped = whelk_markup_escape(with_name);

  GtkWidget *label = gtk_label_new(NULL);

  gtk_label_set_markup(GTK_LABEL(label), escaped);
  gtk_widget_add_css_class(label, "whelk-workspace");
  gtk_widget_add_css_class(label, active ? "active" : "inactive");

  if (empty)
    gtk_widget_add_css_class(label, "empty");

  /* niri's CLI takes the index, not the workspace id. */
  WorkspaceAction *action = g_new0(WorkspaceAction, 1);
  action->command = g_strdup_printf("niri msg action focus-workspace %d", index);
  action->index = index;

  GtkGesture *gesture = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), GDK_BUTTON_PRIMARY);
  g_signal_connect_data(gesture, "pressed", G_CALLBACK(on_workspace_clicked),
                        action, workspace_action_free, 0);
  gtk_widget_add_controller(label, GTK_EVENT_CONTROLLER(gesture));
  gtk_widget_set_cursor_from_name(label, "pointer");

  gtk_box_append(GTK_BOX(module->root), label);
}

static void on_workspaces(WhelkModule *module, const char *text, gpointer data) {
  (void)data;

  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root) {
    LOG_C_WARN("Workspaces", "niri's answer was not JSON: %s", error->message);
    return;
  }

  const gboolean hide_empty =
      whelk_spec_bool(module->spec, "hide-empty?", FALSE);

  clear_children(module->root);

  for (guint i = 0; i < whelk_json_array_length(root); i++) {
    WhelkJson *workspace = whelk_json_array_get(root, i);
    const double index = whelk_json_object_number(workspace, "idx", -1.0);
    const char *name = whelk_json_object_string(workspace, "name");
    const gboolean active =
        whelk_json_object_bool(workspace, "is_active", FALSE) ||
        whelk_json_object_bool(workspace, "is_focused", FALSE);

    WhelkJson *window = whelk_json_object_get(workspace, "active_window_id");
    const gboolean empty =
        window == NULL || whelk_json_type(window) == WHELK_JSON_NULL;

    if (index < 0)
      continue;
    if (hide_empty && empty)
      continue;

    add_workspace_button(module, module->spec, (int)index,
                         name ? name : "", active, empty);
  }

  whelk_json_free(root);
}

static void workspaces_update(WhelkModule *module) {
  whelk_module_command(module, "niri msg -j workspaces", on_workspaces, NULL);
}

WhelkModule *whelk_workspaces_module_new(const WhelkModuleSpec *spec,
                                         const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("workspaces", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-workspaces");
  module->update = workspaces_update;
  whelk_module_install(module, 1);

  return module;
}

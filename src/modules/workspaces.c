// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "logger.h"
#include "util/command.h"
#include "util/format.h"
#include "util/session.h"
#include "util/workspaces.h"

/* =========================================================================
 * WORKSPACES
 * =========================================================================
 * niri's and Hyprland's workspaces, as buttons.  This is the one module that is
 * a row of widgets rather than one reading, so it owns its children: every
 * refresh clears the box and repaints it, which is both the simplest correct
 * answer and cheap at the size of a workspace set.
 *
 * The session layer supplies the command, the parser and the focus command;
 * `niri msg action focus-workspace <idx>' and `hyprctl dispatch workspace <id>'
 * are the two clicks, and the index is what the configuration's format prints.
 */

typedef struct {
  char *command;
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

static void add_workspace_button(WhelkModule *module,
                                 const WhelkWorkspace *workspace,
                                 const char *command) {
  const char *format = whelk_spec_str(module->spec, "format", "{index}");
  g_autofree char *index_text = g_strdup_printf("%d", workspace->index);
  g_autofree char *label_text = whelk_str_subst(format, "index", index_text);
  g_autofree char *with_name =
      whelk_str_subst(label_text, "name", workspace->name);
  g_autofree char *escaped = whelk_markup_escape(with_name);

  GtkWidget *label = gtk_label_new(NULL);

  gtk_label_set_markup(GTK_LABEL(label), escaped);
  gtk_widget_add_css_class(label, "whelk-workspace");
  gtk_widget_add_css_class(label, workspace->active ? "active" : "inactive");

  if (workspace->empty)
    gtk_widget_add_css_class(label, "empty");

  WorkspaceAction *action = g_new0(WorkspaceAction, 1);
  action->command = g_strdup(command ? command : "");

  GtkGesture *gesture = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), GDK_BUTTON_PRIMARY);
  g_signal_connect_data(gesture, "pressed", G_CALLBACK(on_workspace_clicked),
                        action, workspace_action_free, 0);
  gtk_widget_add_controller(label, GTK_EVENT_CONTROLLER(gesture));
  gtk_widget_set_cursor_from_name(label, "pointer");

  gtk_box_append(GTK_BOX(module->root), label);
}

static void on_workspaces(WhelkModule *module, const char *text,
                          gpointer data) {
  (void)data;

  const WhelkSession session = whelk_session_detect();
  GPtrArray *workspaces = whelk_workspaces_parse(session, text);

  if (!workspaces) {
    LOG_C_WARN("Workspaces", "%s's answer was not the expected JSON",
               whelk_session_name(session));
    return;
  }

  const gboolean hide_empty =
      whelk_spec_bool(module->spec, "hide-empty?", FALSE);

  clear_children(module->root);

  for (guint i = 0; i < workspaces->len; i++) {
    const WhelkWorkspace *workspace = g_ptr_array_index(workspaces, i);

    if (hide_empty && workspace->empty)
      continue;

    g_autofree char *command =
        whelk_workspaces_focus_command(session, workspace->index);

    add_workspace_button(module, workspace, command);
  }

  g_ptr_array_unref(workspaces);
}

static void workspaces_update(WhelkModule *module) {
  const WhelkSession session = whelk_session_detect();
  g_autofree char *command = whelk_workspaces_command(session);

  if (!command) {
    clear_children(module->root);
    return;
  }

  whelk_module_command(module, command, on_workspaces, NULL);
}

WhelkModule *whelk_workspaces_module_new(const WhelkModuleSpec *spec,
                                         const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("workspaces", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-workspaces");
  module->update = workspaces_update;
  whelk_module_install(module, 1);

  return module;
}

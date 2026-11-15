// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/workspaces.h"

#include "util/json.h"

#define HYPRLAND_MONITORS_MARKER "WHELK_HYPRLAND_MONITORS"

void whelk_workspace_free(WhelkWorkspace *workspace) {
  if (!workspace)
    return;

  g_free(workspace->name);
  g_free(workspace);
}

static WhelkWorkspace *workspace_new(int index, const char *name, gboolean active,
                                    gboolean empty) {
  WhelkWorkspace *workspace = g_new0(WhelkWorkspace, 1);

  workspace->index = index;
  workspace->name = g_strdup(name ? name : "");
  workspace->active = active;
  workspace->empty = empty;

  return workspace;
}

static GPtrArray *parse_niri(const char *text) {
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root || whelk_json_type(root) != WHELK_JSON_ARRAY) {
    whelk_json_free(root);
    return NULL;
  }

  GPtrArray *out = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_workspace_free);

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

    g_ptr_array_add(out, workspace_new((int)index, name, active, empty));
  }

  whelk_json_free(root);
  return out;
}

static void collect_active_hyprland_workspaces(const char *text,
                                               GArray *active) {
  if (!text || !*text)
    return;

  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root || whelk_json_type(root) != WHELK_JSON_ARRAY) {
    whelk_json_free(root);
    return;
  }

  for (guint i = 0; i < whelk_json_array_length(root); i++) {
    WhelkJson *monitor = whelk_json_array_get(root, i);
    WhelkJson *workspace = whelk_json_object_get(monitor, "activeWorkspace");
    const double id = whelk_json_object_number(workspace, "id", -1.0);

    if (id >= 0)
      g_array_append_val(active, id);
  }

  whelk_json_free(root);
}

static gboolean active_contains(const GArray *active, int index) {
  for (guint i = 0; active && i < active->len; i++)
    if (g_array_index(active, double, i) == (double)index)
      return TRUE;

  return FALSE;
}

static GPtrArray *parse_hyprland(const char *text) {
  const char *marker = strstr(text, "\n" HYPRLAND_MONITORS_MARKER "\n");
  g_autofree char *workspaces_text =
      marker ? g_strndup(text, (gsize)(marker - text)) : g_strdup(text);
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(workspaces_text, &error);

  if (!root || whelk_json_type(root) != WHELK_JSON_ARRAY) {
    whelk_json_free(root);
    return NULL;
  }

  GArray *active = g_array_new(FALSE, FALSE, sizeof(double));

  if (marker)
    collect_active_hyprland_workspaces(
        marker + strlen("\n" HYPRLAND_MONITORS_MARKER "\n"), active);

  GPtrArray *out = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_workspace_free);

  for (guint i = 0; i < whelk_json_array_length(root); i++) {
    WhelkJson *workspace = whelk_json_array_get(root, i);
    const double index = whelk_json_object_number(workspace, "id", -1.0);
    const char *name = whelk_json_object_string(workspace, "name");
    const double windows = whelk_json_object_number(workspace, "windows", 0);

    /* Special workspaces are a scratchpad, not a place on the row. */
    if (index < 1)
      continue;

    g_ptr_array_add(out,
                    workspace_new((int)index, name,
                                  active_contains(active, (int)index) ||
                                      whelk_json_object_bool(workspace, "active",
                                                             FALSE),
                                  windows <= 0));
  }

  g_array_unref(active);
  whelk_json_free(root);
  return out;
}

char *whelk_workspaces_command(WhelkSession session) {
  switch (session) {
  case WHELK_SESSION_NIRI:
    return g_strdup("niri msg -j workspaces");
  case WHELK_SESSION_HYPRLAND:
    return g_strdup("hyprctl -j workspaces; echo; echo " HYPRLAND_MONITORS_MARKER
                    "; echo; hyprctl -j monitors");
  case WHELK_SESSION_SWAY:
  case WHELK_SESSION_RIVER:
  case WHELK_SESSION_X11:
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

GPtrArray *whelk_workspaces_parse(WhelkSession session, const char *text) {
  if (!text || *text == '\0')
    return NULL;

  switch (session) {
  case WHELK_SESSION_NIRI:
    return parse_niri(text);
  case WHELK_SESSION_HYPRLAND:
    return parse_hyprland(text);
  case WHELK_SESSION_SWAY:
  case WHELK_SESSION_RIVER:
  case WHELK_SESSION_X11:
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

char *whelk_workspaces_focus_command(WhelkSession session, int index) {
  if (index < 0)
    return NULL;

  switch (session) {
  case WHELK_SESSION_NIRI:
    return g_strdup_printf("niri msg action focus-workspace %d", index);
  case WHELK_SESSION_HYPRLAND:
    return g_strdup_printf("hyprctl dispatch workspace %d", index);
  case WHELK_SESSION_SWAY:
  case WHELK_SESSION_RIVER:
  case WHELK_SESSION_X11:
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

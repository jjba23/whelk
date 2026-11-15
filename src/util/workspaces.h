// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WORKSPACES_H
#define WHELK_WORKSPACES_H

#include "util/session.h"

#include <glib.h>

/* =========================================================================
 * WORKSPACES
 * =========================================================================
 * niri and Hyprland both publish their workspaces as JSON, but not in the
 * same shape: niri has `idx`, `is_active`/`is_focused` and `active_window_id`;
 * Hyprland has `id`, `windows`, and the active workspace is found on its
 * monitors.  This is the translation both compositors fit through, kept away
 * from GTK so the real answer shapes can be tested without a display.
 *
 * Hyprland's answer is one command that prints its workspaces and monitors
 * separated by a marker line; a workspace is active when a monitor's
 * `activeWorkspace.id` names it.  Special workspaces (`id < 1`) are not part
 * of the ordinary row.
 */

typedef struct {
  int index;      /* niri's idx, or Hyprland's id */
  char *name;     /* the workspace's name, never NULL */
  gboolean active;
  gboolean empty;
} WhelkWorkspace;

void whelk_workspace_free(WhelkWorkspace *workspace);

/* The command whose output `whelk_workspaces_parse' understands; NULL when
 * the session has no workspace IPC. */
char *whelk_workspaces_command(WhelkSession session);

/*
 * whelk_workspaces_parse:
 * @session: where the text came from.
 * @text: the command's output.
 *
 * Returns: (transfer full) (nullable): one WhelkWorkspace per workspace,
 *   in the compositor's order, or NULL when the answer was not the JSON this session
 *   promises.  An empty array is a valid answer with no workspaces.
 */
GPtrArray *whelk_workspaces_parse(WhelkSession session, const char *text);

/* The command that focuses @index on this session, or NULL. */
char *whelk_workspaces_focus_command(WhelkSession session, int index);

#endif // WHELK_WORKSPACES_H

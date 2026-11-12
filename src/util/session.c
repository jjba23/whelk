// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/session.h"

#include "util/json.h"

#include <stdlib.h>
#include <string.h>

WhelkSession whelk_session_detect(void) {
  static gsize detected = 0;
  static WhelkSession session = WHELK_SESSION_WAYLAND;

  if (g_once_init_enter(&detected)) {
    const char *niri = g_getenv("NIRI_SOCKET");
    const char *sway = g_getenv("SWAYSOCK");
    const char *display = g_getenv("DISPLAY");
    const char *wayland = g_getenv("WAYLAND_DISPLAY");
    const char *type = g_getenv("XDG_SESSION_TYPE");

    if (niri && *niri)
      session = WHELK_SESSION_NIRI;
    else if (sway && *sway)
      session = WHELK_SESSION_SWAY;
    else if ((type && g_ascii_strcasecmp(type, "x11") == 0) ||
             ((!wayland || !*wayland) && display && *display))
      session = WHELK_SESSION_X11;

    g_once_init_leave(&detected, 1);
  }

  return session;
}

const char *whelk_session_name(WhelkSession session) {
  switch (session) {
  case WHELK_SESSION_NIRI:
    return "niri";
  case WHELK_SESSION_SWAY:
    return "sway";
  case WHELK_SESSION_X11:
    return "x11";
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return "wayland";
}

char *whelk_session_focused_window_command(WhelkSession session) {
  switch (session) {
  case WHELK_SESSION_NIRI:
    return g_strdup("niri msg -j focused-window");
  case WHELK_SESSION_SWAY:
    return g_strdup("swaymsg -t get_tree");
  case WHELK_SESSION_X11:
    /* EWMH: the active window's id, then that window's name and class. */
    return g_strdup(
        "id=$(xprop -root _NET_ACTIVE_WINDOW 2>/dev/null | sed 's/.* //'); "
        "[ -n \"$id\" ] && [ \"$id\" != 0x0 ] && "
        "xprop -id \"$id\" WM_NAME WM_CLASS 2>/dev/null");
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

char *whelk_session_close_window_command(WhelkSession session) {
  switch (session) {
  case WHELK_SESSION_NIRI:
    return g_strdup("niri msg action close-window");
  case WHELK_SESSION_SWAY:
    return g_strdup("swaymsg kill");
  case WHELK_SESSION_X11:
    /* Whichever of the two EWMH clients the desktop happens to ship. */
    return g_strdup("wmctrl -c :ACTIVE: 2>/dev/null || "
                    "xdotool getactivewindow windowclose 2>/dev/null");
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

char *whelk_session_shortcut_tool(WhelkSession session) {
  const char *wanted = session == WHELK_SESSION_X11 ? "xdotool" : "wtype";

  return g_find_program_in_path(wanted);
}

/* --- niri ---------------------------------------------------------------- */

static gboolean parse_niri(const char *text, char **title, char **app_id) {
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root)
    return FALSE;

  const char *parsed_title = whelk_json_object_string(root, "title");
  const char *parsed_app = whelk_json_object_string(root, "app_id");
  gboolean found = parsed_title != NULL || parsed_app != NULL;

  if (found) {
    *title = g_strdup(parsed_title ? parsed_title : "");
    *app_id = g_strdup(parsed_app ? parsed_app : "");
  }

  whelk_json_free(root);

  return found;
}

/* --- sway ---------------------------------------------------------------- */

static gboolean walk_sway(WhelkJson *node, char **title, char **app_id) {
  if (!node)
    return FALSE;

  const gboolean focused = whelk_json_object_bool(node, "focused", FALSE);

  if (focused) {
    const char *name = whelk_json_object_string(node, "name");
    const char *app = whelk_json_object_string(node, "app_id");
    WhelkJson *properties = whelk_json_object_get(node, "window_properties");
    const char *class_name =
        properties ? whelk_json_object_string(properties, "class") : NULL;

    if (name || app || class_name) {
      *title = g_strdup(name ? name : "");
      *app_id = g_strdup(app && *app ? app : (class_name ? class_name : ""));
      return TRUE;
    }
  }

  WhelkJson *nodes = whelk_json_object_get(node, "nodes");
  const guint count = nodes ? whelk_json_array_length(nodes) : 0;

  for (guint i = 0; i < count; i++)
    if (walk_sway(whelk_json_array_get(nodes, i), title, app_id))
      return TRUE;

  WhelkJson *floating = whelk_json_object_get(node, "floating_nodes");
  const guint floating_count = floating ? whelk_json_array_length(floating) : 0;

  for (guint i = 0; i < floating_count; i++)
    if (walk_sway(whelk_json_array_get(floating, i), title, app_id))
      return TRUE;

  return FALSE;
}

static gboolean parse_sway(const char *text, char **title, char **app_id) {
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root)
    return FALSE;

  const gboolean found = walk_sway(root, title, app_id);

  whelk_json_free(root);

  return found;
}

/* --- xprop --------------------------------------------------------------- */

/* "WM_CLASS(STRING) = \"navigator\", \"Firefox\"" -> Firefox. */
static char *xprop_quoted_value(const char *line, guint index) {
  const char *cursor = line;

  for (guint seen = 0;; seen++) {
    cursor = strchr(cursor, '"');

    if (!cursor)
      return NULL;

    const char *end = strchr(cursor + 1, '"');

    if (!end)
      return NULL;

    if (seen == index)
      return g_strndup(cursor + 1, (gsize)(end - cursor - 1));

    cursor = end + 1;
  }
}

static gboolean parse_x11(const char *text, char **title, char **app_id) {
  g_auto(GStrv) lines = g_strsplit(text, "\n", -1);
  char *utf8_name = NULL;
  char *plain_name = NULL;
  char *class_name = NULL;
  char *instance = NULL;

  for (guint i = 0; lines[i]; i++) {
    if (g_str_has_prefix(lines[i], "WM_NAME(UTF8_STRING)"))
      utf8_name = xprop_quoted_value(lines[i], 0);
    else if (g_str_has_prefix(lines[i], "WM_NAME(STRING)") ||
             g_str_has_prefix(lines[i], "WM_NAME("))
      plain_name = xprop_quoted_value(lines[i], 0);
    else if (g_str_has_prefix(lines[i], "WM_CLASS")) {
      instance = xprop_quoted_value(lines[i], 0);
      class_name = xprop_quoted_value(lines[i], 1);
    }
  }

  const char *chosen_title = utf8_name ? utf8_name : plain_name;
  const char *chosen_app = class_name ? class_name : instance;

  if (!chosen_title && !chosen_app) {
    g_free(utf8_name);
    g_free(plain_name);
    g_free(instance);
    g_free(class_name);
    return FALSE;
  }

  *title = g_strdup(chosen_title ? chosen_title : "");
  *app_id = g_strdup(chosen_app ? chosen_app : "");

  g_free(utf8_name);
  g_free(plain_name);
  g_free(instance);
  g_free(class_name);

  return TRUE;
}

gboolean whelk_session_parse_focused(WhelkSession session, const char *text,
                                     char **title, char **app_id) {
  if (title)
    *title = NULL;

  if (app_id)
    *app_id = NULL;

  if (!text || *text == '\0' || !title || !app_id)
    return FALSE;

  switch (session) {
  case WHELK_SESSION_NIRI:
    return parse_niri(text, title, app_id);
  case WHELK_SESSION_SWAY:
    return parse_sway(text, title, app_id);
  case WHELK_SESSION_X11:
    return parse_x11(text, title, app_id);
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return FALSE;
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/session.h"

#include "logger.h"
#include "util/json.h"

#include <signal.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/* sway's CLI takes the socket from the environment, and a sway started by a
 * service unit exports SWAYSOCK only to the processes it spawned.  The socket
 * itself sits in the runtime directory, newest first -- the same one swaymsg
 * would have used. */
#define SWAY_SOCKET_DISCOVERY                                                  \
  "if [ -z \"$SWAYSOCK\" ]; then "                                             \
  "SWAYSOCK=$(ls -t \"$XDG_RUNTIME_DIR\"/sway-ipc.*.sock 2>/dev/null | "        \
  "head -1); [ -n \"$SWAYSOCK\" ] && export SWAYSOCK; fi; "

typedef struct {
  WhelkSession session;
  const char *command;
  const char *evidence;
} SessionProbe;

/* Cheap, side-effect free, and answerable without the environment: niri finds
 * its own default socket by path, sway's is found by the prefix above. */
static const SessionProbe probes[] = {
    {WHELK_SESSION_NIRI, "niri msg -j version",
     "niri msg -j version answered"},
    {WHELK_SESSION_SWAY, SWAY_SOCKET_DISCOVERY "swaymsg -t get_version",
     "swaymsg -t get_version answered"},
};

/* The probe's shell is looked up rather than assumed: a Guix System and the
 * package's build sandbox have no /bin/sh, only the store's bash. */
static const char *probe_shell(void) {
  static gsize shell = 0;

  if (g_once_init_enter(&shell)) {
    g_autofree char *found = g_find_program_in_path("sh");

    g_once_init_leave(&shell, (gsize)(found ? g_strdup(found)
                                           : g_strdup("/bin/sh")));
  }

  return (const char *)shell;
}

gboolean whelk_session_command_answers(const char *command, guint timeout_ms) {
  if (!command || *command == '\0')
    return FALSE;

  char *argv[] = {(char *)probe_shell(), (char *)"-c", (char *)command, NULL};
  GPid pid = 0;
  g_autoptr(GError) error = NULL;

  /* Nothing is read from the probe: its exit status is the answer, and a
   * compositor that hangs must not hang the bar. */
  if (!g_spawn_async(NULL, argv, NULL,
                     G_SPAWN_DO_NOT_REAP_CHILD | G_SPAWN_SEARCH_PATH |
                         G_SPAWN_STDOUT_TO_DEV_NULL |
                         G_SPAWN_STDERR_TO_DEV_NULL,
                     NULL, NULL, &pid, &error))
    return FALSE;

  const gint64 deadline = g_get_monotonic_time() + (gint64)timeout_ms * 1000;
  gboolean answered = FALSE;

  for (;;) {
    int status = 0;
    const pid_t waited = waitpid(pid, &status, WNOHANG);

    if (waited == pid) {
      answered = WIFEXITED(status) && WEXITSTATUS(status) == 0;
      break;
    }

    if (waited < 0)
      break;

    if (g_get_monotonic_time() >= deadline) {
      kill(pid, SIGKILL);
      waitpid(pid, &status, 0);
      break;
    }

    g_usleep(10 * 1000);
  }

  g_spawn_close_pid(pid);

  return answered;
}

static WhelkSession from_environment(const char **evidence) {
  const char *niri = g_getenv("NIRI_SOCKET");
  const char *sway = g_getenv("SWAYSOCK");
  const char *display = g_getenv("DISPLAY");
  const char *wayland = g_getenv("WAYLAND_DISPLAY");
  const char *type = g_getenv("XDG_SESSION_TYPE");

  if (niri && *niri) {
    if (evidence)
      *evidence = "NIRI_SOCKET";

    return WHELK_SESSION_NIRI;
  }

  if (sway && *sway) {
    if (evidence)
      *evidence = "SWAYSOCK";

    return WHELK_SESSION_SWAY;
  }

  if (type && g_ascii_strcasecmp(type, "x11") == 0) {
    if (evidence)
      *evidence = "XDG_SESSION_TYPE=x11";

    return WHELK_SESSION_X11;
  }

  if ((!wayland || !*wayland) && display && *display) {
    if (evidence)
      *evidence = "DISPLAY without WAYLAND_DISPLAY";

    return WHELK_SESSION_X11;
  }

  return WHELK_SESSION_WAYLAND;
}

WhelkSession whelk_session_from_environment(void) {
  return from_environment(NULL);
}

const char *whelk_session_probe_command(WhelkSession session) {
  for (guint i = 0; i < G_N_ELEMENTS(probes); i++)
    if (probes[i].session == session)
      return probes[i].command;

  return NULL;
}

WhelkSession whelk_session_from_probe(gboolean (*answers)(const char *command)) {
  if (!answers)
    return WHELK_SESSION_WAYLAND;

  for (guint i = 0; i < G_N_ELEMENTS(probes); i++)
    if (answers(probes[i].command))
      return probes[i].session;

  return WHELK_SESSION_WAYLAND;
}

WhelkSession whelk_session_detect(void) {
  static gsize detected = 0;
  static WhelkSession session = WHELK_SESSION_WAYLAND;

  if (g_once_init_enter(&detected)) {
    const char *evidence = NULL;
    WhelkSession found = from_environment(&evidence);

    /* No variable: ask the compositor itself.  A bar started by a service unit
     * or from a login shell inherits none of them, and neither niri nor sway
     * needs one to answer -- their CLIs resolve the socket by path. */
    if (!evidence) {
      for (guint i = 0; i < G_N_ELEMENTS(probes); i++) {
        if (whelk_session_command_answers(probes[i].command, 400)) {
          found = probes[i].session;
          evidence = probes[i].evidence;
          break;
        }
      }
    }

    session = found;

    if (evidence)
      LOG_C_INFO("Session", "%s, from %s", whelk_session_name(session),
                 evidence);
    else
      LOG_C_WARN("Session",
                 "no compositor answered (niri msg, swaymsg); the window and "
                 "menu modules stay empty until one does");

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
    return g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg -t get_tree");
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
    return g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg kill");
  case WHELK_SESSION_X11:
    /* Whichever of the two EWMH clients the desktop happens to ship. */
    return g_strdup("wmctrl -c :ACTIVE: 2>/dev/null || "
                    "xdotool getactivewindow windowclose 2>/dev/null");
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

char *whelk_session_fullscreen_window_command(WhelkSession session) {
  switch (session) {
  case WHELK_SESSION_NIRI:
    return g_strdup("niri msg action fullscreen-window");
  case WHELK_SESSION_SWAY:
    return g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg fullscreen toggle");
  case WHELK_SESSION_X11:
    return g_strdup("wmctrl -r :ACTIVE: -b toggle,fullscreen 2>/dev/null || "
                    "xdotool getactivewindow key F11 2>/dev/null");
  case WHELK_SESSION_WAYLAND:
    break;
  }

  return NULL;
}

void whelk_session_window_actions(WhelkSession session,
                                  WhelkSessionWindowActions *actions) {
  if (!actions)
    return;

  memset(actions, 0, sizeof *actions);
  actions->close = whelk_session_close_window_command(session);
  actions->fullscreen = whelk_session_fullscreen_window_command(session);

  switch (session) {
  case WHELK_SESSION_NIRI:
    actions->maximize = g_strdup("niri msg action maximize-column");
    actions->float_window = g_strdup("niri msg action toggle-window-floating");
    actions->move_left = g_strdup("niri msg action move-window-left");
    actions->move_right = g_strdup("niri msg action move-window-right");
    actions->move_up = g_strdup("niri msg action move-window-up");
    actions->move_down = g_strdup("niri msg action move-window-down");
    break;
  case WHELK_SESSION_SWAY:
    actions->float_window =
        g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg floating toggle");
    actions->move_left = g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg move left");
    actions->move_right = g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg move right");
    actions->move_up = g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg move up");
    actions->move_down = g_strdup(SWAY_SOCKET_DISCOVERY "swaymsg move down");
    break;
  case WHELK_SESSION_X11:
    actions->maximize = g_strdup(
        "wmctrl -r :ACTIVE: -b toggle,maximized_vert,maximized_horz "
        "2>/dev/null");
    break;
  case WHELK_SESSION_WAYLAND:
    break;
  }
}

void whelk_session_window_actions_clear(WhelkSessionWindowActions *actions) {
  if (!actions)
    return;

  g_free(actions->close);
  g_free(actions->fullscreen);
  g_free(actions->maximize);
  g_free(actions->float_window);
  g_free(actions->move_left);
  g_free(actions->move_right);
  g_free(actions->move_up);
  g_free(actions->move_down);
  memset(actions, 0, sizeof *actions);
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

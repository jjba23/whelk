// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_SESSION_H
#define WHELK_SESSION_H

#include <glib.h>

/* =========================================================================
 * THE SESSION
 * =========================================================================
 * A bar sits on whatever the machine is running.  Two questions have different
 * answers per session -- what is in front, and how to close it -- and the rest
 * of the shell should not carry an `if niri' in every module.
 *
 * WheLK talks to the compositor the way a shell has to: through the tools that
 * exist.  niri and sway answer over their own IPC; on an X11 desktop, EWMH is
 * the same information in `xprop'.  GNOME's Wayland session offers a shell
 * none of this (Shell.Eval is gone), so that session is named honestly and the
 * modules that need a compositor go quiet instead of printing errors.
 *
 * The environment is the first guess, never the verdict: NIRI_SOCKET and
 * SWAYSOCK are set for the processes a compositor spawns, and absent for one
 * started by a service unit, a login shell or another compositor's session.
 * When they are missing, the compositor is asked instead -- `niri msg' finds
 * its own default socket without the variable, and sway's socket is found in
 * the runtime directory -- because a bar that goes blank because a variable was
 * not exported is a bar that has forgotten what it can do.
 */

typedef enum {
  WHELK_SESSION_NIRI,
  WHELK_SESSION_SWAY,
  WHELK_SESSION_X11,
  WHELK_SESSION_WAYLAND, /* a Wayland compositor with no shell-facing IPC */
} WhelkSession;

/* Detected once, by the environment and then by asking the compositor; logged
 * either way, so a session that ends up unsupported says so. */
WhelkSession whelk_session_detect(void);

/* The environment's answer alone, without touching the compositor: NIRI_SOCKET,
 * SWAYSOCK, XDG_SESSION_TYPE, DISPLAY.  WAYLAND means "the environment does not
 * name one". */
WhelkSession whelk_session_from_environment(void);

/**
 * whelk_session_from_probe:
 * @answers: called with each probe command in turn; TRUE when it succeeded.
 *
 * The compositor's answer from a probe, for callers that supply their own
 * transport (and for the tests).  WAYLAND when nothing answers.
 */
WhelkSession whelk_session_from_probe(gboolean (*answers)(const char *command));

/* The command that proves a compositor is reachable, or NULL for sessions that
 * have nothing to ask. */
const char *whelk_session_probe_command(WhelkSession session);

/**
 * whelk_session_command_answers:
 * @command: a shell command line.
 * @timeout_ms: how long to wait before killing it.
 *
 * Runs @command and reports whether it exited zero.  Output is discarded: the
 * exit status is the whole answer.
 */
gboolean whelk_session_command_answers(const char *command, guint timeout_ms);

/* "niri", "sway", "x11" or "wayland". */
const char *whelk_session_name(WhelkSession session);

/* The command that prints the focused window in the shape this session's
 * parser understands; NULL when the session cannot answer. */
char *whelk_session_focused_window_command(WhelkSession session);

/* The command that closes the focused window; NULL when there is none. */
char *whelk_session_close_window_command(WhelkSession session);

/* The command that toggles the focused window's full-screen state; NULL when
 * the session has no such action. */
char *whelk_session_fullscreen_window_command(WhelkSession session);

/**
 * WhelkSessionWindowActions:
 * @close: command that closes the focused window.
 * @fullscreen: command that toggles full screen.
 * @maximize: command that toggles maximisation, or NULL.
 * @float_window: command that toggles floating, or NULL.
 * @move_left: command that moves the window left, or NULL.
 * @move_right: command that moves it right.
 * @move_up: command that moves it up.
 * @move_down: command that moves it down.
 *
 * The focused-window verbs a session can carry out.  A NULL field is a verb
 * the session has no reliable command for, and the menu leaves it out rather
 * than showing an entry that does nothing.
 */
typedef struct {
  char *close;
  char *fullscreen;
  char *maximize;
  char *float_window;
  char *move_left;
  char *move_right;
  char *move_up;
  char *move_down;
} WhelkSessionWindowActions;

void whelk_session_window_actions(WhelkSession session,
                                  WhelkSessionWindowActions *actions);
void whelk_session_window_actions_clear(WhelkSessionWindowActions *actions);

/**
 * whelk_session_parse_focused:
 * @session: the session the text came from.
 * @text: the command's output.
 * @title: (out) (transfer full) (nullable): the window's title.
 * @app_id: (out) (transfer full) (nullable): its application id or class.
 *
 * Understands niri's focused-window JSON, sway's window tree and the
 * `WM_NAME'/`WM_CLASS' lines of xprop.
 *
 * Returns: TRUE when a window was found.
 */
gboolean whelk_session_parse_focused(WhelkSession session, const char *text,
                                     char **title, char **app_id);

/**
 * WhelkFocusedPaths:
 * @bus_name: the `_GTK_UNIQUE_BUS_NAME` of the focused window, or NULL.
 * @window_id: the X11 window id, or 0 when the session is not X11.
 * @app_path: its `_GTK_APPLICATION_OBJECT_PATH`, or NULL.
 * @window_path: its `_GTK_WINDOW_OBJECT_PATH`, or NULL.
 * @menubar_path: its `_GTK_MENUBAR_OBJECT_PATH`, or NULL.
 * @appmenu_path: its `_GTK_APP_MENU_OBJECT_PATH`, or NULL.
 *
 * The paths a GTK application publishes on its X11 window.  A Wayland
 * compositor does not expose another client's window properties, so these
 * are filled only on X11; the application id remains the fallback everywhere.
 */
typedef struct {
  guint32 window_id;
  char *bus_name;
  char *app_path;
  char *window_path;
  char *menubar_path;
  char *appmenu_path;
} WhelkFocusedPaths;

void whelk_focused_paths_clear(WhelkFocusedPaths *paths);

/**
 * whelk_session_parse_focused_paths:
 * @session: the session the text came from.
 * @text: the focused-window command's output.
 * @paths: (out caller-allocates): cleared and filled with the window's GTK
 *   object paths; every field is NULL when the window published none.
 *
 * On X11, GTK exports the exact bus name and menu object paths as window
 * properties.  Reading them turns the conventional-path guesses into exact
 * probes, and reaches applications whose application id is not a valid bus
 * name (Firefox's `WM_CLASS`, for example).  Other sessions leave @paths empty:
 * their protocols do not expose another client's properties.
 */
void whelk_session_parse_focused_paths(WhelkSession session, const char *text,
                                       WhelkFocusedPaths *paths);

/* The program that types a key chord into the focused window: wtype on
 * Wayland, xdotool on X11, or NULL when neither is installed. */
char *whelk_session_shortcut_tool(WhelkSession session);

#endif // WHELK_SESSION_H

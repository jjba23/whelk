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

/* The program that types a key chord into the focused window: wtype on
 * Wayland, xdotool on X11, or NULL when neither is installed. */
char *whelk_session_shortcut_tool(WhelkSession session);

#endif // WHELK_SESSION_H

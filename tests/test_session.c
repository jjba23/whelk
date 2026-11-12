// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/session.h"

#include <glib.h>

/* The same two questions on three desktops: what is in front, and how to close
 * it.  The commands are strings; the parsing is where the differences live. */

static void test_session_names(void) {
  g_assert_cmpstr(whelk_session_name(WHELK_SESSION_NIRI), ==, "niri");
  g_assert_cmpstr(whelk_session_name(WHELK_SESSION_SWAY), ==, "sway");
  g_assert_cmpstr(whelk_session_name(WHELK_SESSION_X11), ==, "x11");
  g_assert_cmpstr(whelk_session_name(WHELK_SESSION_WAYLAND), ==, "wayland");
}

static void test_session_commands(void) {
  g_autofree char *niri =
      whelk_session_focused_window_command(WHELK_SESSION_NIRI);
  g_autofree char *sway =
      whelk_session_focused_window_command(WHELK_SESSION_SWAY);
  g_autofree char *x11 =
      whelk_session_focused_window_command(WHELK_SESSION_X11);

  g_assert_nonnull(strstr(niri, "niri msg -j focused-window"));
  g_assert_nonnull(strstr(sway, "swaymsg -t get_tree"));
  g_assert_nonnull(strstr(x11, "xprop"));
  g_assert_nonnull(strstr(x11, "_NET_ACTIVE_WINDOW"));

  /* GNOME's Wayland session has no shell-facing answer, and says so. */
  g_assert_null(whelk_session_focused_window_command(WHELK_SESSION_WAYLAND));

  g_autofree char *niri_close =
      whelk_session_close_window_command(WHELK_SESSION_NIRI);
  g_autofree char *sway_close =
      whelk_session_close_window_command(WHELK_SESSION_SWAY);
  g_autofree char *x11_close =
      whelk_session_close_window_command(WHELK_SESSION_X11);

  g_assert_nonnull(strstr(niri_close, "close-window"));
  g_assert_nonnull(strstr(sway_close, "kill"));
  g_assert_nonnull(strstr(x11_close, "wmctrl"));
  g_assert_null(whelk_session_close_window_command(WHELK_SESSION_WAYLAND));
}

static void test_parse_niri(void) {
  char *title = NULL;
  char *app_id = NULL;

  g_assert_true(whelk_session_parse_focused(
      WHELK_SESSION_NIRI,
      "{\"id\": 5, \"title\": \"Home\", \"app_id\": \"org.gnome.Nautilus\"}",
      &title, &app_id));
  g_assert_cmpstr(title, ==, "Home");
  g_assert_cmpstr(app_id, ==, "org.gnome.Nautilus");
  g_free(title);
  g_free(app_id);

  g_assert_false(
      whelk_session_parse_focused(WHELK_SESSION_NIRI, "{\"id\": null}", &title,
                                  &app_id));
  g_assert_false(
      whelk_session_parse_focused(WHELK_SESSION_NIRI, "not json", &title,
                                  &app_id));
}

static void test_parse_sway(void) {
  const char *tree =
      "{\"name\": \"root\", \"focused\": false,"
      " \"nodes\": [{\"name\": \"1\", \"nodes\": ["
      "   {\"name\": \"Notes\", \"app_id\": \"org.gnome.TextEditor\","
      "    \"focused\": true}], \"floating_nodes\": []}],"
      " \"floating_nodes\": []}";
  char *title = NULL;
  char *app_id = NULL;

  g_assert_true(whelk_session_parse_focused(WHELK_SESSION_SWAY, tree, &title,
                                           &app_id));
  g_assert_cmpstr(title, ==, "Notes");
  g_assert_cmpstr(app_id, ==, "org.gnome.TextEditor");
  g_free(title);
  g_free(app_id);
}

static void test_parse_sway_x11_window(void) {
  const char *tree =
      "{\"nodes\": [{\"name\": \"Report\", \"app_id\": null,"
      " \"focused\": true, \"window_properties\": {\"class\": \"LibreOffice\"}}],"
      " \"floating_nodes\": []}";
  char *title = NULL;
  char *app_id = NULL;

  g_assert_true(whelk_session_parse_focused(WHELK_SESSION_SWAY, tree, &title,
                                           &app_id));
  g_assert_cmpstr(app_id, ==, "LibreOffice");
  g_free(title);
  g_free(app_id);
}

static void test_parse_x11(void) {
  const char *xprop = "WM_NAME(UTF8_STRING) = \"Mozilla Firefox\"\n"
                      "WM_CLASS(STRING) = \"Navigator\", \"Firefox\"\n";
  char *title = NULL;
  char *app_id = NULL;

  g_assert_true(whelk_session_parse_focused(WHELK_SESSION_X11, xprop, &title,
                                           &app_id));
  g_assert_cmpstr(title, ==, "Mozilla Firefox");
  g_assert_cmpstr(app_id, ==, "Firefox");
  g_free(title);
  g_free(app_id);

  /* No window: xprop says so by printing nothing. */
  g_assert_false(
      whelk_session_parse_focused(WHELK_SESSION_X11, "", &title, &app_id));
  g_assert_false(
      whelk_session_parse_focused(WHELK_SESSION_X11, "xprop: no such atom",
                                  &title, &app_id));
}

static void test_parse_unsupported_session(void) {
  char *title = NULL;
  char *app_id = NULL;

  g_assert_false(whelk_session_parse_focused(
      WHELK_SESSION_WAYLAND, "{\"title\": \"anything\"}", &title, &app_id));
}


/* --- detection without the environment -------------------------------------
 * The regression this guards: a bar started by a service unit or a login shell
 * inherits no NIRI_SOCKET, and `niri msg' worked anyway because it resolves the
 * default socket itself.  Detection has to ask the compositor, not trust the
 * environment. */

static void clear_session_environment(void) {
  g_unsetenv("NIRI_SOCKET");
  g_unsetenv("SWAYSOCK");
  g_unsetenv("XDG_SESSION_TYPE");
  g_unsetenv("DISPLAY");
  g_unsetenv("WAYLAND_DISPLAY");
}

static void test_environment_detection(void) {
  clear_session_environment();

  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_WAYLAND);

  g_setenv("NIRI_SOCKET", "/tmp/niri.sock", TRUE);
  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_NIRI);

  g_unsetenv("NIRI_SOCKET");
  g_setenv("SWAYSOCK", "/tmp/sway.sock", TRUE);
  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_SWAY);

  g_unsetenv("SWAYSOCK");
  g_setenv("XDG_SESSION_TYPE", "x11", TRUE);
  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_X11);

  g_unsetenv("XDG_SESSION_TYPE");
  g_setenv("DISPLAY", ":0", TRUE);
  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_X11);

  /* A Wayland display makes DISPLAY an XWayland leftover, not a session. */
  g_setenv("WAYLAND_DISPLAY", "wayland-1", TRUE);
  g_assert_cmpint(whelk_session_from_environment(), ==, WHELK_SESSION_WAYLAND);

  clear_session_environment();
}

static gboolean answers_niri(const char *command) {
  return g_str_has_prefix(command, "niri msg");
}

static gboolean answers_sway(const char *command) {
  return strstr(command, "swaymsg") != NULL;
}

static gboolean answers_nothing(const char *command) {
  (void)command;
  return FALSE;
}

static void test_probe_detection(void) {
  g_assert_cmpint(whelk_session_from_probe(answers_niri), ==,
                  WHELK_SESSION_NIRI);
  g_assert_cmpint(whelk_session_from_probe(answers_sway), ==,
                  WHELK_SESSION_SWAY);
  g_assert_cmpint(whelk_session_from_probe(answers_nothing), ==,
                  WHELK_SESSION_WAYLAND);
  g_assert_cmpint(whelk_session_from_probe(NULL), ==, WHELK_SESSION_WAYLAND);
}

static void test_probe_commands(void) {
  g_assert_nonnull(strstr(whelk_session_probe_command(WHELK_SESSION_NIRI),
                          "niri msg -j version"));
  g_assert_nonnull(strstr(whelk_session_probe_command(WHELK_SESSION_SWAY),
                          "swaymsg -t get_version"));
  /* sway's socket is discovered when the variable is missing, so that the same
   * command works in a session that never exported it. */
  g_assert_nonnull(strstr(whelk_session_probe_command(WHELK_SESSION_SWAY),
                          "sway-ipc.*.sock"));
  g_assert_null(whelk_session_probe_command(WHELK_SESSION_X11));
  g_assert_null(whelk_session_probe_command(WHELK_SESSION_WAYLAND));
}

static void test_command_answers(void) {
  g_assert_true(whelk_session_command_answers("exit 0", 2000));
  g_assert_false(whelk_session_command_answers("exit 3", 2000));
  g_assert_false(whelk_session_command_answers("/nonexistent/probe", 2000));
  g_assert_false(whelk_session_command_answers(NULL, 2000));
}

static void test_command_times_out(void) {
  const gint64 started = g_get_monotonic_time();

  g_assert_false(whelk_session_command_answers("sleep 5", 300));

  /* The wait is bounded, so a hung compositor costs the probe's timeout and
   * not the bar's first paint. */
  g_assert_cmpint(g_get_monotonic_time() - started, <, 2 * G_USEC_PER_SEC);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/session/names", test_session_names);
  g_test_add_func("/session/commands", test_session_commands);
  g_test_add_func("/session/parse/niri", test_parse_niri);
  g_test_add_func("/session/parse/sway", test_parse_sway);
  g_test_add_func("/session/parse/sway-x11", test_parse_sway_x11_window);
  g_test_add_func("/session/parse/x11", test_parse_x11);
  g_test_add_func("/session/parse/unsupported",
                  test_parse_unsupported_session);
  g_test_add_func("/session/environment", test_environment_detection);
  g_test_add_func("/session/probe", test_probe_detection);
  g_test_add_func("/session/probe/commands", test_probe_commands);
  g_test_add_func("/session/answers", test_command_answers);
  g_test_add_func("/session/answers/timeout", test_command_times_out);

  return g_test_run();
}

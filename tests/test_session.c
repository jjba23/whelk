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

  return g_test_run();
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The two workspace JSON shapes, against answers the compositors really print.
// Hyprland's command concatenates `workspaces' and `monitors' with a marker, so
// the active workspace can be read from the monitor that owns it.

#include "util/workspaces.h"

#include <glib.h>
#include <string.h>

static void test_niri(void) {
  const char *text =
      "[{\"id\": 1, \"idx\": 1, \"name\": \"one\", \"is_active\": true,"
      "  \"is_focused\": false, \"active_window_id\": 42},"
      " {\"id\": 2, \"idx\": 2, \"name\": \"two\", \"is_active\": false,"
      "  \"is_focused\": true, \"active_window_id\": null},"
      " {\"id\": 3, \"idx\": -1, \"name\": \"scratch\"}]";
  GPtrArray *workspaces = whelk_workspaces_parse(WHELK_SESSION_NIRI, text);

  g_assert_nonnull(workspaces);
  g_assert_cmpuint(workspaces->len, ==, 2);

  const WhelkWorkspace *one = g_ptr_array_index(workspaces, 0);
  g_assert_cmpint(one->index, ==, 1);
  g_assert_cmpstr(one->name, ==, "one");
  g_assert_true(one->active);
  g_assert_false(one->empty);

  const WhelkWorkspace *two = g_ptr_array_index(workspaces, 1);
  g_assert_cmpint(two->index, ==, 2);
  g_assert_cmpstr(two->name, ==, "two");
  g_assert_true(two->active);
  g_assert_true(two->empty);

  g_ptr_array_unref(workspaces);

  /* A malformed answer is not an empty workspace set. */
  g_assert_null(whelk_workspaces_parse(WHELK_SESSION_NIRI, "not json"));
  g_assert_null(whelk_workspaces_parse(WHELK_SESSION_NIRI, "{}"));
}

static void test_hyprland(void) {
  const char *text =
      "[{\"id\": 1, \"name\": \"one\", \"monitor\": \"DP-1\","
      "  \"monitorID\": 0, \"windows\": 2, \"hasfullscreen\": false,"
      "  \"lastwindow\": \"0x1\", \"lastwindowtitle\": \"A\"},"
      " {\"id\": 2, \"name\": \"two\", \"monitor\": \"DP-1\","
      "  \"monitorID\": 0, \"windows\": 0, \"hasfullscreen\": false,"
      "  \"lastwindow\": \"0x0\", \"lastwindowtitle\": \"\"},"
      " {\"id\": -99, \"name\": \"special:magic\", \"monitor\": \"DP-1\","
      "  \"monitorID\": 0, \"windows\": 1, \"hasfullscreen\": false}]\n\n"
      "WHELK_HYPRLAND_MONITORS\n\n"
      "[{\"id\": 0, \"name\": \"DP-1\", \"focused\": true,"
      "  \"activeWorkspace\": {\"id\": 2, \"name\": \"two\"}}]";
  GPtrArray *workspaces = whelk_workspaces_parse(WHELK_SESSION_HYPRLAND, text);

  g_assert_nonnull(workspaces);
  g_assert_cmpuint(workspaces->len, ==, 2);

  const WhelkWorkspace *one = g_ptr_array_index(workspaces, 0);
  g_assert_cmpint(one->index, ==, 1);
  g_assert_cmpstr(one->name, ==, "one");
  g_assert_false(one->active);
  g_assert_false(one->empty);

  const WhelkWorkspace *two = g_ptr_array_index(workspaces, 1);
  g_assert_cmpint(two->index, ==, 2);
  g_assert_true(two->active);
  g_assert_true(two->empty);

  g_ptr_array_unref(workspaces);

  /* The workspaces answer alone is still usable; without monitors, no button
   * is active rather than every button being guessed active. */
  workspaces = whelk_workspaces_parse(
      WHELK_SESSION_HYPRLAND,
      "[{\"id\": 7, \"name\": \"seven\", \"windows\": 1}]");
  g_assert_nonnull(workspaces);
  g_assert_cmpuint(workspaces->len, ==, 1);
  g_assert_false(
      ((const WhelkWorkspace *)g_ptr_array_index(workspaces, 0))->active);
  g_ptr_array_unref(workspaces);
}

static void test_commands(void) {
  g_autofree char *niri = whelk_workspaces_command(WHELK_SESSION_NIRI);
  g_autofree char *hypr = whelk_workspaces_command(WHELK_SESSION_HYPRLAND);

  g_assert_nonnull(strstr(niri, "niri msg -j workspaces"));
  g_assert_nonnull(strstr(hypr, "hyprctl -j workspaces"));
  /* The three answers are separated by ordinary newline-producing echos, not
   * by escape sequences the shell might leave literal. */
  g_assert_nonnull(strstr(hypr, "echo; echo WHELK_HYPRLAND_MONITORS; echo;"));
  g_assert_nonnull(strstr(hypr, "hyprctl -j monitors"));
  g_assert_null(whelk_workspaces_command(WHELK_SESSION_SWAY));
  g_assert_null(whelk_workspaces_command(WHELK_SESSION_RIVER));
  g_assert_null(whelk_workspaces_command(WHELK_SESSION_WAYLAND));

  g_autofree char *niri_focus =
      whelk_workspaces_focus_command(WHELK_SESSION_NIRI, 3);
  g_autofree char *hypr_focus =
      whelk_workspaces_focus_command(WHELK_SESSION_HYPRLAND, 3);

  g_assert_nonnull(strstr(niri_focus, "focus-workspace 3"));
  g_assert_nonnull(strstr(hypr_focus, "workspace 3"));
  g_assert_null(whelk_workspaces_focus_command(WHELK_SESSION_RIVER, 3));
  g_assert_null(whelk_workspaces_focus_command(WHELK_SESSION_NIRI, -1));
}

static void test_empty(void) {
  GPtrArray *workspaces = whelk_workspaces_parse(WHELK_SESSION_NIRI, "[]");

  g_assert_nonnull(workspaces);
  g_assert_cmpuint(workspaces->len, ==, 0);
  g_ptr_array_unref(workspaces);

  g_assert_null(whelk_workspaces_parse(WHELK_SESSION_HYPRLAND, NULL));
  g_assert_null(whelk_workspaces_parse(WHELK_SESSION_RIVER, "[]"));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/workspaces/niri", test_niri);
  g_test_add_func("/workspaces/hyprland", test_hyprland);
  g_test_add_func("/workspaces/commands", test_commands);
  g_test_add_func("/workspaces/empty", test_empty);

  return g_test_run();
}

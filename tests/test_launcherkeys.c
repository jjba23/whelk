// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The launcher's key map is data, not a switch in the overlay: these tests pin
// the built-in prefixes, the override that replaces one, the empty override
// that disables one, and the longer-prefix rule.

#include "util/launcherkeys.h"

#include <glib.h>

static void test_built_in_map(void) {
  WhelkLauncherKeys *keys = whelk_launcher_keys_new();
  const char *query = NULL;

  g_assert_cmpstr(whelk_launcher_keys_default_mode(keys), ==, "app");
  g_assert_true(whelk_launcher_keys_is_mode("clipboard"));
  g_assert_false(whelk_launcher_keys_is_mode("not-a-mode"));

  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "@firefox", &query),
                  ==, "window");
  g_assert_cmpstr(query, ==, "firefox");

  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, ">ls", &query), ==,
                  "run");
  g_assert_cmpstr(query, ==, "ls");

  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "=2+2", &query), ==,
                  "calc");
  g_assert_cmpstr(query, ==, "2+2");

  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "/etc", &query), ==,
                  "file");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "?whelk", &query), ==,
                  "web");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "%host", &query), ==,
                  "ssh");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "!editor", &query),
                  ==, "quit");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, ";copy", &query), ==,
                  "clipboard");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, ":Save", &query), ==,
                  "menuitem");

  /* A query with no prefix is no mode: the launcher keeps its current one. */
  g_assert_null(whelk_launcher_keys_mode_for_text(keys, "firefox", &query));
  g_assert_cmpstr(query, ==, "firefox");

  whelk_launcher_keys_free(keys);
}

static void test_overrides(void) {
  WhelkLauncherKeys *keys = whelk_launcher_keys_new();
  const char *query = NULL;

  whelk_launcher_keys_set_prefix(keys, "window", "");
  g_assert_cmpstr(whelk_launcher_keys_prefix(keys, "window"), ==, "");
  g_assert_null(whelk_launcher_keys_mode_for_text(keys, "@firefox", &query));
  g_assert_cmpstr(query, ==, "@firefox");

  whelk_launcher_keys_set_prefix(keys, "run", "//");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, "//ls", &query), ==,
                  "run");
  g_assert_cmpstr(query, ==, "ls");

  /* The old one-character key is gone, not shared. */
  g_assert_null(whelk_launcher_keys_mode_for_text(keys, ">ls", &query));

  /* A saved field can be cleared, and an unknown mode cannot be chosen. */
  whelk_launcher_keys_set_default_mode(keys, "clipboard");
  g_assert_cmpstr(whelk_launcher_keys_default_mode(keys), ==, "clipboard");
  whelk_launcher_keys_set_default_mode(keys, "not-a-mode");
  g_assert_cmpstr(whelk_launcher_keys_default_mode(keys), ==, "clipboard");

  whelk_launcher_keys_free(keys);
}

static void test_longest_prefix_wins(void) {
  WhelkLauncherKeys *keys = whelk_launcher_keys_new();
  const char *query = NULL;

  whelk_launcher_keys_set_prefix(keys, "ssh", ">>");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, ">>host", &query), ==,
                  "ssh");
  g_assert_cmpstr(query, ==, "host");

  /* One `>' still means run, because it is its own (shorter) match. */
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(keys, ">ls", &query), ==,
                  "run");
  g_assert_cmpstr(query, ==, "ls");

  whelk_launcher_keys_free(keys);
}

static void test_null_map_is_defaults(void) {
  const char *query = NULL;

  g_assert_cmpstr(whelk_launcher_keys_default_mode(NULL), ==, "app");
  g_assert_cmpstr(whelk_launcher_keys_mode_for_text(NULL, "@wp", &query), ==,
                  "window");
  g_assert_cmpstr(query, ==, "wp");
  g_assert_null(whelk_launcher_keys_mode_for_text(NULL, NULL, &query));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/launcherkeys/defaults", test_built_in_map);
  g_test_add_func("/launcherkeys/overrides", test_overrides);
  g_test_add_func("/launcherkeys/longest-prefix", test_longest_prefix_wins);
  g_test_add_func("/launcherkeys/null-map", test_null_map_is_defaults);

  return g_test_run();
}

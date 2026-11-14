// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// A bar's monitor rule is the difference between a layout and a fallback.
// These are the decisions the shell makes on every build and every hotplug:
// which outputs a bar belongs on, and when it belongs on none of them.

#include "config.h"
#include "util/barmonitors.h"

#include <glib.h>

static const char *const two_outputs[] = {"eDP-1", "DP-1", NULL};

static void clear_rule(WhelkBarConfig *bar) {
  g_ptr_array_set_size(bar->monitors, 0);
  g_free(bar->monitor);
  bar->monitor = g_strdup("all");
  bar->when = WHELK_BAR_WHEN_ALWAYS;
}

static void assert_selection(WhelkBarConfig *bar, guint count,
                             const char *const *connectors,
                             const gint *expected, guint expected_len) {
  GArray *selected = g_array_new(FALSE, FALSE, sizeof(gint));

  whelk_bar_select_monitors(bar, count, connectors, selected);
  g_assert_cmpuint(selected->len, ==, expected_len);

  for (guint i = 0; i < expected_len; i++)
    g_assert_cmpint(g_array_index(selected, gint, i), ==, expected[i]);

  g_array_unref(selected);
}

static void test_every_output(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("main");
  const gint expected[] = {0, 1};

  assert_selection(bar, 2, two_outputs, expected, G_N_ELEMENTS(expected));
  whelk_bar_config_free(bar);
}

static void test_named_connector(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("external");

  clear_rule(bar);
  g_free(bar->monitor);
  bar->monitor = g_strdup("DP-1");

  const gint expected[] = {1};

  assert_selection(bar, 2, two_outputs, expected, G_N_ELEMENTS(expected));

  /* The external screen is gone: this bar waits for it rather than moving to
   * the laptop's panel. */
  assert_selection(bar, 1, two_outputs, NULL, 0);

  whelk_bar_config_free(bar);
}

static void test_connector_list(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("both");

  g_ptr_array_add(bar->monitors, g_strdup("eDP-1"));
  g_ptr_array_add(bar->monitors, g_strdup("HDMI-A-1"));

  const gint expected[] = {0};

  assert_selection(bar, 2, two_outputs, expected, G_N_ELEMENTS(expected));
  assert_selection(bar, 0, NULL, NULL, 0);
  whelk_bar_config_free(bar);
}

static void test_output_count_rule(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("second-screen");

  bar->when = WHELK_BAR_WHEN_MULTI;

  const gint both[] = {0, 1};

  assert_selection(bar, 2, two_outputs, both, G_N_ELEMENTS(both));
  assert_selection(bar, 1, two_outputs, NULL, 0);
  assert_selection(bar, 0, NULL, NULL, 0);

  bar->when = WHELK_BAR_WHEN_SINGLE;

  const gint laptop[] = {0};

  assert_selection(bar, 1, two_outputs, laptop, G_N_ELEMENTS(laptop));
  assert_selection(bar, 2, two_outputs, NULL, 0);

  whelk_bar_config_free(bar);
}

static void test_default_output(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("default");
  const gint expected[] = {WHELK_MONITOR_DEFAULT};

  clear_rule(bar);
  g_free(bar->monitor);
  bar->monitor = g_strdup("default");

  assert_selection(bar, 2, two_outputs, expected, G_N_ELEMENTS(expected));

  /* No monitor model: "all" still gets one surface, and the compositor places
   * it, which is what a start before the outputs are announced needs. */
  clear_rule(bar);
  assert_selection(bar, 0, NULL, expected, G_N_ELEMENTS(expected));

  whelk_bar_config_free(bar);
}

static void test_copies_carry_the_rule(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("copy");

  g_ptr_array_add(bar->monitors, g_strdup("DP-1"));
  bar->when = WHELK_BAR_WHEN_MULTI;

  WhelkBarConfig *copy = whelk_bar_config_copy(bar);

  g_assert_cmpint(copy->when, ==, WHELK_BAR_WHEN_MULTI);
  g_assert_cmpuint(copy->monitors->len, ==, 1);
  g_assert_cmpstr(g_ptr_array_index(copy->monitors, 0), ==, "DP-1");

  whelk_bar_config_free(bar);
  whelk_bar_config_free(copy);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/barmonitors/all", test_every_output);
  g_test_add_func("/barmonitors/connector", test_named_connector);
  g_test_add_func("/barmonitors/list", test_connector_list);
  g_test_add_func("/barmonitors/count", test_output_count_rule);
  g_test_add_func("/barmonitors/default", test_default_output);
  g_test_add_func("/barmonitors/copy", test_copies_carry_the_rule);

  return g_test_run();
}

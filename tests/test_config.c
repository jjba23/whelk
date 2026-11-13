// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "config.h"
#include "util/format.h"

#include <glib.h>

/* The option table is the contract between the Guile bridge and every module:
 * a value arrives as text and each reader has to be honest about what it can
 * and cannot parse. */

static void test_option_readers(void) {
  WhelkModuleSpec *spec = whelk_module_spec_new("cpu");

  whelk_module_spec_set(spec, "interval", "2");
  whelk_module_spec_set(spec, "format", "{icon} {usage}%");
  whelk_module_spec_set(spec, "spacing", "2.5");
  whelk_module_spec_set(spec, "hide-empty?", "#t");
  whelk_module_spec_set(spec, "broken", "2px");
  whelk_module_spec_set(spec, "empty", "");

  g_assert_cmpint(whelk_spec_int(spec, "interval", 9), ==, 2);
  g_assert_cmpstr(whelk_spec_str(spec, "format", ""), ==, "{icon} {usage}%");

  /* A number with a unit is not a number. */
  g_assert_cmpint(whelk_spec_int(spec, "broken", 9), ==, 9);
  g_assert_cmpfloat(whelk_spec_double(spec, "spacing", 0.0), ==, 2.5);
  g_assert_true(whelk_spec_bool(spec, "hide-empty?", FALSE));

  /* An empty value is a missing value, so the module's default applies. */
  g_assert_false(whelk_spec_has(spec, "empty"));
  g_assert_cmpstr(whelk_spec_str(spec, "empty", "fallback"), ==, "fallback");
  g_assert_cmpstr(whelk_spec_str(spec, "absent", "fallback"), ==, "fallback");

  whelk_module_spec_free(spec);
}

static void test_format_helpers(void) {
  g_autofree char *substituted =
      whelk_str_subst("{icon} {usage}% of {usage}", "usage", "42");

  g_assert_cmpstr(substituted, ==, "{icon} 42% of 42");

  g_autofree char *escaped = whelk_markup_escape("a <b> & c");
  g_assert_cmpstr(escaped, ==, "a &lt;b&gt; &amp; c");

  g_autofree char *icon = whelk_icon_markup("\uf2db", "Font Awesome 6 Free");
  g_assert_cmpstr(icon, ==,
                  "<span font_family=\"Font Awesome 6 Free\">\uf2db</span>");

  g_autofree char *truncated =
      whelk_str_truncate_utf8("a rather long window title", 8);
  g_assert_cmpstr(truncated, ==, "a rather…");

  g_autofree char *untouched = whelk_str_truncate_utf8("short", 8);
  g_assert_cmpstr(untouched, ==, "short");

  g_autofree char *first = whelk_str_first_line("Nothing playing\nsecond\n");
  g_assert_cmpstr(first, ==, "Nothing playing");
}

static void test_bar_copy(void) {
  WhelkBarConfig *bar = whelk_bar_config_new("main");
  WhelkModuleSpec *clock = whelk_module_spec_new("clock");

  whelk_module_spec_set(clock, "format", "%H:%M");
  g_ptr_array_add(bar->right, clock);

  g_free(bar->monitor);
  bar->monitor = g_strdup("DP-1");
  bar->height = 40;
  bar->margin_top = 4;
  bar->layer = GTK_LAYER_SHELL_LAYER_BOTTOM;
  bar->accent = g_strdup("#8fd16a");

  WhelkBarConfig *copy = whelk_bar_config_copy(bar);

  /* The copy is deep: the original can be freed underneath it. */
  whelk_bar_config_free(bar);

  g_assert_cmpstr(copy->name, ==, "main");
  g_assert_cmpstr(copy->monitor, ==, "DP-1");
  g_assert_cmpstr(copy->accent, ==, "#8fd16a");
  g_assert_cmpint(copy->height, ==, 40);
  g_assert_cmpint(copy->margin_top, ==, 4);
  g_assert_cmpint(copy->layer, ==, GTK_LAYER_SHELL_LAYER_BOTTOM);
  g_assert_cmpuint(copy->right->len, ==, 1);

  WhelkModuleSpec *copied_clock = g_ptr_array_index(copy->right, 0);

  g_assert_cmpstr(copied_clock->kind, ==, "clock");
  g_assert_cmpstr(whelk_spec_str(copied_clock, "format", ""), ==, "%H:%M");

  whelk_bar_config_free(copy);
}

static void test_default_bar(void) {
  GPtrArray *bars = whelk_config_default_bars();

  g_assert_cmpuint(bars->len, ==, 1);

  WhelkBarConfig *bar = g_ptr_array_index(bars, 0);

  g_assert_cmpint(bar->edge, ==, GTK_LAYER_SHELL_EDGE_TOP);
  g_assert_cmpint(bar->layer, ==, GTK_LAYER_SHELL_LAYER_TOP);
  g_assert_true(bar->right->len > 0);

  whelk_config_free_bars(bars);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/whelk/config/options", test_option_readers);
  g_test_add_func("/whelk/config/format", test_format_helpers);
  g_test_add_func("/whelk/config/bar_copy", test_bar_copy);
  g_test_add_func("/whelk/config/default_bar", test_default_bar);

  return g_test_run();
}

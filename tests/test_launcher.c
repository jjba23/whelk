// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/calc.h"
#include "util/fuzzy.h"
#include "util/launchergeometry.h"

#include <glib.h>

/* The two pure halves of the launcher: how a query finds a name, and what an
 * arithmetic expression is worth. */

static void test_fuzzy_subsequence(void) {
  g_assert_cmpint(whelk_fuzzy_score("ffx", "Firefox"), >, 0);
  g_assert_cmpint(whelk_fuzzy_score("wterm", "WezTerm"), >, 0);
  g_assert_cmpint(whelk_fuzzy_score("xyz", "Firefox"), ==, -1);

  /* An empty query matches everything. */
  g_assert_cmpint(whelk_fuzzy_score("", "Firefox"), ==, 0);
  g_assert_cmpint(whelk_fuzzy_score("f", ""), ==, -1);
}

static void test_fuzzy_ordering(void) {
  /* A prefix beats a match buried in the middle, and a word start beats an
   * interior character. */
  g_assert_cmpint(whelk_fuzzy_score("term", "Terminal"),
                  >, whelk_fuzzy_score("term", "WezTerm"));
  g_assert_cmpint(whelk_fuzzy_score("fx", "Firefox"),
                  >, whelk_fuzzy_score("fx", "Suffix"));

  /* Case does not matter. */
  g_assert_cmpint(whelk_fuzzy_score("FIRE", "firefox"), ==,
                  whelk_fuzzy_score("fire", "firefox"));
}

static void test_fuzzy_best(void) {
  const char *const candidates[] = {"Files", "Firefox", "Terminal", NULL};

  g_assert_cmpint(whelk_fuzzy_best("fire", candidates), ==, 1);
  g_assert_cmpint(whelk_fuzzy_best("term", candidates), ==, 2);
  g_assert_cmpint(whelk_fuzzy_best("zzz", candidates), ==, -1);
  g_assert_cmpint(whelk_fuzzy_best("fi", candidates), ==, 0);
}

static void test_calc_arithmetic(void) {
  double value = 0.0;

  g_assert_true(whelk_calc_eval("1 + 2 * 3", &value));
  g_assert_cmpfloat(value, ==, 7.0);

  g_assert_true(whelk_calc_eval("(1 + 2) * 3", &value));
  g_assert_cmpfloat(value, ==, 9.0);

  g_assert_true(whelk_calc_eval("2 ^ 3 ^ 2", &value));
  g_assert_cmpfloat(value, ==, 512.0);

  g_assert_true(whelk_calc_eval("-4 + 10 / 5", &value));
  g_assert_cmpfloat(value, ==, -2.0);

  g_assert_true(whelk_calc_eval("10 % 4", &value));
  g_assert_cmpfloat(value, ==, 2.0);

  g_assert_true(whelk_calc_eval("1.5 * 2", &value));
  g_assert_cmpfloat(value, ==, 3.0);
}

static void test_calc_rejects(void) {
  double value = 0.0;

  g_assert_false(whelk_calc_eval("", &value));
  g_assert_false(whelk_calc_eval("1 +", &value));
  g_assert_false(whelk_calc_eval("1 / 0", &value));
  g_assert_false(whelk_calc_eval("hello", &value));
  g_assert_false(whelk_calc_eval("2x", &value));
  g_assert_false(whelk_calc_eval("(1 + 2", &value));
}

static void test_calc_format(void) {
  g_autofree char *whole = whelk_calc_format(42.0);
  g_autofree char *fraction = whelk_calc_format(2.5);

  g_assert_cmpstr(whole, ==, "42");
  g_assert_cmpstr(fraction, ==, "2.5");
}

/* The launcher's scroll position follows the selected row, not just the
 * selection: an arrow key at the bottom of the view must bring the next row
 * into view. */
static void test_scroll_follows_selection(void) {
  g_assert_cmpfloat_with_epsilon(
      whelk_launcher_scroll_value(0, 100, 0, 500, 150, 180), 80.0, 0.01);
  g_assert_cmpfloat_with_epsilon(
      whelk_launcher_scroll_value(80, 100, 0, 500, 300, 330), 230.0, 0.01);
  g_assert_cmpfloat_with_epsilon(
      whelk_launcher_scroll_value(80, 100, 0, 500, 70, 100), 70.0, 0.01);

  /* A list shorter than the viewport has nowhere to scroll. */
  g_assert_cmpfloat_with_epsilon(
      whelk_launcher_scroll_value(0, 100, 0, 60, 10, 30), 0.0, 0.01);
}

/* Clicking the dimmed area outside the card is the launcher's other close
 * gesture, alongside the new close button and Esc. */
static void test_click_outside_card(void) {
  g_assert_false(whelk_launcher_point_outside(50, 100, 10, 20, 110, 220));
  g_assert_false(whelk_launcher_point_outside(10, 20, 10, 20, 110, 220));
  g_assert_true(whelk_launcher_point_outside(9, 100, 10, 20, 110, 220));
  g_assert_true(whelk_launcher_point_outside(50, 9, 10, 20, 110, 220));
  g_assert_true(whelk_launcher_point_outside(111, 100, 10, 20, 110, 220));
  g_assert_true(whelk_launcher_point_outside(50, 221, 10, 20, 110, 220));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/whelk/fuzzy/subsequence", test_fuzzy_subsequence);
  g_test_add_func("/whelk/fuzzy/ordering", test_fuzzy_ordering);
  g_test_add_func("/whelk/fuzzy/best", test_fuzzy_best);
  g_test_add_func("/whelk/calc/arithmetic", test_calc_arithmetic);
  g_test_add_func("/whelk/calc/rejects", test_calc_rejects);
  g_test_add_func("/whelk/calc/format", test_calc_format);
  g_test_add_func("/whelk/launcher/scroll", test_scroll_follows_selection);
  g_test_add_func("/whelk/launcher/outside", test_click_outside_card);

  return g_test_run();
}

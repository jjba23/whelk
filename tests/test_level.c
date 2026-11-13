// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/level.h"

/* The first reading a bar takes is not a change: showing a HUD on startup for
 * a volume nobody touched would be the shell talking to itself.  After that,
 * only a different number or a different mute state raises it. */

static void test_first_reading_is_not_a_change(void) {
  WhelkLevel level = {0};

  g_assert_false(whelk_level_update(&level, 42, FALSE));
  g_assert_true(level.known);
  g_assert_cmpint(level.percent, ==, 42);
}

static void test_the_same_reading_is_not_a_change(void) {
  WhelkLevel level = {0};

  whelk_level_update(&level, 42, FALSE);
  g_assert_false(whelk_level_update(&level, 42, FALSE));
}

static void test_a_different_number_is_a_change(void) {
  WhelkLevel level = {0};

  whelk_level_update(&level, 42, FALSE);
  g_assert_true(whelk_level_update(&level, 55, FALSE));
  g_assert_cmpint(level.percent, ==, 55);

  g_assert_false(whelk_level_update(&level, 55, FALSE));
}

static void test_muting_is_a_change_at_the_same_number(void) {
  WhelkLevel level = {0};

  whelk_level_update(&level, 42, FALSE);
  g_assert_true(whelk_level_update(&level, 42, TRUE));
  g_assert_true(level.muted);
  g_assert_false(whelk_level_update(&level, 42, TRUE));
  g_assert_true(whelk_level_update(&level, 42, FALSE));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/level/first-reading", test_first_reading_is_not_a_change);
  g_test_add_func("/level/same-reading", test_the_same_reading_is_not_a_change);
  g_test_add_func("/level/new-number", test_a_different_number_is_a_change);
  g_test_add_func("/level/mute", test_muting_is_a_change_at_the_same_number);

  return g_test_run();
}

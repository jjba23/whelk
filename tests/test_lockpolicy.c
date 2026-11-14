// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/lockpolicy.h"

#include <glib.h>

static void test_submission_guard(void) {
  g_assert_true(whelk_lock_submission_allowed(FALSE, 0, TRUE));
  g_assert_false(whelk_lock_submission_allowed(TRUE, 0, TRUE));
  g_assert_false(whelk_lock_submission_allowed(FALSE, 1, TRUE));
  g_assert_false(whelk_lock_submission_allowed(FALSE, 0, FALSE));
}

static void test_delays(void) {
  g_assert_cmpuint(whelk_lock_retry_delay_ms(0), ==, 0);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(1), ==, 1000);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(2), ==, 2000);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(3), ==, 4000);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(4), ==, 8000);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(5), ==, 16000);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(6), ==, WHELK_LOCK_MAX_DELAY_MS);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(100), ==,
                   WHELK_LOCK_MAX_DELAY_MS);
  g_assert_cmpuint(whelk_lock_retry_delay_ms(G_MAXUINT), ==,
                   WHELK_LOCK_MAX_DELAY_MS);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/lockpolicy/delays", test_delays);
  g_test_add_func("/lockpolicy/submission-guard", test_submission_guard);

  return g_test_run();
}

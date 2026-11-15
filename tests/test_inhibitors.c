// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The inhibitor reader turns `systemd-inhibit --list' into the answer to "why
// did my screen not blank".  The parsing is the part worth testing: the
// command's columns are padded, and a reason can contain spaces.

#include "util/inhibitors.h"

#include <glib.h>

static const char *sample =
    "WHO            UID  USER  PID  COMM           WHAT                   "
    "WHY                          MODE\n"
    "ModemManager   0    root  754  ModemManager   sleep                  "
    "Modem dialing                delay\n"
    "firefox        1000  joe   4242  firefox        idle                   "
    "Playing video                block\n";

static void test_parse(void) {
  g_autoptr(GPtrArray) list = whelk_inhibitors_parse(sample);

  g_assert_cmpuint(list->len, ==, 2);

  g_autofree char *text = whelk_inhibitors_describe(list);

  g_assert_nonnull(strstr(text, "2 inhibitors"));
  g_assert_nonnull(strstr(text, "sleep — ModemManager: Modem dialing"));
  g_assert_nonnull(strstr(text, "Playing video"));
  g_assert_nonnull(strstr(text, "(block)"));
}

static void test_empty(void) {
  g_assert_cmpuint(whelk_inhibitors_parse(NULL)->len, ==, 0);
  g_assert_cmpuint(whelk_inhibitors_parse("")->len, ==, 0);
  g_assert_cmpuint(whelk_inhibitors_parse("WHO UID USER PID COMM WHAT WHY "
                                          "MODE\n")
                       ->len,
                   ==, 0);
  g_assert_null(whelk_inhibitors_describe(NULL));
}

static void test_garbage(void) {
  /* A missing tool prints nothing; a tool that changed shape prints lines this
   * reader does not understand.  Neither becomes an inhibitor. */
  g_autoptr(GPtrArray) list =
      whelk_inhibitors_parse("systemd-inhibit: command not found\n");

  g_assert_cmpuint(list->len, ==, 0);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/inhibitors/parse", test_parse);
  g_test_add_func("/inhibitors/empty", test_empty);
  g_test_add_func("/inhibitors/garbage", test_garbage);

  return g_test_run();
}

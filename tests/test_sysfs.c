// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/sysfs.h"

#include <glib.h>

/* The status word is the kernel's, and the shell must not read more into it
 * than it says: a full pack and a plugged-in pack that is not taking power
 * are both not filling. */

static void test_only_charging_is_charging(void) {
  g_assert_true(whelk_battery_is_charging("Charging"));
  g_assert_false(whelk_battery_is_charging("Full"));
  g_assert_false(whelk_battery_is_charging("Not charging"));
  g_assert_false(whelk_battery_is_charging("Discharging"));
  g_assert_false(whelk_battery_is_charging("Unknown"));
  g_assert_false(whelk_battery_is_charging(NULL));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/whelk/sysfs/only-charging-is-charging",
                  test_only_charging_is_charging);

  return g_test_run();
}

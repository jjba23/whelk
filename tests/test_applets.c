// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/applets.h"

#include <glib.h>

/* Which tray items a bar that reads its radios itself leaves out.  The rule is
 * an exact id: the loose word "network" once hid an item that only mentioned
 * it in its title. */

static void test_network_applets(void) {
  g_assert_true(whelk_applet_is_network("nm-applet"));
  g_assert_true(whelk_applet_is_network("nm-tray"));
  g_assert_true(whelk_applet_is_network("org.kde.networkmanagement"));

  /* Ids are compared case-insensitively and trimmed. */
  g_assert_true(whelk_applet_is_network("NM-Applet"));
  g_assert_true(whelk_applet_is_network("  nm-applet  "));
}

static void test_network_near_misses(void) {
  /* The old rule's words: none of these is an applet id. */
  g_assert_false(whelk_applet_is_network("network"));
  g_assert_false(whelk_applet_is_network("wifi"));
  g_assert_false(whelk_applet_is_network("my-network-monitor"));

  /* An id that merely starts with an applet's id is a different applet. */
  g_assert_false(whelk_applet_is_network("nm-applet-extra"));
  g_assert_false(
      whelk_applet_is_network("org.kde.networkmanagement.something"));

  g_assert_false(whelk_applet_is_network(NULL));
  g_assert_false(whelk_applet_is_network(""));
  g_assert_false(whelk_applet_is_network("   "));
}

static void test_bluetooth_applets(void) {
  g_assert_true(whelk_applet_is_bluetooth("blueman"));
  g_assert_true(whelk_applet_is_bluetooth("blueman-tray"));
  g_assert_true(whelk_applet_is_bluetooth("blueberry"));
  g_assert_true(whelk_applet_is_bluetooth("org.kde.bluetooth"));

  /* Titles are not ids, and one radio's applet is not the other's. */
  g_assert_false(whelk_applet_is_bluetooth("bluetooth"));
  g_assert_false(whelk_applet_is_bluetooth("bluetooth-sendto"));
  g_assert_false(whelk_applet_is_bluetooth("nm-applet"));
  g_assert_false(whelk_applet_is_network("blueman"));
  g_assert_false(whelk_applet_is_bluetooth(NULL));
}

int main(int argc, char *argv[]) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/applets/network", test_network_applets);
  g_test_add_func("/applets/network-near-misses", test_network_near_misses);
  g_test_add_func("/applets/bluetooth", test_bluetooth_applets);

  return g_test_run();
}

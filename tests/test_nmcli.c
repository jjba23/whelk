// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/nmcli.h"

#include <glib.h>

/* A typed Wi-Fi password has to reach NetworkManager's saved profile.  These
 * are the two halves of that: reading nmcli's terse answers, and the editor
 * script that carries the key on standard input. */

static void test_split_plain(void) {
  g_autoptr(GPtrArray) fields = whelk_nmcli_split_terse("Manon-en-Joe:89:WPA2");

  g_assert_cmpuint(fields->len, ==, 3);
  g_assert_cmpstr(g_ptr_array_index(fields, 0), ==, "Manon-en-Joe");
  g_assert_cmpstr(g_ptr_array_index(fields, 1), ==, "89");
  g_assert_cmpstr(g_ptr_array_index(fields, 2), ==, "WPA2");
}

static void test_split_honours_escapes(void) {
  /* An SSID may contain a colon, which nmcli escapes rather than lose. */
  g_autoptr(GPtrArray) fields =
      whelk_nmcli_split_terse("Cafe\\:Guest:77:WPA2");

  g_assert_cmpuint(fields->len, ==, 3);
  g_assert_cmpstr(g_ptr_array_index(fields, 0), ==, "Cafe:Guest");
  g_assert_cmpstr(g_ptr_array_index(fields, 1), ==, "77");

  /* A backslash is escaped the same way, and a trailing one is kept. */
  g_autoptr(GPtrArray) slashes = whelk_nmcli_split_terse("a\\\\b:1");
  g_assert_cmpstr(g_ptr_array_index(slashes, 0), ==, "a\\b");

  g_autoptr(GPtrArray) empty = whelk_nmcli_split_terse("open::88");
  g_assert_cmpstr(g_ptr_array_index(empty, 0), ==, "open");
  g_assert_cmpstr(g_ptr_array_index(empty, 1), ==, "");
}

static void test_profile_names(void) {
  const char *output =
      "Manon-en-Joe:802-11-wireless\n"
      "lo:loopback\n"
      "Cafe\\:Guest:802-11-wireless\n"
      "Wired:802-3-ethernet\n"
      "\n";

  g_autoptr(GPtrArray) names = whelk_nmcli_wifi_profile_names(output);

  g_assert_cmpuint(names->len, ==, 2);
  g_assert_cmpstr(g_ptr_array_index(names, 0), ==, "Manon-en-Joe");
  g_assert_cmpstr(g_ptr_array_index(names, 1), ==, "Cafe:Guest");

  g_autoptr(GPtrArray) none = whelk_nmcli_wifi_profile_names(NULL);
  g_assert_cmpuint(none->len, ==, 0);
}

static void test_wifi_reading_hides_the_ssid(void) {
  /* The bar should not name the network to anyone who can see the screen. */
  g_assert_cmpstr(whelk_nmcli_wifi_format(FALSE), ==, "{icon}");
  g_assert_cmpstr(whelk_nmcli_wifi_tooltip(FALSE), ==,
                  "Wi-Fi · {signal}%\nClick: networks");

  /* #:show-ssid? #t asks for the name back, in both places. */
  g_assert_cmpstr(whelk_nmcli_wifi_format(TRUE), ==, "{icon} {ssid}");
  g_assert_cmpstr(whelk_nmcli_wifi_tooltip(TRUE), ==,
                  "{ssid} · {signal}%\nClick: networks");
}

static void test_key_script_is_literal(void) {
  /* The editor takes the rest of the line as the value: quotes are part of the
   * key, never syntax around it, and nothing is interpolated by a shell. */
  g_autofree char *script =
      whelk_nmcli_key_update_script("p@ss \"word\" $x\\y; rm -rf /");

  g_assert_cmpstr(script, ==,
                  "set 802-11-wireless-security.psk "
                  "p@ss \"word\" $x\\y; rm -rf /\n"
                  "save\nquit\n");
}

static void test_key_script_refuses_unwritable_keys(void) {
  g_assert_null(whelk_nmcli_key_update_script(NULL));
  g_assert_null(whelk_nmcli_key_update_script(""));

  /* A newline would end the command and begin another. */
  g_assert_null(whelk_nmcli_key_update_script("first\nsave"));
  g_assert_null(whelk_nmcli_key_update_script("tab\there"));

  /* The editor trims the value's edges, so these would be stored changed. */
  g_assert_null(whelk_nmcli_key_update_script(" leading"));
  g_assert_null(whelk_nmcli_key_update_script("trailing "));
}

int main(int argc, char *argv[]) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/nmcli/split-terse", test_split_plain);
  g_test_add_func("/nmcli/split-escapes", test_split_honours_escapes);
  g_test_add_func("/nmcli/profile-names", test_profile_names);
  g_test_add_func("/nmcli/wifi-hides-ssid", test_wifi_reading_hides_the_ssid);
  g_test_add_func("/nmcli/key-script", test_key_script_is_literal);
  g_test_add_func("/nmcli/key-script-refusals", test_key_script_refuses_unwritable_keys);

  return g_test_run();
}

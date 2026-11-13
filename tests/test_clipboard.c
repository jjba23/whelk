// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <glib.h>

/* The history rule and the pipe reader, without a compositor: what the watcher
 * does with each copy is the part that can be wrong in ways a running desktop
 * would only show much later. */

static GPtrArray *history(void) {
  return g_ptr_array_new_with_free_func(g_free);
}

static const char *at(GPtrArray *entries, guint index) {
  return g_ptr_array_index(entries, index);
}

static void test_newest_first(void) {
  g_autoptr(GPtrArray) entries = history();

  whelk_clipboard_history_add(entries, "one", 50);
  whelk_clipboard_history_add(entries, "two", 50);

  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(at(entries, 0), ==, "two");
  g_assert_cmpstr(at(entries, 1), ==, "one");
}

static void test_repeat_moves_to_the_front(void) {
  g_autoptr(GPtrArray) entries = history();

  whelk_clipboard_history_add(entries, "one", 50);
  whelk_clipboard_history_add(entries, "two", 50);
  whelk_clipboard_history_add(entries, "one", 50);

  /* The same copy is not two entries: it is the first one again. */
  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(at(entries, 0), ==, "one");
  g_assert_cmpstr(at(entries, 1), ==, "two");
}

static void test_empty_copies_are_not_history(void) {
  g_autoptr(GPtrArray) entries = history();

  whelk_clipboard_history_add(entries, "", 50);
  whelk_clipboard_history_add(entries, NULL, 50);

  g_assert_cmpuint(entries->len, ==, 0);
}

static void test_oldest_falls_off(void) {
  g_autoptr(GPtrArray) entries = history();

  whelk_clipboard_history_add(entries, "one", 2);
  whelk_clipboard_history_add(entries, "two", 2);
  whelk_clipboard_history_add(entries, "three", 2);

  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(at(entries, 0), ==, "three");
  g_assert_cmpstr(at(entries, 1), ==, "two");
}

static void test_zero_means_the_default_cap(void) {
  g_autoptr(GPtrArray) entries = history();

  for (guint i = 0; i < 80; i++) {
    g_autofree char *text = g_strdup_printf("%u", i);

    whelk_clipboard_history_add(entries, text, 0);
  }

  g_assert_cmpuint(entries->len, ==, 50);
  g_assert_cmpstr(at(entries, 0), ==, "79");
}

static void test_split_takes_only_finished_items(void) {
  GString *pending = g_string_new(NULL);
  g_autoptr(GPtrArray) items = g_ptr_array_new_with_free_func(g_free);

  /* A C string literal stops at the NUL, so the three parts are appended the
   * way the pipe delivers them. */
  g_string_append(pending, "first\nitem");
  g_string_append_c(pending, '\0');
  g_string_append(pending, "second");

  /* The second item has no NUL yet: it is still arriving, so it stays in the
   * buffer rather than becoming a truncated entry. */
  whelk_clipboard_split(pending, items);

  g_assert_cmpuint(items->len, ==, 1);
  g_assert_cmpstr(g_ptr_array_index(items, 0), ==, "first\nitem");
  g_assert_cmpstr(pending->str, ==, "second");

  g_string_append_c(pending, '\0');
  whelk_clipboard_split(pending, items);

  g_assert_cmpuint(items->len, ==, 2);
  g_assert_cmpstr(g_ptr_array_index(items, 1), ==, "second");
  g_assert_cmpuint(pending->len, ==, 0);

  g_string_free(pending, TRUE);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/clipboard/newest-first", test_newest_first);
  g_test_add_func("/clipboard/repeat-moves-up",
                  test_repeat_moves_to_the_front);
  g_test_add_func("/clipboard/empty-ignored",
                  test_empty_copies_are_not_history);
  g_test_add_func("/clipboard/cap", test_oldest_falls_off);
  g_test_add_func("/clipboard/default-cap", test_zero_means_the_default_cap);
  g_test_add_func("/clipboard/split", test_split_takes_only_finished_items);

  return g_test_run();
}

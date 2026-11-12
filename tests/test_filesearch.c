// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/filesearch.h"

#include <glib.h>

/* Searching the whole home directory: which paths a query finds, in what
 * order, and what the walk is told to skip. */

static void test_score_basename_beats_parent(void) {
  /* The file you named is the one you meant, even when the query also appears
   * in a directory further up the path. */
  g_assert_cmpint(whelk_file_score("notes", "/home/joe/notes/lista.txt", FALSE),
                  >, whelk_file_score("notes", "/home/joe/lista-notes/tmp.txt",
                                      FALSE));
}

static void test_score_prefix_beats_inside(void) {
  g_assert_cmpint(whelk_file_score("bar", "/home/joe/bar.c", FALSE), >,
                  whelk_file_score("bar", "/home/joe/foobar.c", FALSE));
}

static void test_score_shallow_beats_deep(void) {
  g_assert_cmpint(whelk_file_score("whelk", "/home/joe/whelk/src/bar.c", FALSE),
                  >, whelk_file_score("whelk", "/home/joe/a/b/c/d/e/whelk/bar.c",
                                      FALSE));
}

static void test_score_case_insensitive(void) {
  g_assert_cmpint(whelk_file_score("WHELK", "/home/joe/whelk/README.org",
                                   FALSE),
                  >, -1);
  g_assert_cmpint(whelk_file_score("whelk", "/home/joe/Whelk/README.org",
                                   FALSE),
                  >, -1);
}

static void test_score_misses(void) {
  g_assert_cmpint(whelk_file_score("zzzz", "/home/joe/whelk", TRUE), ==, -1);
  g_assert_cmpint(whelk_file_score("", "/home/joe/whelk", TRUE), ==, 0);
  g_assert_cmpint(whelk_file_score("a", "", FALSE), ==, -1);
}

static void test_score_files_beat_directories(void) {
  g_assert_cmpint(whelk_file_score("whelk", "/home/joe/src/whelk", TRUE), <,
                  whelk_file_score("whelk", "/home/joe/src/whelk", FALSE));
}

static void test_rank_orders_and_limits(void) {
  /* The walk prints a type character, a tab, then the path. */
  const char *output = "f\t/home/joe/notes/lista.txt\n"
                       "f\t/home/joe/whelk/README.org\n"
                       "d\t/home/joe/whelk\n"
                       "f\t/home/joe/whelk/src/bar.c\n"
                       "\n";

  g_autoptr(GPtrArray) ranked = whelk_file_search_rank("whelk", output, 10);

  g_assert_cmpuint(ranked->len, ==, 3);
  /* The directory named after the query, then the shallower file. */
  g_assert_cmpstr(g_ptr_array_index(ranked, 0), ==, "/home/joe/whelk");
  g_assert_cmpstr(g_ptr_array_index(ranked, 1), ==,
                  "/home/joe/whelk/README.org");
  g_assert_cmpstr(g_ptr_array_index(ranked, 2), ==,
                  "/home/joe/whelk/src/bar.c");

  g_autoptr(GPtrArray) limited = whelk_file_search_rank("whelk", output, 1);

  g_assert_cmpuint(limited->len, ==, 1);
}

static void test_rank_empty_query_finds_nothing(void) {
  g_autoptr(GPtrArray) ranked =
      whelk_file_search_rank("", "/home/joe/whelk\n", 10);

  g_assert_cmpuint(ranked->len, ==, 0);
}

static void test_command_prunes_noise(void) {
  g_autofree char *command = whelk_file_search_command("/home/joe", 6);

  g_assert_nonnull(strstr(command, "'/home/joe'"));
  g_assert_nonnull(strstr(command, "-maxdepth 6"));
  g_assert_nonnull(strstr(command, "-prune"));
  g_assert_nonnull(strstr(command, "-type d"));
  g_assert_nonnull(strstr(command, "-printf"));

  /* The walk skips what nobody means to search and what nobody should pay the
   * launcher to walk. */
  for (const char *const *name = whelk_file_search_ignored(); *name; name++) {
    g_autofree char *needle = g_strdup_printf("-name %s", *name);

    g_assert_nonnull(strstr(command, needle));
  }

  g_assert_nonnull(strstr(command, "-name .cache"));
  g_assert_nonnull(strstr(command, "-name .git"));
  g_assert_nonnull(strstr(command, "-name node_modules"));
  g_assert_nonnull(strstr(command, "-name .venv"));

  /* The query is ranked in C, not in find: the walk does not know it. */
  g_assert_null(strstr(command, "-iname"));
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/filesearch/score/basename-beats-parent",
                  test_score_basename_beats_parent);
  g_test_add_func("/filesearch/score/prefix-beats-inside",
                  test_score_prefix_beats_inside);
  g_test_add_func("/filesearch/score/shallow-beats-deep",
                  test_score_shallow_beats_deep);
  g_test_add_func("/filesearch/score/case-insensitive",
                  test_score_case_insensitive);
  g_test_add_func("/filesearch/score/misses", test_score_misses);
  g_test_add_func("/filesearch/score/files-beat-directories",
                  test_score_files_beat_directories);
  g_test_add_func("/filesearch/rank/orders-and-limits",
                  test_rank_orders_and_limits);
  g_test_add_func("/filesearch/rank/empty-query",
                  test_rank_empty_query_finds_nothing);
  g_test_add_func("/filesearch/command/prunes-noise",
                  test_command_prunes_noise);

  return g_test_run();
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/filesearch.h"

#include <string.h>

/* =========================================================================
 * SEARCHING THE HOME DIRECTORY
 * =========================================================================
 * The launcher's file mode answers "where is that thing I wrote".  Two rules
 * make it useful: every directory under the home is walked, so a file is found
 * wherever it lives and not only in a handful of guessed folders, and the
 * answer arrives in the order a person would rank it -- the file whose own
 * name matches first, then the shallower paths, with directories behind files
 * of the same name.
 *
 * The walk is a `find` that prints each entry's type along with its path, so
 * ranking never has to stat a hundred thousand paths to know which ones are
 * directories.
 */

/* What the walk never enters.  These are caches, repositories, package trees
 * and version-control internals: search results nobody wants, in volumes
 * nobody wants to pay for on every keystroke. */
static const char *const ignored[] = {
    ".cache",  ".cargo",   ".git",   ".hg",  ".mozilla",
    ".npm",    ".rustup",  ".steam", ".svn", ".thunderbird",
    ".venv",   ".wine",    "node_modules", "__pycache__",
    "Trash",   NULL,
};

const char *const *whelk_file_search_ignored(void) { return ignored; }

/* Does @query appear as a whole path component of @path?  "notes" is a
 * component of "/home/joe/notes/lista.txt" but not of
 * "/home/joe/lista-notes/tmp.txt", and the difference is worth a rank. */
static gboolean has_exact_component(const char *path, const char *query) {
  const size_t length = strlen(query);
  const char *start = path;

  for (const char *cursor = path;; cursor++) {
    if (*cursor != '/' && *cursor != '\0')
      continue;

    if ((size_t)(cursor - start) == length && strncmp(start, query, length) == 0)
      return TRUE;

    if (*cursor == '\0')
      return FALSE;

    start = cursor + 1;
  }
}

int whelk_file_score(const char *query, const char *path,
                     gboolean is_directory) {
  if (!query || *query == '\0')
    return 0;

  if (!path || *path == '\0')
    return -1;

  g_autofree char *lower_query = g_utf8_strdown(query, -1);
  g_autofree char *lower_path = g_utf8_strdown(path, -1);

  const char *base = strrchr(path, '/');
  base = base ? base + 1 : path;

  g_autofree char *lower_base = g_utf8_strdown(base, -1);

  int score;

  if (g_str_has_prefix(lower_base, lower_query))
    score = 100;
  else if (strstr(lower_base, lower_query))
    score = 70;
  else if (has_exact_component(lower_path, lower_query))
    score = 45;
  else if (strstr(lower_path, lower_query))
    score = 30;
  else
    return -1;

  /* Every directory a match hides under costs a point: the file you named in
   * the home directory beats the same name six levels down. */
  for (const char *cursor = path; *cursor; cursor++)
    if (*cursor == '/')
      score--;

  /* A directory is a place, not the thing that was asked for. */
  if (is_directory)
    score -= 10;

  return score;
}

typedef struct {
  int score;
  char *path;
} RankedPath;

static gint compare_ranked(gconstpointer a, gconstpointer b) {
  const RankedPath *left = a;
  const RankedPath *right = b;

  if (left->score != right->score)
    return right->score - left->score;

  const size_t left_length = strlen(left->path);
  const size_t right_length = strlen(right->path);

  if (left_length != right_length)
    return left_length < right_length ? -1 : 1;

  return strcmp(left->path, right->path);
}

GPtrArray *whelk_file_search_rank(const char *query, const char *output,
                                  guint limit) {
  GPtrArray *paths = g_ptr_array_new_with_free_func(g_free);

  if (!query || *query == '\0' || !output)
    return paths;

  g_auto(GStrv) lines = g_strsplit(output, "\n", -1);
  GArray *ranked = g_array_new(FALSE, FALSE, sizeof(RankedPath));

  for (guint i = 0; lines[i]; i++) {
    const char *line = lines[i];

    if (*line == '\0')
      continue;

    /* `find -printf '%y\t%p\n'`: one type character, a tab, the path. */
    gboolean is_directory = FALSE;
    const char *path = line;

    if (line[0] != '\0' && line[1] == '\t') {
      is_directory = line[0] == 'd';
      path = line + 2;
    }

    const int score = whelk_file_score(query, path, is_directory);

    if (score < 0)
      continue;

    RankedPath entry = {score, g_strdup(path)};

    g_array_append_val(ranked, entry);
  }

  g_array_sort(ranked, compare_ranked);

  for (guint i = 0; i < ranked->len && i < limit; i++)
    g_ptr_array_add(paths,
                    g_strdup(g_array_index(ranked, RankedPath, i).path));

  for (guint i = 0; i < ranked->len; i++)
    g_free(g_array_index(ranked, RankedPath, i).path);

  g_array_unref(ranked);

  return paths;
}

char *whelk_file_search_command(const char *root, guint max_depth) {
  g_autofree char *quoted_root = g_shell_quote(root ? root : "");
  GString *command = g_string_new("find ");

  g_string_append_printf(command, "%s -mindepth 1 -maxdepth %u \\(",
                         quoted_root, max_depth);

  for (guint i = 0; ignored[i]; i++)
    g_string_append_printf(command, "%s-name %s", i == 0 ? " " : " -o ",
                           ignored[i]);

  /* -printf, not -print: the type comes back with the path, so ranking knows
   * which results are directories without stat'ing every entry. */
  g_string_append(command,
                  " \\) -prune -o \\( -type f -o -type d \\) "
                  "-printf '%y\\t%p\\n'");

  return g_string_free(command, FALSE);
}

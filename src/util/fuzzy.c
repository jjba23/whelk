// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/fuzzy.h"

#include <string.h>

/* Characters that start a word in the things a launcher searches: a space
 * between words, the separators of a reverse-DNS id, a path. */
static gboolean is_word_start(const char *haystack, const char *position) {
  if (position == haystack)
    return TRUE;

  switch (position[-1]) {
  case ' ':
  case '-':
  case '_':
  case '.':
  case '/':
  case ':':
  case '(':
    return TRUE;
  default:
    return FALSE;
  }
}

int whelk_fuzzy_score(const char *needle, const char *haystack) {
  if (!needle)
    return -1;

  /* An empty query is not a filter: everything matches, equally. */
  if (*needle == '\0')
    return 0;

  if (!haystack)
    return -1;

  int score = 0;
  int streak = 0;
  const char *cursor = haystack;

  for (const char *query = needle; *query; query++) {
    const char wanted = g_ascii_tolower(*query);
    gboolean found = FALSE;

    while (*cursor) {
      if (g_ascii_tolower(*cursor) == wanted) {
        found = TRUE;
        break;
      }

      cursor++;
      streak = 0;
    }

    if (!found)
      return -1;

    score += 10;

    if (is_word_start(haystack, cursor))
      score += 14;

    if (streak > 0)
      score += 6 + streak;

    streak++;
    cursor++;
  }

  /* A short name that matched is a better answer than a long one that merely
   * contains the same characters. */
  score -= (int)(strlen(haystack) / 8);

  return score;
}

int whelk_fuzzy_best(const char *needle, const char *const *candidates) {
  int best_index = -1;
  int best_score = -1;

  if (!candidates)
    return -1;

  for (int i = 0; candidates[i]; i++) {
    const int score = whelk_fuzzy_score(needle, candidates[i]);

    if (score > best_score) {
      best_score = score;
      best_index = i;
    }
  }

  return best_index;
}

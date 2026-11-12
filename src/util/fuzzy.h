// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_FUZZY_H
#define WHELK_FUZZY_H

#include <glib.h>

/* =========================================================================
 * FUZZY MATCHING
 * =========================================================================
 * The launcher's search: a query matches when its characters appear in order,
 * so "ffx" finds "Firefox" and "wterm" finds "WezTerm".  The score decides the
 * order, and it is shaped by what people mean when they type: a match at a word
 * start is worth more than one in the middle, a run of consecutive characters
 * more than scattered ones, and a short name more than a long one.
 */

/**
 * whelk_fuzzy_score:
 * @needle: the query; empty matches everything with 0.
 * @haystack: the text searched.
 *
 * Returns: the match score (higher is better), or -1 when @needle is not a
 * subsequence of @haystack.
 */
int whelk_fuzzy_score(const char *needle, const char *haystack);

/**
 * whelk_fuzzy_best:
 * @needle: the query.
 * @candidates: (array zero-terminated=1): texts to score.
 *
 * Returns: the index of the best-scoring candidate, or -1 when none matches.
 */
int whelk_fuzzy_best(const char *needle, const char *const *candidates);

#endif // WHELK_FUZZY_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_INHIBITORS_H
#define WHELK_INHIBITORS_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * INHIBITORS
 * =========================================================================
 * "Why did my screen not blank?" has an answer on a systemd machine: the
 * inhibitor list.  `systemd-inhibit --list' prints who holds what, for how
 * long, and why -- the reading this shell puts in a tooltip instead of leaving
 * the question to guesswork.
 *
 * parsing text rather than calling logind keeps this a few lines and makes it
 * testable without a system bus; the command is only ever a reading.
 */

/**
 * whelk_inhibitors_parse:
 * @text: the output of `systemd-inhibit --list --no-pager', or NULL.
 *
 * Returns: (transfer full): an array of parsed rows; empty when the command
 *   said nothing or printed only its header.  Free with g_ptr_array_unref().
 */
GPtrArray *whelk_inhibitors_parse(const char *text);

/**
 * whelk_inhibitors_describe:
 * @list: (nullable): a list from whelk_inhibitors_parse().
 *
 * Returns: (transfer full) (nullable): one line per inhibitor, ready for a
 *   tooltip; NULL when nothing is held.
 */
char *whelk_inhibitors_describe(const GPtrArray *list);

G_END_DECLS

#endif // WHELK_INHIBITORS_H

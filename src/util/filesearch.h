// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_FILESEARCH_H
#define WHELK_FILESEARCH_H

#include <glib.h>

int whelk_file_score(const char *query, const char *path,
                     gboolean is_directory);
GPtrArray *whelk_file_search_rank(const char *query, const char *output,
                                  guint limit);
char *whelk_file_search_command(const char *root, guint max_depth);
const char *const *whelk_file_search_ignored(void);

#endif // WHELK_FILESEARCH_H

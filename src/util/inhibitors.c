// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/inhibitors.h"

typedef struct {
  char *who;
  char *what;
  char *why;
  char *mode;
} WhelkInhibitor;

static void inhibitor_free(gpointer data) {
  WhelkInhibitor *inhibitor = data;

  if (!inhibitor)
    return;

  g_free(inhibitor->who);
  g_free(inhibitor->what);
  g_free(inhibitor->why);
  g_free(inhibitor->mode);
  g_free(inhibitor);
}

/* `systemd-inhibit --list' pads its columns, so a run of two or more spaces is
 * a column boundary.  A single space inside a name, a program or a reason is
 * part of that field. */
static char **split_columns(const char *line) {
  g_auto(GStrv) parts =
      g_regex_split_simple("\\s{2,}", line, 0, 0);

  if (!parts)
    return NULL;

  for (guint i = 0; parts[i]; i++) {
    char *trimmed = g_strstrip(g_strdup(parts[i]));

    g_free(parts[i]);
    parts[i] = trimmed;
  }

  return g_steal_pointer(&parts);
}

GPtrArray *whelk_inhibitors_parse(const char *text) {
  GPtrArray *list = g_ptr_array_new_with_free_func(inhibitor_free);

  if (!text || *text == '\0')
    return list;

  g_auto(GStrv) lines = g_strsplit(text, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    const char *line = lines[i];

    if (*line == '\0' || g_str_has_prefix(line, "WHO"))
      continue;

    g_auto(GStrv) columns = split_columns(line);

    if (!columns)
      continue;

    guint count = 0;

    while (columns[count])
      count++;

    /* WHO UID USER PID COMM WHAT WHY MODE.  Anything narrower is a line this
     * reader does not understand, and dropping it is better than inventing a
     * reason for a lock that is not there. */
    if (count < 8)
      continue;

    WhelkInhibitor *inhibitor = g_new0(WhelkInhibitor, 1);

    inhibitor->who = g_strdup(columns[0]);
    inhibitor->what = g_strdup(columns[5]);
    inhibitor->why = g_strdup(columns[6]);
    inhibitor->mode = g_strdup(columns[7]);

    g_ptr_array_add(list, inhibitor);
  }

  return list;
}

char *whelk_inhibitors_describe(const GPtrArray *list) {
  if (!list || list->len == 0)
    return NULL;

  GString *out = g_string_new(NULL);

  g_string_append_printf(out, "%u inhibitor%s", list->len,
                         list->len == 1 ? "" : "s");

  for (guint i = 0; i < list->len; i++) {
    const WhelkInhibitor *inhibitor = g_ptr_array_index(list, i);

    g_string_append_printf(out, "\n%s — %s: %s (%s)",
                           inhibitor->what && *inhibitor->what ? inhibitor->what
                                                              : "idle",
                           inhibitor->who && *inhibitor->who ? inhibitor->who
                                                            : "an application",
                           inhibitor->why && *inhibitor->why ? inhibitor->why
                                                            : "no reason given",
                           inhibitor->mode && *inhibitor->mode ? inhibitor->mode
                                                              : "block");
  }

  return g_string_free(out, FALSE);
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/barmonitors.h"

#include <string.h>

gboolean whelk_bar_when_matches(WhelkBarWhen when, guint monitor_count) {
  switch (when) {
  case WHELK_BAR_WHEN_SINGLE:
    return monitor_count == 1;
  case WHELK_BAR_WHEN_MULTI:
    return monitor_count > 1;
  case WHELK_BAR_WHEN_ALWAYS:
  default:
    return TRUE;
  }
}

static gboolean already_selected(const GArray *out, gint index) {
  for (guint i = 0; i < out->len; i++)
    if (g_array_index(out, gint, i) == index)
      return TRUE;

  return FALSE;
}

static void select_one(GArray *out, gint index) {
  if (!already_selected(out, index))
    g_array_append_val(out, index);
}

static gboolean connector_is(const char *const *connectors, guint index,
                             const char *wanted) {
  return connectors && connectors[index] && wanted &&
         g_strcmp0(connectors[index], wanted) == 0;
}

void whelk_bar_select_monitors(const WhelkBarConfig *bar, guint monitor_count,
                               const char *const *connectors, GArray *out) {
  if (!out)
    return;

  g_array_set_size(out, 0);

  if (!bar || !whelk_bar_when_matches(bar->when, monitor_count))
    return;

  if (bar->monitors && bar->monitors->len > 0) {
    for (guint i = 0; i < monitor_count; i++)
      for (guint name = 0; name < bar->monitors->len; name++)
        if (connector_is(connectors, i,
                         g_ptr_array_index(bar->monitors, name)))
          select_one(out, (gint)i);

    return;
  }

  const char *rule = (bar->monitor && *bar->monitor) ? bar->monitor : "all";

  if (g_strcmp0(rule, "all") == 0) {
    if (monitor_count == 0) {
      select_one(out, WHELK_MONITOR_DEFAULT);
      return;
    }

    for (guint i = 0; i < monitor_count; i++)
      select_one(out, (gint)i);

    return;
  }

  if (g_strcmp0(rule, "default") == 0) {
    select_one(out, WHELK_MONITOR_DEFAULT);
    return;
  }

  for (guint i = 0; i < monitor_count; i++)
    if (connector_is(connectors, i, rule))
      select_one(out, (gint)i);
}

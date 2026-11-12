// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"

/* =========================================================================
 * THE CALENDAR
 * =========================================================================
 * A date on a bar is asked one question a bar has no room to answer: what day
 * of the month is it.  The tooltip is the answer, laid out as the month it
 * belongs to: the name, the weekday columns, and the days under them.
 */

static char *build_calendar(GDateTime *now) {
  const int year = g_date_time_get_year(now);
  const int month = g_date_time_get_month(now);

  g_autoptr(GDateTime) first = g_date_time_new_local(year, month, 1, 0, 0, 0.0);

  if (!first)
    return g_strdup("");

  /* The last day of this month: one month on, one day back. */
  g_autoptr(GDateTime) next_month = g_date_time_add_months(first, 1);
  g_autoptr(GDateTime) last = g_date_time_add_days(next_month, -1);
  const int days = g_date_time_get_day_of_month(last);

  g_autofree char *heading = g_date_time_format(now, "%B %Y");
  GString *out = g_string_new(heading);

  g_string_append(out, "\nMo Tu We Th Fr Sa Su\n");

  /* g_date_time_get_day_of_week() is 1 (Monday) .. 7 (Sunday). */
  const int leading = g_date_time_get_day_of_week(first) - 1;

  for (int i = 0; i < leading; i++)
    g_string_append(out, "   ");

  for (int day = 1; day <= days; day++) {
    const gboolean sunday = ((leading + day - 1) % 7) == 6;

    if (day == days && !sunday)
      g_string_append_printf(out, "%2d", day);
    else
      g_string_append_printf(out, "%2d%s", day, sunday ? "\n" : " ");
  }

  if (!g_str_has_suffix(out->str, "\n"))
    g_string_append_c(out, '\n');

  return g_string_free(out, FALSE);
}

static void clock_update(WhelkModule *module) {
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  if (!now) {
    whelk_module_set_text(module, "--:--");
    return;
  }

  /* #:format is a strftime(3) pattern, not a whelk format string: it is the
   * clock's own language, and the rendered result is what the label shows. */
  const char *pattern =
      whelk_spec_str(module->spec, "format", "%a %d %b  %H:%M");
  const char *icon = whelk_spec_str(module->spec, "icon", "");
  g_autofree char *time_text = g_date_time_format(now, pattern);
  g_autofree char *date_text = g_date_time_format(now, "%A %d %B %Y");
  g_autofree char *calendar = build_calendar(now);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "time", time_text);
  whelk_values_set(values, "date", date_text);
  whelk_values_set(values, "calendar", calendar);

  whelk_module_present_format(
      module, (icon && *icon) ? "{icon} {time}" : "{time}", icon,
      whelk_spec_str(module->spec, "tooltip", NULL), values);
}

WhelkModule *whelk_clock_module_new(const WhelkModuleSpec *spec,
                                    const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("clock", spec, bar);

  whelk_module_add_label(module);
  module->update = clock_update;
  whelk_module_install(module, 5);

  return module;
}

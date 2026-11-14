// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/hostinfo.h"

#include <stdio.h>
#include <string.h>
#include <sys/statvfs.h>
#include <sys/utsname.h>

#include <gtk/gtk.h>

#include "util/session.h"

/* =========================================================================
 * THE MACHINE'S ACCOUNT OF ITSELF
 * =========================================================================
 * Read once, when a window opens, because a window that says "memory: 31.2
 * GiB" should say what memory is there now, not what it was at login.
 */

/* "processor : 0" is a core index.  A model can be a number on paper, but not
 * on a CPU: a value with nothing but digits in it is the index line. */
static gboolean all_digits(const char *text) {
  gboolean digit = FALSE;

  for (const char *p = text; *p; p++) {
    if (g_ascii_isdigit(*p))
      digit = TRUE;
    else if (!g_ascii_isspace(*p))
      return FALSE;
  }

  return digit;
}

char *whelk_host_os_name(void) {
  g_autofree char *name = g_get_os_info(G_OS_INFO_KEY_PRETTY_NAME);

  if (name && *name)
    return g_steal_pointer(&name);

  return g_strdup("GNU/Linux");
}

char *whelk_host_cpu_model_from(const char *cpuinfo) {
  /* The keys are matched case-insensitively because ARM says "Processor" and
   * x86 says "model name", which is also why the numeric per-core line has to
   * be skipped explicitly. */
  static const char *const keys[] = {"model name", "Model", "Hardware",
                                     "Processor", "cpu model", NULL};

  if (!cpuinfo)
    return NULL;

  g_auto(GStrv) lines = g_strsplit(cpuinfo, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    g_auto(GStrv) pair = g_strsplit(lines[i], ":", 2);

    if (!pair[0] || !pair[1])
      continue;

    g_strstrip(pair[0]);

    for (guint k = 0; keys[k]; k++) {
      if (g_ascii_strcasecmp(pair[0], keys[k]) != 0)
        continue;

      g_strstrip(pair[1]);

      if (*pair[1] && !all_digits(pair[1]))
        return g_strdup(pair[1]);
    }
  }

  return NULL;
}

char *whelk_host_cpu_model(void) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/cpuinfo", &contents, NULL, NULL))
    return NULL;

  return whelk_host_cpu_model_from(contents);
}

char *whelk_host_cpu_summary(void) {
  g_autofree char *model = whelk_host_cpu_model();
  const int threads = g_get_num_processors();

  if (model && *model)
    return g_strdup_printf("%s · %d thread%s", model, threads,
                           threads == 1 ? "" : "s");

  if (threads > 0)
    return g_strdup_printf("%d thread%s", threads, threads == 1 ? "" : "s");

  return NULL;
}

char *whelk_host_memory_summary_from(const char *meminfo) {
  guint64 total = 0;
  guint64 available = 0;

  if (!meminfo)
    return NULL;

  g_auto(GStrv) lines = g_strsplit(meminfo, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    guint64 kibibytes = 0;

    if (sscanf(lines[i], "MemTotal: %" G_GUINT64_FORMAT " kB", &kibibytes) ==
        1) {
      total = kibibytes;
    } else if (sscanf(lines[i], "MemAvailable: %" G_GUINT64_FORMAT " kB",
                      &kibibytes) == 1) {
      available = kibibytes;
    }
  }

  if (total == 0)
    return NULL;

  const double to_gib = 1.0 / (1024.0 * 1024.0);

  if (available > 0 && available <= total)
    return g_strdup_printf("%.1f GiB total · %.1f GiB available",
                           (double)total * to_gib,
                           (double)available * to_gib);

  return g_strdup_printf("%.1f GiB", (double)total * to_gib);
}

char *whelk_host_memory_summary(void) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/meminfo", &contents, NULL, NULL))
    return NULL;

  return whelk_host_memory_summary_from(contents);
}

char *whelk_host_disk_summary(void) {
  struct statvfs stats;

  if (statvfs("/", &stats) != 0)
    return NULL;

  const guint64 total = (guint64)stats.f_blocks * stats.f_frsize;
  const guint64 free = (guint64)stats.f_bavail * stats.f_frsize;
  const guint64 used = total > free ? total - free : 0;

  if (total == 0)
    return NULL;

  const double to_gib = 1.0 / 1073741824.0;
  const int percent = (int)((double)used * 100.0 / (double)total + 0.5);

  return g_strdup_printf("%.0f GiB free of %.0f GiB (%d%% used)",
                         (double)free * to_gib, (double)total * to_gib,
                         percent);
}

char *whelk_host_kernel_release(void) {
  struct utsname name;

  if (uname(&name) != 0)
    return NULL;

  return g_strdup_printf("%s (%s)", name.release, name.machine);
}

char *whelk_host_uptime_from(const char *uptime) {
  double seconds = 0.0;

  if (!uptime || sscanf(uptime, "%lf", &seconds) != 1)
    return NULL;

  const guint64 minutes = (guint64)(seconds / 60.0);
  const guint64 days = minutes / (60 * 24);
  const guint64 hours = (minutes / 60) % 24;
  const guint64 rest = minutes % 60;

  if (days > 0 && hours > 0)
    return g_strdup_printf("%" G_GUINT64_FORMAT " day%s, %" G_GUINT64_FORMAT
                           " hour%s",
                           days, days == 1 ? "" : "s", hours,
                           hours == 1 ? "" : "s");

  if (days > 0)
    return g_strdup_printf("%" G_GUINT64_FORMAT " day%s", days,
                           days == 1 ? "" : "s");

  if (hours > 0 && rest > 0)
    return g_strdup_printf("%" G_GUINT64_FORMAT " hour%s, %" G_GUINT64_FORMAT
                           " minute%s",
                           hours, hours == 1 ? "" : "s", rest,
                           rest == 1 ? "" : "s");

  if (hours > 0)
    return g_strdup_printf("%" G_GUINT64_FORMAT " hour%s", hours,
                           hours == 1 ? "" : "s");

  return g_strdup_printf("%" G_GUINT64_FORMAT " minute%s", minutes,
                         minutes == 1 ? "" : "s");
}

char *whelk_host_uptime_summary(void) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/uptime", &contents, NULL, NULL))
    return NULL;

  return whelk_host_uptime_from(contents);
}

char *whelk_host_session_summary(void) {
  const char *desktop = g_getenv("XDG_CURRENT_DESKTOP");
  const char *type = g_getenv("XDG_SESSION_TYPE");
  g_autofree char *name = g_strdup(whelk_session_name(whelk_session_detect()));
  GString *summary = g_string_new(name);

  /* A niri session exports XDG_CURRENT_DESKTOP=niri; printing "niri, niri"
   * tells the reader nothing twice. */
  if (desktop && *desktop) {
    g_auto(GStrv) parts = g_strsplit(desktop, ":", -1);
    gboolean named = FALSE;

    for (guint i = 0; parts[i] && !named; i++)
      named = g_ascii_strcasecmp(parts[i], name) == 0;

    if (!named)
      g_string_append_printf(summary, " · %s", desktop);
  }

  if (type && *type) {
    g_autofree char *protocol = g_strdup(type);

    protocol[0] = g_ascii_toupper(protocol[0]);
    g_string_append_printf(summary, " · %s", protocol);
  }

  return g_string_free(summary, FALSE);
}

char *whelk_host_display_summary(void) {
  GdkDisplay *display = gdk_display_get_default();

  if (!display)
    return NULL;

  GListModel *monitors = gdk_display_get_monitors(display);
  const guint count = g_list_model_get_n_items(monitors);

  if (count == 0)
    return NULL;

  GdkMonitor *monitor = g_list_model_get_item(monitors, 0);

  if (!monitor)
    return NULL;

  GdkRectangle geometry = {0, 0, 0, 0};

  gdk_monitor_get_geometry(monitor, &geometry);

  const char *connector = gdk_monitor_get_connector(monitor);
  const char *model = gdk_monitor_get_model(monitor);
  const char *label = (connector && *connector) ? connector : model;
  const double refresh = gdk_monitor_get_refresh_rate(monitor);

  g_object_unref(monitor);

  GString *summary = g_string_new(NULL);

  g_string_append_printf(summary, "%d × %d", MAX(geometry.width, 0),
                         MAX(geometry.height, 0));

  if (refresh > 0.0)
    g_string_append_printf(summary, " · %.0f Hz", refresh / 1000.0);

  if (label && *label)
    g_string_append_printf(summary, " · %s", label);

  if (count > 1)
    g_string_append_printf(summary, " · %u monitors", count);

  return g_string_free(summary, FALSE);
}

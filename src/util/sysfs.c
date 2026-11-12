// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/sysfs.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

char *whelk_sysfs_read(const char *path) {
  if (!path)
    return NULL;

  g_autofree char *contents = NULL;

  if (!g_file_get_contents(path, &contents, NULL, NULL))
    return NULL;

  return g_strdup(g_strstrip(contents));
}

char *whelk_sysfs_first_entry(const char *directory,
                             gboolean (*wanted)(const char *parent,
                                                const char *name,
                                                gpointer user_data),
                             gpointer user_data) {
  g_autoptr(GDir) dir = g_dir_open(directory, 0, NULL);

  if (!dir)
    return NULL;

  const char *name;

  while ((name = g_dir_read_name(dir)) != NULL) {
    if (!wanted || wanted(directory, name, user_data))
      return g_strdup(name);
  }

  return NULL;
}

/* =========================================================================
 * CPU
 * ========================================================================= */

gboolean whelk_cpu_sample(WhelkCpuSample *out) {
  g_autofree char *contents = whelk_sysfs_read("/proc/stat");

  if (!contents || !g_str_has_prefix(contents, "cpu "))
    return FALSE;

  /* "cpu  user nice system idle iowait irq softirq steal guest guest_nice" */
  g_auto(GStrv) fields = g_strsplit(contents, " ", 0);
  guint64 total = 0;

  for (guint i = 1; i < 9 && fields[i]; i++) {
    if (*fields[i] == '\0')
      continue;

    total += g_ascii_strtoull(fields[i], NULL, 10);
  }

  const guint64 idle = g_ascii_strtoull(fields[4] ? fields[4] : "0", NULL, 10);
  const guint64 iowait =
      g_ascii_strtoull(fields[5] ? fields[5] : "0", NULL, 10);

  out->total = total;
  out->idle = idle + iowait;
  return TRUE;
}

double whelk_cpu_usage(const WhelkCpuSample *previous,
                       const WhelkCpuSample *current) {
  if (!previous || !current)
    return 0.0;

  const guint64 total_delta = current->total - previous->total;
  const guint64 idle_delta = current->idle - previous->idle;

  if (total_delta == 0 || current->total < previous->total ||
      current->idle < previous->idle)
    return 0.0;

  const double busy = (double)(total_delta - idle_delta);
  double percent = (busy / (double)total_delta) * 100.0;

  if (percent < 0.0)
    percent = 0.0;
  if (percent > 100.0)
    percent = 100.0;

  return percent;
}

/* =========================================================================
 * MEMORY
 * ========================================================================= */

gboolean whelk_memory_read(WhelkMemoryInfo *out) {
  g_autofree char *contents = NULL;

  if (!g_file_get_contents("/proc/meminfo", &contents, NULL, NULL))
    return FALSE;

  g_auto(GStrv) lines = g_strsplit(contents, "\n", 0);
  gboolean saw_total = FALSE;

  memset(out, 0, sizeof(*out));

  for (guint i = 0; lines[i]; i++) {
    guint64 value = 0;
    g_auto(GStrv) parts = g_strsplit(lines[i], ":", 2);

    if (!parts[0] || !parts[1])
      continue;

    value = g_ascii_strtoull(g_strstrip(parts[1]), NULL, 10);

    if (g_strcmp0(parts[0], "MemTotal") == 0) {
      out->total_kb = value;
      saw_total = TRUE;
    } else if (g_strcmp0(parts[0], "MemAvailable") == 0) {
      out->available_kb = value;
    } else if (g_strcmp0(parts[0], "SwapTotal") == 0) {
      out->swap_total_kb = value;
    } else if (g_strcmp0(parts[0], "SwapFree") == 0) {
      out->swap_free_kb = value;
    }
  }

  return saw_total && out->total_kb > 0;
}

/* =========================================================================
 * BATTERY
 * ========================================================================= */

static gboolean is_battery(const char *parent, const char *name,
                           gpointer user_data) {
  (void)user_data;

  g_autofree char *type_path =
      g_build_filename(parent, name, "type", NULL);
  g_autofree char *type = whelk_sysfs_read(type_path);

  if (!type || g_strcmp0(type, "Battery") != 0)
    return FALSE;

  g_autofree char *capacity_path =
      g_build_filename(parent, name, "capacity", NULL);

  return g_file_test(capacity_path, G_FILE_TEST_EXISTS);
}

gboolean whelk_battery_read(const char *device, WhelkBatteryInfo *out) {
  const char *root = "/sys/class/power_supply";
  g_autofree char *name = NULL;

  memset(out, 0, sizeof(*out));

  if (device && *device != '\0')
    name = g_strdup(device);
  else
    name = whelk_sysfs_first_entry(root, is_battery, NULL);

  if (!name)
    return FALSE;

  g_autofree char *capacity_path =
      g_build_filename(root, name, "capacity", NULL);
  g_autofree char *capacity_text = whelk_sysfs_read(capacity_path);

  if (!capacity_text)
    return FALSE;

  g_autofree char *status_path =
      g_build_filename(root, name, "status", NULL);
  g_autofree char *status_text = whelk_sysfs_read(status_path);
  g_autofree char *present_path =
      g_build_filename(root, name, "present", NULL);
  g_autofree char *present_text = whelk_sysfs_read(present_path);

  out->name = g_strdup(name);
  out->status = g_strdup(status_text ? status_text : "Unknown");
  out->capacity = (int)g_ascii_strtoull(capacity_text, NULL, 10);
  out->present = present_text ? g_strcmp0(present_text, "1") == 0 : TRUE;

  if (out->capacity < 0)
    out->capacity = 0;
  if (out->capacity > 100)
    out->capacity = 100;

  return TRUE;
}

void whelk_battery_clear(WhelkBatteryInfo *info) {
  if (!info)
    return;

  g_free(info->name);
  g_free(info->status);
  memset(info, 0, sizeof(*info));
}

/* =========================================================================
 * BACKLIGHT
 * ========================================================================= */

static gboolean is_backlight(const char *parent, const char *name,
                             gpointer user_data) {
  (void)user_data;

  g_autofree char *max_path =
      g_build_filename(parent, name, "max_brightness", NULL);
  g_autofree char *brightness_path =
      g_build_filename(parent, name, "brightness", NULL);

  return g_file_test(max_path, G_FILE_TEST_EXISTS) &&
         g_file_test(brightness_path, G_FILE_TEST_EXISTS);
}

int whelk_backlight_percent(const char *device) {
  const char *root = "/sys/class/backlight";
  g_autofree char *name = NULL;

  if (device && *device != '\0')
    name = g_strdup(device);
  else
    name = whelk_sysfs_first_entry(root, is_backlight, NULL);

  if (!name)
    return -1;

  g_autofree char *brightness_path =
      g_build_filename(root, name, "brightness", NULL);
  g_autofree char *max_path =
      g_build_filename(root, name, "max_brightness", NULL);
  g_autofree char *brightness_text = whelk_sysfs_read(brightness_path);
  g_autofree char *max_text = whelk_sysfs_read(max_path);

  if (!brightness_text || !max_text)
    return -1;

  const double brightness = g_ascii_strtod(brightness_text, NULL);
  const double maximum = g_ascii_strtod(max_text, NULL);

  if (maximum <= 0.0)
    return -1;

  int percent = (int)((brightness / maximum) * 100.0 + 0.5);

  if (percent < 0)
    percent = 0;
  if (percent > 100)
    percent = 100;

  return percent;
}

/* =========================================================================
 * CAPS LOCK
 * ========================================================================= */

static gboolean is_capslock_led(const char *parent, const char *name,
                                gpointer user_data) {
  (void)parent;
  (void)user_data;

  return g_str_has_suffix(name, "::capslock");
}

gboolean whelk_capslock_active(void) {
  g_autofree char *name =
      whelk_sysfs_first_entry("/sys/class/leds", is_capslock_led, NULL);

  if (!name)
    return FALSE;

  g_autofree char *path =
      g_build_filename("/sys/class/leds", name, "brightness", NULL);
  g_autofree char *text = whelk_sysfs_read(path);

  return text != NULL && g_strcmp0(text, "0") != 0;
}

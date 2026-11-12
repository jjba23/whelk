// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_SYSFS_H
#define WHELK_SYSFS_H

#include <glib.h>

/* =========================================================================
 * THE MACHINE'S OWN NUMBERS
 * =========================================================================
 * CPU load, memory, battery, brightness and the Caps Lock LED all come from
 * /proc and /sys.  Reading them is the whole of a hardware module's work; this
 * file is that work, separated from the widgets so it can be read (and tested)
 * without a display.
 */

/* --- CPU ------------------------------------------------------------------ */

typedef struct {
  guint64 total; /* every field of /proc/stat's first line, summed */
  guint64 idle;  /* idle + iowait */
} WhelkCpuSample;

gboolean whelk_cpu_sample(WhelkCpuSample *out);

/**
 * whelk_cpu_usage:
 * The busy share between two samples, in percent (0..100).  0 when the two
 * samples are the same, so a first paint and a later one read the same way.
 */
double whelk_cpu_usage(const WhelkCpuSample *previous,
                       const WhelkCpuSample *current);

/* --- memory --------------------------------------------------------------- */

typedef struct {
  guint64 total_kb;
  guint64 available_kb;
  guint64 swap_total_kb;
  guint64 swap_free_kb;
} WhelkMemoryInfo;

gboolean whelk_memory_read(WhelkMemoryInfo *out);

/* --- battery -------------------------------------------------------------- */

typedef struct {
  char *name;     /* "BAT0" */
  char *status;   /* "Charging", "Discharging", "Full", "Not charging" */
  int capacity;   /* 0..100 */
  gboolean present;
} WhelkBatteryInfo;

/**
 * whelk_battery_read:
 * @device: (nullable): the pack's directory name, or NULL for the first
 *   battery /sys/class/power_supply offers.
 * @out: filled in on success; release with whelk_battery_clear().
 */
gboolean whelk_battery_read(const char *device, WhelkBatteryInfo *out);
void whelk_battery_clear(WhelkBatteryInfo *info);

/* --- backlight ------------------------------------------------------------ */

/**
 * whelk_backlight_percent:
 * @device: (nullable): the panel's directory name, or NULL for the first one.
 *
 * Returns: the brightness as 0..100, or -1 when there is no backlight.
 */
int whelk_backlight_percent(const char *device);

/* --- Caps Lock ------------------------------------------------------------ */

/* TRUE while a keyboard's Caps Lock LED is lit.  Input LEDs are the only
 * compositor-independent answer to "is Caps Lock on" that a Wayland client can
 * read, since no protocol hands a client the seat's modifier state. */
gboolean whelk_capslock_active(void);

/* --- helpers -------------------------------------------------------------- */

/* Trimmed contents of @path, or NULL.  Caller frees with g_free(). */
char *whelk_sysfs_read(const char *path);

/* First entry of @directory that satisfies @wanted (may be NULL). */
char *whelk_sysfs_first_entry(const char *directory,
                             gboolean (*wanted)(const char *parent,
                                                const char *name,
                                                gpointer user_data),
                             gpointer user_data);

#endif // WHELK_SYSFS_H

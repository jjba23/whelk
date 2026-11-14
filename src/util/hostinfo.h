// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_HOSTINFO_H
#define WHELK_HOSTINFO_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE MACHINE'S ACCOUNT OF ITSELF
 * =========================================================================
 * What an About window says: the system's name, the processor, the memory and
 * disk that are there now, the kernel, the session and the display.  Every
 * reader answers NULL when its source cannot be read, so a window leaves a row
 * out rather than print a guess.
 *
 * The parsers take their file's text instead of opening the file, because the
 * awkward cases live in the text: /proc/cpuinfo's per-core "processor : 0" line
 * is a core index rather than a model, and a memory total can arrive without
 * MemAvailable beside it.
 */

/* /etc/os-release's PRETTY_NAME, or "GNU/Linux". */
char *whelk_host_os_name(void);

/* The model in @cpuinfo, skipping the per-core "processor" index lines. */
char *whelk_host_cpu_model_from(const char *cpuinfo);

/* The same, read from /proc/cpuinfo. */
char *whelk_host_cpu_model(void);

/* The model and the thread count, e.g. "AMD Ryzen 7 · 16 threads". */
char *whelk_host_cpu_summary(void);

/* @meminfo as "30.7 GiB total · 21.0 GiB available". */
char *whelk_host_memory_summary_from(const char *meminfo);

/* The same, read from /proc/meminfo. */
char *whelk_host_memory_summary(void);

/* The root filesystem: "413 GiB free of 928 GiB (44% used)". */
char *whelk_host_disk_summary(void);

/* The kernel release and machine, e.g. "7.3.3-1-default (x86_64)". */
char *whelk_host_kernel_release(void);

/* @uptime in seconds, as a sentence that leaves zero parts out. */
char *whelk_host_uptime_from(const char *uptime);

/* The same, read from /proc/uptime. */
char *whelk_host_uptime_summary(void);

/* The session and its protocol, e.g. "niri · Wayland".  A desktop name the
 * session already carries is not repeated. */
char *whelk_host_session_summary(void);

/* The first monitor, e.g. "2560 × 1600 · 60 Hz · eDP-1", with the monitor
 * count appended when there is more than one. */
char *whelk_host_display_summary(void);

G_END_DECLS

#endif // WHELK_HOSTINFO_H

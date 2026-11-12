// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"
#include "util/sysfs.h"

/* =========================================================================
 * MEMORY
 * =========================================================================
 * /proc/meminfo is the machine's own answer, and it distinguishes "used" from
 * "available" -- which is the distinction a memory readout is for: caches are
 * used and are not in the way.
 */

static void memory_update(WhelkModule *module) {
  WhelkMemoryInfo info;

  if (!whelk_memory_read(&info)) {
    whelk_module_set_text(module, "—");
    return;
  }

  const guint64 used_kb =
      info.total_kb > info.available_kb ? info.total_kb - info.available_kb : 0;
  const guint64 swap_used_kb = info.swap_total_kb > info.swap_free_kb
                                   ? info.swap_total_kb - info.swap_free_kb
                                   : 0;

  const int percentage =
      info.total_kb > 0 ? (int)((used_kb * 100 + info.total_kb / 2) /
                                info.total_kb)
                        : 0;

  g_autofree char *percent = g_strdup_printf("%d", percentage);
  g_autofree char *used = whelk_str_gib(used_kb);
  g_autofree char *total = whelk_str_gib(info.total_kb);
  g_autofree char *available = whelk_str_gib(info.available_kb);
  g_autofree char *swap_used = whelk_str_gib(swap_used_kb);
  g_autofree char *swap_total = whelk_str_gib(info.swap_total_kb);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "percentage", percent);
  whelk_values_set(values, "used", used);
  whelk_values_set(values, "total", total);
  whelk_values_set(values, "avail", available);
  whelk_values_set(values, "swap-used", swap_used);
  whelk_values_set(values, "swap-total", swap_total);

  whelk_module_present(module, "{icon} {percentage}%", values);
}

WhelkModule *whelk_memory_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("memory", spec, bar);

  whelk_module_add_label(module);
  module->update = memory_update;
  whelk_module_install(module, 5);

  return module;
}

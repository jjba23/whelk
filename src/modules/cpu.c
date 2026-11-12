// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/sysfs.h"

/* =========================================================================
 * CPU
 * =========================================================================
 * /proc/stat counts jiffies, so a percentage is a difference between two
 * samples rather than a reading: the module keeps the last sample and reports
 * what happened between them.
 */

typedef struct {
  WhelkCpuSample previous;
  gboolean have_previous;
} CpuState;

static void cpu_update(WhelkModule *module) {
  CpuState *state = module->data;
  WhelkCpuSample current;

  if (!whelk_cpu_sample(&current)) {
    whelk_module_set_text(module, "—");
    return;
  }

  const double usage = state->have_previous
                           ? whelk_cpu_usage(&state->previous, &current)
                           : 0.0;

  state->previous = current;
  state->have_previous = TRUE;

  g_autofree char *percent = g_strdup_printf("%d", (int)(usage + 0.5));

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "usage", percent);

  whelk_module_present(module, "{icon} {usage}%", values);
}

static void cpu_destroy(WhelkModule *module) { g_clear_pointer(&module->data, g_free); }

WhelkModule *whelk_cpu_module_new(const WhelkModuleSpec *spec,
                                  const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("cpu", spec, bar);
  CpuState *state = g_new0(CpuState, 1);

  /* A sample now means the first paint two seconds later is a real interval
   * rather than a division by zero. */
  state->have_previous = whelk_cpu_sample(&state->previous);

  module->data = state;
  module->destroy = cpu_destroy;

  whelk_module_add_label(module);
  module->update = cpu_update;
  whelk_module_install(module, 2);

  return module;
}

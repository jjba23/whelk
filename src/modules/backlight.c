// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/sysfs.h"

/* =========================================================================
 * BACKLIGHT
 * =========================================================================
 * The panel's own brightness, from /sys/class/backlight.  A machine without a
 * backlight has no readout rather than a zero: the module hides itself, which is
 * what a desktop wants from a laptop's indicator.
 */

typedef struct {
  char *device;
} BacklightState;

static void backlight_update(WhelkModule *module) {
  BacklightState *state = module->data;
  const int percent = whelk_backlight_percent(state->device);

  if (percent < 0) {
    whelk_module_set_text(module, "");
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  g_autofree char *text = g_strdup_printf("%d", percent);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "percent", text);

  whelk_module_present(module, "{icon} {percent}%", values);
}

static void backlight_destroy(WhelkModule *module) {
  BacklightState *state = module->data;

  if (state) {
    g_free(state->device);
    g_free(state);
  }

  module->data = NULL;
}

WhelkModule *whelk_backlight_module_new(const WhelkModuleSpec *spec,
                                        const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("backlight", spec, bar);
  BacklightState *state = g_new0(BacklightState, 1);

  const char *device = whelk_spec_str(spec, "device", NULL);
  state->device = (device && *device) ? g_strdup(device) : NULL;

  module->data = state;
  module->destroy = backlight_destroy;

  whelk_module_add_label(module);
  module->update = backlight_update;
  whelk_module_install(module, 5);

  return module;
}

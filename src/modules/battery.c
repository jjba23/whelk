// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <string.h>

#include "util/sysfs.h"

/* =========================================================================
 * BATTERY
 * =========================================================================
 * One pack's capacity and state, with the two thresholds that change its
 * colour.  "Nearly flat" is not a theme, so the classes the module sets
 * (.warning, .critical, .charging) are painted by the structural stylesheet,
 * and a machine can repaint them through the bar's own #:css hook.
 */

typedef struct {
  char *device;
} BatteryState;

static gboolean status_means_charging(const char *status) {
  return g_strcmp0(status, "Charging") == 0 ||
         g_strcmp0(status, "Full") == 0 ||
         g_strcmp0(status, "Not charging") == 0;
}

static void battery_update(WhelkModule *module) {
  BatteryState *state = module->data;
  WhelkBatteryInfo info;

  if (!whelk_battery_read(state->device, &info) || !info.present) {
    whelk_battery_clear(&info);
    whelk_module_set_text(module, "");
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  const int warning = whelk_spec_int(module->spec, "warning", 30);
  const int critical = whelk_spec_int(module->spec, "critical", 15);
  const gboolean charging = status_means_charging(info.status);

  whelk_module_remove_class(module, "warning");
  whelk_module_remove_class(module, "critical");
  whelk_module_remove_class(module, "charging");

  if (info.capacity <= critical)
    whelk_module_add_class(module, "critical");
  else if (info.capacity <= warning)
    whelk_module_add_class(module, "warning");

  if (charging)
    whelk_module_add_class(module, "charging");

  g_autofree char *capacity = g_strdup_printf("%d", info.capacity);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "capacity", capacity);
  whelk_values_set(values, "state", info.status);
  whelk_values_set(values, "device", info.name);

  const char *charging_format =
      whelk_spec_str(module->spec, "charging-format", NULL);
  const char *format = whelk_spec_str(module->spec, "format",
                                      "{icon} {capacity}%");
  const char *icon = (charging && whelk_spec_str(module->spec, "charging-icon",
                                                 NULL))
                         ? whelk_spec_str(module->spec, "charging-icon", "")
                         : whelk_spec_str(module->spec, "icon", "");

  if (charging && charging_format)
    format = charging_format;

  whelk_module_present_format(module, format, icon,
                              whelk_spec_str(module->spec, "tooltip", NULL),
                              values);

  whelk_battery_clear(&info);
}

static void battery_destroy(WhelkModule *module) {
  BatteryState *state = module->data;

  if (state) {
    g_free(state->device);
    g_free(state);
  }

  module->data = NULL;
}

WhelkModule *whelk_battery_module_new(const WhelkModuleSpec *spec,
                                      const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("battery", spec, bar);
  BatteryState *state = g_new0(BatteryState, 1);

  const char *device = whelk_spec_str(spec, "device", NULL);
  state->device = (device && *device) ? g_strdup(device) : NULL;

  module->data = state;
  module->destroy = battery_destroy;

  whelk_module_add_label(module);
  module->update = battery_update;
  whelk_module_install(module, 30);

  return module;
}

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
 *
 * A charging pack is deliberately quiet: the bolt keeps the bar's own ink
 * unless `#:charging-color` names a tint, and the percentage is only shown
 * once the pack is running down, unless `#:show-capacity-when-charging?` asks
 * for it.  The capacity stays in the tooltip either way.
 */

typedef struct {
  char *device;
  char *tint_class;      /* the class #:charging-color paints, when set */
  GtkCssProvider *tint;  /* the provider holding that one rule */
  GdkDisplay *tint_display;
} BatteryState;

static gboolean status_means_charging(const char *status) {
  return g_strcmp0(status, "Charging") == 0 ||
         g_strcmp0(status, "Full") == 0 ||
         g_strcmp0(status, "Not charging") == 0;
}

/* `#:charging-color` is the one colour a charging pack may wear.  Without it
 * the bolt inherits the bar's ink -- white on a dark bar, black on a light
 * one -- because a pack that is filling is a reading, not an alarm.
 *
 * A widget has no provider of its own in GTK 4, so the tint is one rule on the
 * display, scoped to a class this module owns and removes with itself. */
static void apply_charging_tint(WhelkModule *module, BatteryState *state,
                                const char *color) {
  if (!module || !state || !color || *color == '\0')
    return;

  state->tint_display = gdk_display_get_default();

  if (!state->tint_display)
    return;

  g_object_ref(state->tint_display);
  state->tint_class = g_strdup_printf("whelk-battery-tint-%lx",
                                      (gulong)(guintptr)module);

  g_autofree char *css =
      g_strdup_printf(".%s.whelk-battery.charging { color: %s; }",
                      state->tint_class, color);

  state->tint = gtk_css_provider_new();
  gtk_css_provider_load_from_string(state->tint, css);
  gtk_style_context_add_provider_for_display(
      state->tint_display, GTK_STYLE_PROVIDER(state->tint),
      GTK_STYLE_PROVIDER_PRIORITY_USER);
  gtk_widget_add_css_class(module->root, state->tint_class);
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

  /* A pack that is filling is not an alarm: while charging the bolt keeps the
   * bar's ink instead of the warning and critical colours, which stay for a
   * battery that is running down. */
  if (charging)
    whelk_module_add_class(module, "charging");
  else if (info.capacity <= critical)
    whelk_module_add_class(module, "critical");
  else if (info.capacity <= warning)
    whelk_module_add_class(module, "warning");

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

  if (charging && charging_format) {
    /* An explicit format is the machine's decision, numbers included. */
    format = charging_format;
  } else if (charging &&
             !whelk_spec_bool(module->spec, "show-capacity-when-charging?",
                              FALSE)) {
    /* Otherwise the bolt says it and the capacity lives in the tooltip. */
    format = "{icon}";
  }

  whelk_module_present_format(module, format, icon,
                              whelk_spec_str(module->spec, "tooltip", NULL),
                              values);

  whelk_battery_clear(&info);
}

static void battery_destroy(WhelkModule *module) {
  BatteryState *state = module->data;

  if (state) {
    if (state->tint && state->tint_display)
      gtk_style_context_remove_provider_for_display(
          state->tint_display, GTK_STYLE_PROVIDER(state->tint));

    g_clear_object(&state->tint);
    g_clear_object(&state->tint_display);
    g_free(state->tint_class);
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

  apply_charging_tint(module, state,
                      whelk_spec_str(spec, "charging-color", NULL));

  whelk_module_add_label(module);
  module->update = battery_update;
  whelk_module_install(module, 30);

  return module;
}

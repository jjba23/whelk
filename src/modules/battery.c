// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "style.h"
#include "util/sysfs.h"

/* =========================================================================
 * BATTERY
 * =========================================================================
 * One pack's capacity and state, with the two thresholds that change its
 * colour.  Those classes (.warning, .critical, .charging) are painted by the
 * structural stylesheet, so a machine can repaint them through #:css.
 *
 * Only a pack that is filling is quiet: the bolt keeps the bar's ink unless
 * `#:charging-color` names a tint, and the percentage is hidden unless
 * `#:show-capacity-when-charging?` asks for it.  A full pack, or one that is
 * plugged in but not taking power, still shows its capacity and keeps the
 * warning and critical colours.  The tooltip keeps the capacity either way.
 */

typedef struct {
  char *device;
  GtkCssProvider *tint; /* the #:charging-color rule, when set */
} BatteryState;

/* A widget has no provider of its own in GTK 4, so the tint is one rule on
 * the display, scoped to a class on this module's root.  The class needs no
 * cleanup: it dies with the widget, as the provider does with the module. */
static void apply_charging_tint(WhelkModule *module, BatteryState *state,
                                const char *color) {
  static guint serial;

  if (!color || !*color || !gdk_display_get_default())
    return;

  g_autofree char *class_name =
      g_strdup_printf("whelk-battery-tint-%u", ++serial);
  g_autofree char *css =
      g_strdup_printf(".%s.charging { color: %s; }", class_name, color);

  state->tint = gtk_css_provider_new();
  gtk_css_provider_load_from_string(state->tint, css);
  gtk_style_context_add_provider_for_display(
      gdk_display_get_default(), GTK_STYLE_PROVIDER(state->tint),
      GTK_STYLE_PROVIDER_PRIORITY_USER);
  gtk_widget_add_css_class(module->root, class_name);
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
  const gboolean charging = whelk_battery_is_charging(info.status);

  whelk_module_remove_class(module, "warning");
  whelk_module_remove_class(module, "critical");
  whelk_module_remove_class(module, "charging");

  /* Only a pack that is filling outranks the warning and critical colours. */
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

  const char *charging_icon =
      whelk_spec_str(module->spec, "charging-icon", NULL);
  const char *format = whelk_spec_str(module->spec, "format",
                                      "{icon} {capacity}%");
  const char *icon = whelk_spec_str(module->spec, "icon", "");

  if (charging && charging_icon)
    icon = charging_icon;

  if (charging) {
    const char *charging_format =
        whelk_spec_str(module->spec, "charging-format", NULL);

    if (charging_format)
      format = charging_format;
    else if (!whelk_spec_bool(module->spec, "show-capacity-when-charging?",
                              FALSE))
      format = "{icon}";
  }

  whelk_module_present_format(module, format, icon,
                              whelk_spec_str(module->spec, "tooltip", NULL),
                              values);

  whelk_battery_clear(&info);
}

static void battery_destroy(WhelkModule *module) {
  BatteryState *state = module->data;

  if (!state)
    return;

  whelk_style_detach(state->tint, gdk_display_get_default());
  g_free(state->device);
  g_free(state);
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

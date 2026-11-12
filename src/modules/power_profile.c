// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"

/* =========================================================================
 * POWER PROFILE
 * =========================================================================
 * power-profiles-daemon's active profile, read with its own CLI.  A machine
 * without the daemon answers nothing and the module hides itself: an indicator
 * for a profile that cannot be chosen is a decoration.
 */

static void on_profile(WhelkModule *module, const char *text, gpointer data) {
  (void)data;

  g_autofree char *profile = whelk_str_first_line(text);

  if (!profile || *profile == '\0') {
    gtk_widget_set_visible(module->root, FALSE);
    whelk_module_set_text(module, "");
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "profile", profile);

  whelk_module_present(module, "{profile}", values);
}

static void power_profile_update(WhelkModule *module) {
  whelk_module_command(module, "powerprofilesctl get", on_profile, NULL);
}

WhelkModule *whelk_power_profile_module_new(const WhelkModuleSpec *spec,
                                            const WhelkBarConfig *bar) {
  WhelkModule *module =
      whelk_module_base_new("power-profile", spec, bar);

  whelk_module_add_label(module);
  module->update = power_profile_update;
  whelk_module_install(module, 5);

  return module;
}

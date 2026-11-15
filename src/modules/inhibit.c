// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "logger.h"
#include "util/inhibitors.h"

/* =========================================================================
 * INHIBITORS
 * =========================================================================
 * The shell owns the idle policy: it listens to logind's IdleHint and locks
 * when the seat goes idle.  This module is the other half of that answer --
 * which process is holding an inhibitor, and why -- so "the screen did not
 * blank" has a name attached to it.
 *
 * It hides when nothing is held, unless #:show-off? keeps the tile visible.
 */

static void on_inhibitors(WhelkModule *module, const char *text,
                          gpointer user_data) {
  (void)user_data;

  g_autoptr(GPtrArray) list = whelk_inhibitors_parse(text);
  g_autofree char *summary = whelk_inhibitors_describe(list);
  const guint count = list ? list->len : 0;
  const gboolean show_off =
      whelk_spec_bool(module->spec, "show-off?", FALSE);

  if (count == 0 && !show_off) {
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  g_autofree char *number = g_strdup_printf("%u", count);
  GHashTable *values = whelk_values_new();

  whelk_values_set(values, "count", number);
  whelk_module_present(module,
                       whelk_spec_str(module->spec, "format", "{icon} {count}"),
                       values);

  gtk_widget_set_tooltip_text(
      module->root,
      summary ? summary
              : "No inhibitor is held: idle policy can blank the screen");
}

static void inhibit_update(WhelkModule *module) {
  whelk_module_command(module, "systemd-inhibit --list --no-pager 2>/dev/null",
                       on_inhibitors, NULL);
}

WhelkModule *whelk_inhibit_module_new(const WhelkModuleSpec *spec,
                                      const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("inhibit", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-inhibit");
  whelk_module_add_label(module);
  module->update = inhibit_update;
  whelk_module_install(module, 10);

  return module;
}

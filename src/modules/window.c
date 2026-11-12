// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"
#include "util/json.h"

/* =========================================================================
 * THE FOCUSED WINDOW
 * =========================================================================
 * What is in front of you, named the way a menu bar names it.  niri reports the
 * title and the application id; the format decides which of them the bar says,
 * the tooltip can say both, and #:max-length keeps a long title from eating the
 * row.
 */

static void on_focused_window(WhelkModule *module, const char *text,
                              gpointer data) {
  (void)data;

  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(text, &error);

  if (!root) {
    whelk_module_set_text(module, "");
    return;
  }

  const char *title = whelk_json_object_string(root, "title");
  const char *app_id = whelk_json_object_string(root, "app_id");
  const int max_length = whelk_spec_int(module->spec, "max-length", 50);

  g_autofree char *shortened =
      whelk_str_truncate_utf8(title ? title : "", max_length);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "title", shortened);
  whelk_values_set(values, "app-id", app_id ? app_id : "");

  whelk_json_free(root);

  whelk_module_present(module, "{title}", values);
}

static void window_update(WhelkModule *module) {
  whelk_module_command(module, "niri msg -j focused-window", on_focused_window,
                       NULL);
}

WhelkModule *whelk_window_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("window", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-window");
  whelk_module_add_label(module);
  gtk_label_set_ellipsize(GTK_LABEL(module->label), PANGO_ELLIPSIZE_END);
  gtk_label_set_xalign(GTK_LABEL(module->label), 0.0);

  module->update = window_update;
  whelk_module_install(module, 1);

  return module;
}

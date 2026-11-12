// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"
#include "util/session.h"

/* =========================================================================
 * THE FOCUSED WINDOW
 * =========================================================================
 * What is in front of you, named the way a menu bar names it.  The session
 * answers -- niri's focused window, sway's tree, xprop on an X11 desktop -- and
 * its parser hands back the same two strings either way; the format decides
 * which of them the bar says, the tooltip can say both, and #:max-length keeps
 * a long title from eating the row.  A session with no answer (GNOME's Wayland
 * session) leaves the module empty instead of printing an error.
 */

static void on_focused_window(WhelkModule *module, const char *text,
                              gpointer data) {
  (void)data;

  char *title = NULL;
  char *app_id = NULL;

  if (!whelk_session_parse_focused(whelk_session_detect(), text, &title,
                                   &app_id)) {
    whelk_module_set_text(module, "");
    return;
  }

  const int max_length = whelk_spec_int(module->spec, "max-length", 50);
  g_autofree char *shortened =
      whelk_str_truncate_utf8(title, max_length);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "title", shortened);
  whelk_values_set(values, "app-id", app_id);

  whelk_module_present(module, "{title}", values);

  g_free(title);
  g_free(app_id);
}

static void window_update(WhelkModule *module) {
  g_autofree char *command =
      whelk_session_focused_window_command(whelk_session_detect());

  if (!command) {
    whelk_module_set_text(module, "");
    return;
  }

  whelk_module_command(module, command, on_focused_window, NULL);
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

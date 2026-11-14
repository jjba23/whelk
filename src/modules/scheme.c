// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "guile_bridge.h"
#include "logger.h"

/* =========================================================================
 * SCHEME
 * =========================================================================
 * The module that is a procedure.  `(scheme-module #:update PROC)' stores PROC
 * in the Scheme-side registry and hands this factory a token; on every refresh
 * the bridge calls PROC and gives back the widget it built.  The procedure runs
 * on the GTK thread, so it can touch GTK through the whelk-widget primitives,
 * but it can also read a file, a socket or an API -- from the bar's point of
 * view those are all just a reading that returns a widget.
 *
 * A failed or empty reading leaves the previous widget alone.  That is the same
 * contract a command-backed module has: a slow program does not erase the
 * last value it printed.
 */

static void scheme_update(WhelkModule *module) {
  const char *token = whelk_spec_str(module->spec, "update", NULL);

  if (!token || *token == '\0') {
    if (gtk_widget_get_first_child(module->root) == NULL) {
      GtkWidget *label = whelk_module_add_label(module);

      gtk_label_set_text(GTK_LABEL(label), "scheme: #:update is missing");
      LOG_C_WARN("Scheme", "scheme-module has no #:update procedure");
    }

    return;
  }

  GtkWidget *widget = whelk_bridge_call_module_widget(token);

  if (!widget)
    return;

  while (gtk_widget_get_first_child(module->root)) {
    GtkWidget *child = gtk_widget_get_first_child(module->root);

    gtk_widget_unparent(child);
  }

  module->label = NULL;
  gtk_widget_set_valign(widget, GTK_ALIGN_CENTER);
  gtk_widget_add_css_class(widget, "whelk-scheme-widget");
  gtk_box_append(GTK_BOX(module->root), widget);
  g_object_unref(widget);
  whelk_module_refresh_tooltip(module);
}

WhelkModule *whelk_scheme_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("scheme", spec, bar);

  gtk_widget_add_css_class(module->root, "whelk-scheme");
  module->update = scheme_update;
  whelk_module_install(module, 1);

  return module;
}

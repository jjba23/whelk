// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/format.h"

/* =========================================================================
 * CUSTOM
 * =========================================================================
 * The workhorse: a reading produced by a command.  Everything a Wayland bar
 * cannot ask a protocol for -- now playing, the clipboard, do-not-disturb, a
 * machine's own scripts -- is a custom module, and #:interval 0 makes it a
 * static label for the glyphs that only exist to be clicked.
 *
 * The reading is the command's first line of standard output, so a script can
 * print a paragraph and have the bar show its headline.
 */

static void on_custom_output(WhelkModule *module, const char *text,
                             gpointer data) {
  (void)data;

  g_autofree char *line = whelk_str_first_line(text);
  const int max_length = whelk_spec_int(module->spec, "max-length", 0);
  g_autofree char *shortened = whelk_str_truncate_utf8(line, max_length);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "text", shortened);

  whelk_module_present(module, "{text}", values);
}

static void custom_update(WhelkModule *module) {
  const char *command = whelk_spec_str(module->spec, "exec", NULL);

  if (!command || *command == '\0') {
    whelk_module_set_text(module, "");
    return;
  }

  whelk_module_command(module, command, on_custom_output, NULL);
}

WhelkModule *whelk_custom_module_new(const WhelkModuleSpec *spec,
                                     const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("custom", spec, bar);

  whelk_module_add_label(module);

  /* A long custom reading is truncated by the module, not by the bar. */
  const int max_length = whelk_spec_int(spec, "max-length", 0);

  if (max_length > 0)
    gtk_label_set_ellipsize(GTK_LABEL(module->label), PANGO_ELLIPSIZE_END);

  module->update = custom_update;
  whelk_module_install(module, 5);

  return module;
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include <string.h>

#include "util/format.h"

/* =========================================================================
 * KEYBOARD LAYOUT
 * =========================================================================
 * niri reports its layouts by display name ("English (US)"), which is a
 * sentence, and a bar wants two letters.  Its plain-text listing marks the
 * active layout with an asterisk; the configuration names the XKB codes in the
 * same order, and the module reads the mark, not the name -- the JSON listing
 * does not say which layout is in use.
 *
 *   Keyboard layouts: "English (US)", "Dutch"
 *    * 0 English (US)
 *      1 Dutch
 */

static void on_layouts(WhelkModule *module, const char *text, gpointer data) {
  (void)data;

  int active_index = -1;
  g_autofree char *active_name = NULL;

  if (text) {
    g_auto(GStrv) lines = g_strsplit(text, "\n", 0);

    for (guint i = 0; lines[i]; i++) {
      const char *line = g_strstrip(lines[i]);
      const char *cursor = line;

      /* Skip the spaces an unselected row is indented with. */
      while (*cursor == ' ')
        cursor++;

      if (*cursor != '*')
        continue;

      cursor++;
      while (*cursor == ' ')
        cursor++;

      active_index = (int)g_ascii_strtoull(cursor, NULL, 10);

      while (*cursor != '\0' && *cursor != ' ')
        cursor++;
      while (*cursor == ' ')
        cursor++;

      g_free(active_name);
      active_name = g_strdup(cursor);
      break;
    }
  }

  if (active_index < 0) {
    whelk_module_set_text(module, "");
    return;
  }

  g_autofree char *code = NULL;

  /* The codes are the XKB ones, in niri's order. */
  const char *codes = whelk_spec_str(module->spec, "codes", NULL);

  if (codes && *codes) {
    g_auto(GStrv) split = g_strsplit(codes, ",", 0);

    if ((guint)active_index < g_strv_length(split) &&
        split[active_index] && *split[active_index])
      code = g_ascii_strup(split[active_index], -1);
  }

  if (!code)
    code = g_strdup(active_name ? active_name : "");

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "text", code);
  whelk_values_set(values, "name", active_name ? active_name : code);
  whelk_values_set(values, "index", "");

  whelk_module_present(module, "{text}", values);
}

static void language_update(WhelkModule *module) {
  whelk_module_command(module, "niri msg keyboard-layouts", on_layouts, NULL);
}

WhelkModule *whelk_language_module_new(const WhelkModuleSpec *spec,
                                       const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("language", spec, bar);

  whelk_module_add_label(module);
  module->update = language_update;
  whelk_module_install(module, 3);

  return module;
}

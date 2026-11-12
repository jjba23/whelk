// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "modules/modules.h"

#include "util/sysfs.h"

/* =========================================================================
 * CAPS LOCK
 * =========================================================================
 * Caps Lock is the key whose state nobody can see, and the one letter case a
 * bar can still warn about.  No Wayland protocol hands a client the seat's
 * modifier state, but every keyboard's Caps Lock LED is readable in sysfs, and
 * that LED is the user's own answer.
 *
 * The reading is empty while the key is unlocked, which collapses the module:
 * a bar that spends width saying "nothing is locked" is a bar with a hole in
 * it.  The glyph is the keyboard's own caps-lock symbol.
 */

static void keyboard_update(WhelkModule *module) {
  const gboolean locked = whelk_capslock_active();

  if (!locked) {
    whelk_module_set_text(module, "");
    gtk_widget_set_visible(module->root, FALSE);
    return;
  }

  gtk_widget_set_visible(module->root, TRUE);

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "state", "on");

  whelk_module_add_class(module, "locked");
  whelk_module_present(module, "⇪", values);
}

WhelkModule *whelk_keyboard_module_new(const WhelkModuleSpec *spec,
                                       const WhelkBarConfig *bar) {
  WhelkModule *module = whelk_module_base_new("keyboard", spec, bar);

  whelk_module_add_label(module);
  module->update = keyboard_update;
  whelk_module_install(module, 1);

  return module;
}

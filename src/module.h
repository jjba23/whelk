// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MODULE_H
#define WHELK_MODULE_H

#include "config.h"

#include <gtk/gtk.h>

/* =========================================================================
 * THE MODULE CONTRACT
 * =========================================================================
 * A module is a reading plus the actions on it: a GTK widget, an `update`
 * function that refreshes what it says, and a `destroy` that releases whatever
 * the reading needed.  There is no module subclass and no signal: a module is
 * a value, the way an item in a status bar is, and the registry below turns the
 * configuration's kind symbol into one.
 *
 * Adding a module is therefore three things: a .c file with a factory, its
 * prototype in modules/modules.h, and one row in the factory table in
 * module.c.  Nothing else in the shell changes, which is the point.
 */

typedef struct _WhelkModule WhelkModule;

/**
 * WhelkModuleFactory:
 * @spec: the configuration of this module (borrowed).
 * @bar: the bar it belongs to, for fonts and colours (borrowed).
 *
 * Returns: (transfer full) (nullable): the built module.
 */
typedef WhelkModule *(*WhelkModuleFactory)(const WhelkModuleSpec *spec,
                                           const WhelkBarConfig *bar);

typedef struct {
  const char *kind;
  WhelkModuleFactory create;
} WhelkModuleEntry;

/**
 * WhelkModule:
 * @kind: the factory name.
 * @root: the widget the bar inserts.
 * @label: (nullable): the reading's label, when the module has a single one.
 * @bar: (nullable): the bar this module was built for (borrowed).
 * @spec: the configuration (borrowed; outlives the module).
 * @timer_id: the refresh source, 0 when the module has none.
 * @busy: TRUE while a command is in flight, so a slow program cannot stack
 *   up polls.
 * @text: the last plain reading, for tooltips that say "{text}".
 * @data: the module's own state.
 * @guard: (nullable): the liveness token an in-flight command holds, so a
 *   program that answers after its module is gone cannot touch freed memory.
 * @update: (nullable): refresh the reading.
 * @destroy: (nullable): release @data.
 *
 * One module, alive.
 */
struct _WhelkModule {
  char *kind;
  GtkWidget *root;
  GtkWidget *label;
  const WhelkBarConfig *bar;
  const WhelkModuleSpec *spec;
  guint timer_id;
  gboolean busy;
  char *text;
  gpointer data;
  gpointer guard;
  void (*update)(WhelkModule *module);
  void (*destroy)(WhelkModule *module);
};

/* --- lifecycle ------------------------------------------------------------ */

/**
 * whelk_module_new:
 * @spec: the module's configuration.
 * @bar: the bar it belongs to.
 *
 * Returns: (transfer full) (nullable): a module, or NULL when no factory
 * claims the kind (a message is logged; the rest of the bar still builds).
 */
WhelkModule *whelk_module_new(const WhelkModuleSpec *spec,
                              const WhelkBarConfig *bar);

/* Stops the refresh source and runs the module's own cleanup. */
void whelk_module_free(WhelkModule *module);

/**
 * whelk_module_base_new:
 * @kind: the module kind, also its CSS class suffix.
 * @spec: the configuration (borrowed).
 * @bar: the bar (borrowed).
 *
 * The skeleton every factory starts from: a box wearing `.whelk-module` and
 * `.whelk-<kind>`, with `spacing` between its children.
 */
WhelkModule *whelk_module_base_new(const char *kind, const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar);

/**
 * whelk_module_add_label:
 * Adds the module's single label to its box and returns it.
 */
GtkWidget *whelk_module_add_label(WhelkModule *module);

/**
 * whelk_module_install:
 * @module: a module whose @update is set.
 * @fallback_interval: seconds to refresh at when the spec has no `interval`.
 *
 * Attaches the configured clicks, paints once, and starts the refresh timer.
 * An interval of 0 means "once": a label that never changes is not polled.
 */
void whelk_module_install(WhelkModule *module, guint fallback_interval);

/* --- readings ------------------------------------------------------------- */

/**
 * whelk_module_present:
 * @module: the module.
 * @default_format: the format to use when the spec has none.
 * @values: (transfer full) (element-type utf8 utf8): the reading's plain
 *   values, keyed by the name a format writes in braces.  Consumed.
 *
 * Renders the spec's `format` into the label (escaping every value, drawing
 * `{icon}` from the bar's icon font) and the spec's `tooltip` into the hover
 * text, where `{text}` is the rendered reading.  This is the one place a module
 * turns numbers into words, so every module's text behaves the same.
 */
void whelk_module_present(WhelkModule *module, const char *default_format,
                          GHashTable *values);

/**
 * whelk_module_present_format:
 * @module: the module.
 * @format: the format string to render (may come from an option of its own).
 * @icon: (nullable): the glyph to draw for {icon}.
 * @tooltip: (nullable): the hover format, where {text} is the reading.
 * @values: (transfer full): the reading's plain values.  Consumed.
 *
 * The rendering step of whelk_module_present(), with the format and icon named
 * by the caller.  A module whose reading changes shape -- a battery that is
 * charging, a sink that is muted -- picks its format and calls this, so the
 * escaping and the tooltip still happen in one place.
 */
void whelk_module_present_format(WhelkModule *module, const char *format,
                                 const char *icon, const char *tooltip,
                                 GHashTable *values);

/* An empty value table, ready for whelk_module_present(). */
GHashTable *whelk_values_new(void);

/* Adds one plain value; the table owns its copies. */
void whelk_values_set(GHashTable *values, const char *key, const char *value);

/* Same, for a number rendered as an integer. */
void whelk_values_set_int(GHashTable *values, const char *key, int value);

/* Sets plain text without a format (used by labels and multi-widget modules). */
void whelk_module_set_text(WhelkModule *module, const char *text);

/* Sets markup the module built itself (workspaces, tray). */
void whelk_module_set_markup(WhelkModule *module, const char *markup);

/* Applies the spec's `tooltip`, if any, with {text} as the reading. */
void whelk_module_refresh_tooltip(WhelkModule *module);

/* Adds/removes one CSS class on the module's root. */
void whelk_module_add_class(WhelkModule *module, const char *css_class);
void whelk_module_remove_class(WhelkModule *module, const char *css_class);

/* --- async helpers -------------------------------------------------------- */

/**
 * whelk_module_command:
 * @module: the module that is polling.
 * @command: the shell command.
 * @callback: called with the first line of output, on the main loop.
 * @user_data: passed through.
 *
 * Runs @command unless a previous run is still in flight, so a program that
 * takes longer than the refresh interval cannot pile up.
 */
void whelk_module_command(WhelkModule *module, const char *command,
                          void (*callback)(WhelkModule *module,
                                           const char *text, gpointer data),
                          gpointer user_data);

#endif // WHELK_MODULE_H

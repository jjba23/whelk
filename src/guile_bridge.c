// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"

#include <libguile.h>
#include <stdlib.h>
#include <string.h>

#include "logger.h"

/* =========================================================================
 * CALLING INTO SCHEME
 * ========================================================================= */

/* Looks up an exported procedure.  A missing module or binding is a warning
 * rather than an abort: the C defaults are a working shell. */
static SCM public_proc(const char *module_name, const char *name) {
  SCM proc = scm_c_public_ref(module_name, name);

  if (!scm_is_true(proc) || !scm_is_true(scm_procedure_p(proc))) {
    LOG_C_DEBUG("Guile", "(%s) has no procedure %s", module_name, name);
    return SCM_BOOL_F;
  }

  return proc;
}

static SCM call0(const char *module_name, const char *name) {
  SCM proc = public_proc(module_name, name);

  return scm_is_true(proc) ? scm_call_0(proc) : SCM_BOOL_F;
}

static SCM call1(const char *module_name, const char *name, SCM argument) {
  SCM proc = public_proc(module_name, name);

  return scm_is_true(proc) ? scm_call_1(proc, argument) : SCM_BOOL_F;
}

/* The path argument, or no argument at all when there is no path. */
static SCM call_with_path(const char *module_name, const char *name,
                          const char *path) {
  if (path && *path != '\0')
    return call1(module_name, name, scm_from_utf8_string(path));

  return call0(module_name, name);
}

/* =========================================================================
 * CONVERTING SCHEME VALUES
 * =========================================================================
 * The Scheme side hands over alists of plain values (see `bar->alist' in
 * scheme/whelk/state.scm): SRFI-9 accessors are macros, and a macro is not
 * something this file can call.  Everything below is therefore an alist walk.
 */

/* A string or a symbol as a fresh C string, or NULL. */
static char *scm_text(SCM value) {
  SCM text = SCM_BOOL_F;

  if (scm_is_string(value))
    text = value;
  else if (scm_is_symbol(value))
    text = scm_symbol_to_string(value);
  else
    return NULL;

  char *utf8 = scm_to_utf8_string(text);

  if (!utf8)
    return NULL;

  char *copy = g_strdup(utf8);
  free(utf8);

  return copy;
}

/* One option value as text.  Numbers, booleans and symbols keep their Scheme
 * spelling; the typed readers in config.c parse them back. */
static char *scm_value_text(SCM value) {
  char *text = scm_text(value);

  if (text)
    return text;

  if (scm_is_bool(value))
    return g_strdup(scm_is_true(value) ? "#t" : "#f");

  if (scm_is_number(value)) {
    if (scm_is_exact_integer(value))
      return g_strdup_printf("%ld", (long)scm_to_long(value));

    return g_strdup_printf("%g", scm_to_double(value));
  }

  if (scm_is_null(value))
    return g_strdup("");

  if (scm_is_pair(value)) {
    /* A list of values becomes one comma-separated option, which is what a
     * module that wants "all the colours" can split on. */
    GString *joined = g_string_new(NULL);
    SCM cursor = value;

    while (scm_is_pair(cursor)) {
      g_autofree char *item = scm_value_text(SCM_CAR(cursor));

      if (item) {
        if (joined->len > 0)
          g_string_append_c(joined, ',');
        g_string_append(joined, item);
      }

      cursor = SCM_CDR(cursor);
    }

    return g_string_free(joined, FALSE);
  }

  LOG_C_DEBUG("Guile", "an option of an unsupported type is ignored");
  return NULL;
}

/* --- alists --------------------------------------------------------------- */

static SCM alist_get(SCM alist, const char *key) {
  SCM symbol = scm_from_utf8_symbol(key);

  for (SCM cursor = alist; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);

    if (scm_is_pair(entry) && scm_is_eq(SCM_CAR(entry), symbol))
      return SCM_CDR(entry);
  }

  return SCM_BOOL_F;
}

static char *alist_text(SCM alist, const char *key, const char *fallback) {
  char *text = scm_text(alist_get(alist, key));

  return (text && *text != '\0') ? text : g_strdup(fallback);
}

/* An option that is genuinely optional: absent or #f answers NULL. */
static char *alist_optional_text(SCM alist, const char *key) {
  return scm_text(alist_get(alist, key));
}

static int alist_int(SCM alist, const char *key, int fallback) {
  SCM value = alist_get(alist, key);

  return scm_is_number(value) ? (int)scm_to_long(value) : fallback;
}

static gboolean alist_bool(SCM alist, const char *key, gboolean fallback) {
  SCM value = alist_get(alist, key);

  if (scm_is_eq(value, SCM_BOOL_T))
    return TRUE;
  if (scm_is_eq(value, SCM_BOOL_F))
    return FALSE;

  return fallback;
}

static GtkLayerShellEdge edge_from_text(const char *text) {
  if (g_strcmp0(text, "bottom") == 0)
    return GTK_LAYER_SHELL_EDGE_BOTTOM;
  if (g_strcmp0(text, "left") == 0)
    return GTK_LAYER_SHELL_EDGE_LEFT;
  if (g_strcmp0(text, "right") == 0)
    return GTK_LAYER_SHELL_EDGE_RIGHT;

  return GTK_LAYER_SHELL_EDGE_TOP;
}

/* Only a non-empty option is a configuration; an absent one is the module's
 * own default. */
static void collect_options(WhelkModuleSpec *spec, SCM alist) {
  for (SCM cursor = alist; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM pair = SCM_CAR(cursor);

    if (!scm_is_pair(pair))
      continue;

    g_autofree char *key = scm_text(SCM_CAR(pair));
    g_autofree char *value = scm_value_text(SCM_CDR(pair));

    if (key && value && *key != '\0' && *value != '\0')
      whelk_module_spec_set(spec, key, value);
  }
}

static void collect_modules(GPtrArray *target, SCM list) {
  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);
    g_autofree char *kind = scm_text(alist_get(entry, "kind"));

    if (!kind || *kind == '\0')
      continue;

    WhelkModuleSpec *spec = whelk_module_spec_new(kind);

    collect_options(spec, alist_get(entry, "options"));
    g_ptr_array_add(target, spec);
  }
}

static WhelkBarConfig *bar_from_alist(SCM alist) {
  g_autofree char *name = alist_text(alist, "name", "main");
  g_autofree char *edge = alist_text(alist, "edge", "top");
  WhelkBarConfig *bar = whelk_bar_config_new(name);

  bar->edge = edge_from_text(edge);
  bar->height = alist_int(alist, "height", 32);
  bar->margin_top = alist_int(alist, "margin-top", 0);
  bar->margin_right = alist_int(alist, "margin-right", 0);
  bar->margin_bottom = alist_int(alist, "margin-bottom", 0);
  bar->margin_left = alist_int(alist, "margin-left", 0);
  bar->exclusive = alist_bool(alist, "exclusive?", TRUE);
  bar->spacing = alist_int(alist, "spacing", 6);
  bar->padding = alist_int(alist, "padding", 12);
  bar->radius = alist_int(alist, "radius", 0);

  g_free(bar->monitor);
  bar->monitor = alist_text(alist, "monitor", "all");

  bar->font = alist_optional_text(alist, "font");
  bar->font_size = alist_optional_text(alist, "font-size");
  bar->icon_font = alist_optional_text(alist, "icon-font");
  bar->foreground = alist_optional_text(alist, "foreground");
  bar->background = alist_optional_text(alist, "background");
  bar->accent = alist_optional_text(alist, "accent");
  bar->border_color = alist_optional_text(alist, "border-color");
  bar->css = alist_optional_text(alist, "css");

  collect_modules(bar->left, alist_get(alist, "modules-left"));
  collect_modules(bar->center, alist_get(alist, "modules-center"));
  collect_modules(bar->right, alist_get(alist, "modules-right"));

  return bar;
}

/* =========================================================================
 * THE PUBLIC BRIDGE
 * ========================================================================= */

void whelk_bridge_setup_load_path(void) {
  /* The checkout's own modules first: running from the build tree must read
   * the tree, not a stale copy installed under the Guile site directory. */
  scm_c_eval_string("(set! %load-path (cons \"scheme\" %load-path))");

#ifdef WHELK_SCHEME_DIR
  scm_c_eval_string("(set! %load-path (cons \"" WHELK_SCHEME_DIR
                    "\" %load-path))");
#endif
}

/* Loads the module; a missing or broken one is a warning, not a crash. */
static SCM use_core_thunk(void *data) {
  scm_c_use_module((const char *)data);
  return SCM_BOOL_T;
}

static SCM on_use_core_error(void *data, SCM tag, SCM args) {
  (void)data;
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile",
             "could not load (whelk core): the Scheme modules are not on the "
             "load path; run whelk from the project root or install it");

  return SCM_BOOL_F;
}

gboolean whelk_bridge_use_core(void) {
  scm_internal_catch(SCM_BOOL_T, use_core_thunk, (void *)"whelk core",
                     on_use_core_error, NULL);

  return public_proc("whelk core", "init-shell!") != SCM_BOOL_F;
}

char *whelk_bridge_config_path(void) {
  return g_build_filename(g_get_user_config_dir(), "whelk", "settings.scm",
                          NULL);
}

gboolean whelk_bridge_init(void) {
  SCM proc = public_proc("whelk core", "init-shell!");

  if (!scm_is_true(proc))
    return FALSE;

  scm_call_0(proc);
  return TRUE;
}

gboolean whelk_bridge_load(const char *path) {
  SCM proc = public_proc("whelk core", "load-shell-config!");

  if (!scm_is_true(proc) || !path)
    return FALSE;

  return scm_is_true(scm_call_1(proc, scm_from_utf8_string(path)));
}

void whelk_bridge_start_repl(const char *path) {
  SCM proc = public_proc("whelk core", "start-repl!");

  if (!scm_is_true(proc))
    return;

  if (path && *path)
    scm_call_1(proc, scm_from_utf8_string(path));
  else
    scm_call_0(proc);
}

GPtrArray *whelk_bridge_bars(const char *path) {
  GPtrArray *bars = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_bar_config_free);
  SCM list = call_with_path("whelk state", "shell-bars->alists", path);

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    WhelkBarConfig *bar = bar_from_alist(SCM_CAR(cursor));

    if (bar)
      g_ptr_array_add(bars, bar);
  }

  return bars;
}

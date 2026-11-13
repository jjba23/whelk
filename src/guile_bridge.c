// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "guile_bridge.h"

#include <libguile.h>
#include <stdlib.h>
#include <string.h>

#include "logger.h"
#include "util/chord.h"
#include "util/menudict.h"
#include "util/menutree.h"

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

void whelk_bridge_start_guile(void) {
  static gboolean started = FALSE;

  if (started)
    return;

  scm_init_guile();
  started = TRUE;
}

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
static SCM use_module_thunk(void *data) {
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
  scm_internal_catch(SCM_BOOL_T, use_module_thunk, (void *)"whelk core",
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

WhelkClipboardConfig *whelk_bridge_clipboard(const char *path) {
  WhelkClipboardConfig *config = whelk_clipboard_config_new();
  SCM alist = call_with_path("whelk state", "shell-clipboard->alist", path);

  if (!scm_is_pair(alist))
    return config;

  config->configured = alist_bool(alist, "configured?", TRUE);
  config->persist_only_pinned =
      alist_bool(alist, "persist-only-pinned?", FALSE);
  config->clear_on_lock = alist_bool(alist, "clear-on-lock?", FALSE);

  g_autofree char *persist = alist_optional_text(alist, "persist");

  if (persist && *persist && g_strcmp0(persist, "#f") != 0) {
    config->persist = TRUE;

    if (g_strcmp0(persist, "auto") != 0 && g_strcmp0(persist, "#t") != 0)
      config->persist_path = g_strdup(persist);
  }

  const int max_entries = alist_int(alist, "max-entries", 0);
  const int max_entry = alist_int(alist, "max-entry-bytes", 0);
  const int max_total = alist_int(alist, "max-total-bytes", 0);
  const int ttl = alist_int(alist, "ttl-seconds", 0);

  config->max_entries = max_entries > 0 ? (guint)max_entries : 0;
  config->max_entry_bytes = max_entry > 0 ? (guint)max_entry : 0;
  config->max_total_bytes = max_total > 0 ? (guint)max_total : 0;
  config->ttl_seconds = ttl > 0 ? (guint)ttl : 0;

  return config;
}

/* =========================================================================
 * THE MENU DICTIONARY
 * =========================================================================
 * Scheme holds the applications (scheme/whelk/menudict.scm); this is the one
 * walk over the alists it exports, at startup.  Everything here becomes plain
 * C strings and structs owned by the table, so no Scheme value outlives the
 * call.
 */

static SCM on_use_module_error(void *data, SCM tag, SCM args) {
  (void)tag;
  (void)args;

  LOG_C_WARN("Guile", "could not load (%s): what it describes is missing",
             (const char *)data);

  return SCM_BOOL_F;
}

static gboolean load_module(const char *name) {
  SCM result = scm_internal_catch(SCM_BOOL_T, use_module_thunk, (void *)name,
                                  on_use_module_error, (void *)name);

  return scm_is_true(result);
}

static guint scheme_list_length(SCM list) {
  guint length = 0;

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor))
    length++;

  return length;
}

static SCM scheme_list_ref(SCM list, guint index) {
  for (guint i = 0; i < index && scm_is_pair(list); i++)
    list = SCM_CDR(list);

  return scm_is_pair(list) ? SCM_CAR(list) : SCM_BOOL_F;
}

/* A list of strings as the NULL-terminated array the lookup walks.  A missing
 * word keeps its place as an empty string rather than cutting the list short. */
static char **scheme_text_list(SCM list) {
  guint length = scheme_list_length(list);
  char **words = g_new0(char *, length + 1);

  for (guint i = 0; i < length; i++) {
    char *text = scm_text(scheme_list_ref(list, i));

    words[i] = text ? text : g_strdup("");
  }

  return words;
}

/* (label accel modifiers key), with #f for a bare key, or the shorter
 * (label chord), where the chord may be an Emacs sequence (C-x C-s) or one
 * chord in either notation.  This is where it stops being text and becomes
 * the modifiers and keysyms the runtime types. */
static WhelkDictItem *scheme_items(SCM list, guint *n_items) {
  guint length = scheme_list_length(list);
  WhelkDictItem *items = whelk_dict_items_new(length);

  for (guint i = 0; i < length; i++) {
    SCM spec = scheme_list_ref(list, i);

    items[i].label = scm_text(scheme_list_ref(spec, 0));

    if (scheme_list_length(spec) == 2) {
      g_autofree char *written = scm_text(scheme_list_ref(spec, 1));
      WhelkChordSequence sequence;

      if (written && whelk_chord_sequence_parse(written, &sequence)) {
        /* What the dictionary stores is not what the menu shows: the table
         * keeps the sequence as CUA, and the bar writes it in the style its
         * configuration asked for. */
        items[i].accel = whelk_chord_sequence_format(&sequence,
                                                     WHELK_CHORD_CUA);
        items[i].n_chords = sequence.n_chords;
        items[i].modifiers = g_new0(char *, sequence.n_chords);
        items[i].keys = g_new0(char *, sequence.n_chords);

        for (guint j = 0; j < sequence.n_chords; j++) {
          items[i].modifiers[j] = whelk_chord_modifiers(&sequence.chords[j]);
          items[i].keys[j] = g_strdup(sequence.chords[j].key);
        }

        whelk_chord_sequence_clear(&sequence);
      } else {
        /* A chord this shell cannot read still prints: the entry is shown
         * disabled rather than silently dropped. */
        items[i].accel = g_strdup(written ? written : "");
      }

      continue;
    }

    items[i].accel = scm_text(scheme_list_ref(spec, 1));
    items[i].modifiers = g_new0(char *, 1);
    items[i].keys = g_new0(char *, 1);
    items[i].modifiers[0] = scm_text(scheme_list_ref(spec, 2));
    items[i].keys[0] = scm_text(scheme_list_ref(spec, 3));
    items[i].n_chords = 1;
  }

  *n_items = length;
  return items;
}

/* (label item ...), in the order the Scheme wrote them. */
static WhelkDictMenu *scheme_menus(SCM list, guint *n_menus) {
  guint length = scheme_list_length(list);
  WhelkDictMenu *menus = whelk_dict_menus_new(length);

  for (guint i = 0; i < length; i++) {
    SCM entry = scheme_list_ref(list, i);

    menus[i].label = scm_text(scheme_list_ref(entry, 0));
    menus[i].items = scheme_items(scm_is_pair(entry) ? SCM_CDR(entry) : SCM_EOL,
                                  &menus[i].n_items);
  }

  *n_menus = length;
  return menus;
}

GPtrArray *whelk_bridge_menudict(void) {
  GPtrArray *table = whelk_menudict_table_new();

  if (!load_module("whelk menudict"))
    return table;

  SCM list = call0("whelk menudict", "menudict->alists");

  for (SCM cursor = list; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM entry = SCM_CAR(cursor);
    char *name = alist_text(entry, "name", "unnamed");
    char **ids = scheme_text_list(alist_get(entry, "ids"));
    guint n_menus = 0;
    WhelkDictMenu *menus = scheme_menus(alist_get(entry, "menus"), &n_menus);

    g_ptr_array_add(table, whelk_dict_app_new(name, ids, menus, n_menus));
  }

  return table;
}

/* =========================================================================
 * THE SHELL'S OWN MENU
 * =========================================================================
 * The popover behind the mark at the far left of the bar is configuration, not
 * a C table: a settings file writes it with `(whelk menu)' units, and the
 * bridge turns those into the same menu tree the global menu uses.  An action
 * spelled `shell:name' is a thing the shell itself does (show the launcher,
 * open the notification centre, offer the windows to close); anything else is a
 * command line, run the way every other click on this bar runs one.
 *
 * A file that configures no menu gets `default-shell-menu', so the walk below
 * is the same in both cases -- and so the default lives where a person can read
 * it and a machine can override it.
 */

static void fill_shell_units(WhelkMenu *menu, SCM units) {
  for (SCM cursor = units; scm_is_pair(cursor); cursor = SCM_CDR(cursor)) {
    SCM unit = SCM_CAR(cursor);

    if (!scm_is_pair(unit))
      continue;

    g_autofree char *tag = scm_text(SCM_CAR(unit));
    SCM rest = SCM_CDR(unit);

    if (!tag)
      continue;

    if (g_strcmp0(tag, "separator") == 0) {
      whelk_menu_add_separator(menu);
      continue;
    }

    if (g_strcmp0(tag, "item") == 0) {
      g_autofree char *label = scm_text(scheme_list_ref(rest, 0));
      g_autofree char *action = scm_text(scheme_list_ref(rest, 1));

      if (!label || *label == '\0')
        continue;

      WhelkMenuEntry *entry = whelk_menu_add_entry(menu, label);

      if (!action || *action == '\0')
        entry->enabled = FALSE; /* a row that says what it cannot do */
      else if (g_str_has_prefix(action, "shell:"))
        entry->action = g_strdup(action);
      else
        entry->command = g_strdup(action);

      continue;
    }

    if (g_strcmp0(tag, "submenu") == 0 || g_strcmp0(tag, "menu") == 0) {
      g_autofree char *label = scm_text(scheme_list_ref(rest, 0));
      SCM body = scm_is_pair(rest) ? SCM_CDR(rest) : SCM_EOL;

      if (!label || *label == '\0')
        continue;

      WhelkMenu *submenu = whelk_menu_add_submenu(menu, label);
      WhelkMenuEntry *entry = menu->entries->len > 0
                                  ? g_ptr_array_index(menu->entries,
                                                      menu->entries->len - 1)
                                  : NULL;

      /* A submenu whose body is one string is a slot the shell fills itself
       * when it opens -- `(submenu "Recent Items" "shell:recent-items")'. */
      SCM marker_arg = scm_is_pair(body) ? SCM_CAR(body) : SCM_BOOL_F;

      if ((scm_is_string(marker_arg) || scm_is_symbol(marker_arg)) &&
          scheme_list_length(body) == 1) {
        g_autofree char *marker = scm_text(marker_arg);

        if (entry && marker && *marker != '\0')
          entry->action = g_strdup(marker);

        continue;
      }

      fill_shell_units(submenu, body);
      continue;
    }
  }
}

GPtrArray *whelk_bridge_shell_menu(const char *path) {
  GPtrArray *bar = whelk_menubar_new();
  SCM units = call_with_path("whelk state", "shell-menu->units", path);

  if (!scm_is_pair(units)) {
    if (!load_module("whelk defaults"))
      return bar;

    units = call0("whelk defaults", "default-shell-menu");
  }

  WhelkMenu *root = whelk_menu_new("shell");

  fill_shell_units(root, units);

  if (root->entries->len > 0)
    g_ptr_array_add(bar, root);
  else
    whelk_menu_free(root);

  return bar;
}

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "config.h"

#include <stdlib.h>
#include <string.h>

/* =========================================================================
 * MODULE SPECS
 * ========================================================================= */

WhelkModuleSpec *whelk_module_spec_new(const char *kind) {
  WhelkModuleSpec *spec = g_new0(WhelkModuleSpec, 1);

  spec->kind = g_strdup(kind ? kind : "");
  spec->options =
      g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);

  return spec;
}

void whelk_module_spec_free(WhelkModuleSpec *spec) {
  if (!spec)
    return;

  g_free(spec->kind);
  g_clear_pointer(&spec->options, g_hash_table_unref);
  g_free(spec);
}

void whelk_module_spec_set(WhelkModuleSpec *spec, const char *key,
                           const char *value) {
  if (!spec || !key || !value)
    return;

  g_hash_table_insert(spec->options, g_strdup(key), g_strdup(value));
}

gboolean whelk_spec_has(const WhelkModuleSpec *spec, const char *key) {
  if (!spec || !spec->options || !key)
    return FALSE;

  const char *value = g_hash_table_lookup(spec->options, key);
  return value != NULL && *value != '\0';
}

const char *whelk_spec_str(const WhelkModuleSpec *spec, const char *key,
                           const char *fallback) {
  if (!whelk_spec_has(spec, key))
    return fallback;

  return g_hash_table_lookup(spec->options, key);
}

int whelk_spec_int(const WhelkModuleSpec *spec, const char *key, int fallback) {
  const char *text = whelk_spec_str(spec, key, NULL);

  if (!text)
    return fallback;

  char *end = NULL;
  long value = strtol(text, &end, 10);

  /* Trailing whitespace only: "2 " is a number, "2px" is not. */
  if (end == text)
    return fallback;
  while (end && g_ascii_isspace(*end))
    end++;
  if (end && *end != '\0')
    return fallback;

  return (int)value;
}

double whelk_spec_double(const WhelkModuleSpec *spec, const char *key,
                         double fallback) {
  const char *text = whelk_spec_str(spec, key, NULL);

  if (!text)
    return fallback;

  char *end = NULL;
  double value = g_ascii_strtod(text, &end);

  if (end == text)
    return fallback;
  while (end && g_ascii_isspace(*end))
    end++;
  if (end && *end != '\0')
    return fallback;

  return value;
}

gboolean whelk_spec_bool(const WhelkModuleSpec *spec, const char *key,
                         gboolean fallback) {
  const char *text = whelk_spec_str(spec, key, NULL);

  if (!text)
    return fallback;

  /* The Scheme writer is the authority: true/#t/yes/1 mean true, everything
   * else that is present means false.  A missing option is the fallback. */
  if (g_ascii_strcasecmp(text, "true") == 0 || g_strcmp0(text, "#t") == 0 ||
      g_ascii_strcasecmp(text, "yes") == 0 || g_strcmp0(text, "1") == 0)
    return TRUE;

  return FALSE;
}

/* =========================================================================
 * DESKTOP WIDGETS
 * ========================================================================= */

WhelkWidgetConfig *whelk_widget_config_new(const char *kind) {
  WhelkWidgetConfig *config = g_new0(WhelkWidgetConfig, 1);

  config->spec = whelk_module_spec_new(kind ? kind : "custom");
  config->x = 48;
  config->y = 64;
  config->width = 320;
  config->height = 220;
  config->monitor = g_strdup("default");

  return config;
}

WhelkWidgetConfig *whelk_widget_config_copy(const WhelkWidgetConfig *config) {
  if (!config)
    return NULL;

  WhelkWidgetConfig *copy = whelk_widget_config_new(config->spec->kind);
  GHashTableIter iter;
  gpointer key;
  gpointer value;

  g_hash_table_iter_init(&iter, config->spec->options);
  while (g_hash_table_iter_next(&iter, &key, &value))
    whelk_module_spec_set(copy->spec, (const char *)key, (const char *)value);

  copy->x = config->x;
  copy->y = config->y;
  copy->width = config->width;
  copy->height = config->height;

  g_free(copy->monitor);
  copy->monitor = g_strdup(config->monitor ? config->monitor : "default");

  return copy;
}

void whelk_widget_config_free(WhelkWidgetConfig *config) {
  if (!config)
    return;

  whelk_module_spec_free(config->spec);
  g_free(config->monitor);
  g_free(config);
}

void whelk_config_free_widgets(GPtrArray *widgets) {
  if (!widgets)
    return;

  g_ptr_array_unref(widgets);
}

/* =========================================================================
 * BARS
 * ========================================================================= */

WhelkBarConfig *whelk_bar_config_new(const char *name) {
  WhelkBarConfig *bar = g_new0(WhelkBarConfig, 1);

  bar->name = g_strdup(name && *name ? name : "main");
  bar->edge = GTK_LAYER_SHELL_EDGE_TOP;
  bar->layer = GTK_LAYER_SHELL_LAYER_TOP;
  bar->monitor = g_strdup("all");
  bar->height = 32;
  bar->margin_top = 0;
  bar->margin_right = 0;
  bar->margin_bottom = 0;
  bar->margin_left = 0;
  bar->exclusive = TRUE;
  bar->spacing = 6;
  bar->padding = 12;
  bar->radius = 0;

  bar->left = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_module_spec_free);
  bar->center = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_module_spec_free);
  bar->right = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_module_spec_free);

  return bar;
}

static WhelkModuleSpec *module_spec_copy(const WhelkModuleSpec *spec) {
  WhelkModuleSpec *copy = whelk_module_spec_new(spec->kind);
  GHashTableIter iter;
  gpointer key;
  gpointer value;

  g_hash_table_iter_init(&iter, spec->options);
  while (g_hash_table_iter_next(&iter, &key, &value))
    whelk_module_spec_set(copy, (const char *)key, (const char *)value);

  return copy;
}

static void copy_modules(GPtrArray *target, const GPtrArray *source) {
  for (guint i = 0; source && i < source->len; i++)
    g_ptr_array_add(target,
                    module_spec_copy(g_ptr_array_index(source, i)));
}

WhelkBarConfig *whelk_bar_config_copy(const WhelkBarConfig *bar) {
  if (!bar)
    return NULL;

  WhelkBarConfig *copy = whelk_bar_config_new(bar->name);

  copy->edge = bar->edge;
  copy->layer = bar->layer;
  copy->height = bar->height;
  copy->margin_top = bar->margin_top;
  copy->margin_right = bar->margin_right;
  copy->margin_bottom = bar->margin_bottom;
  copy->margin_left = bar->margin_left;
  copy->exclusive = bar->exclusive;
  copy->spacing = bar->spacing;
  copy->padding = bar->padding;
  copy->radius = bar->radius;
  copy->native_network = bar->native_network;
  copy->native_bluetooth = bar->native_bluetooth;

  g_free(copy->monitor);
  copy->monitor = g_strdup(bar->monitor ? bar->monitor : "all");

  copy->font = g_strdup(bar->font);
  copy->theme = g_strdup(bar->theme);
  copy->font_size = g_strdup(bar->font_size);
  copy->icon_font = g_strdup(bar->icon_font);
  copy->foreground = g_strdup(bar->foreground);
  copy->background = g_strdup(bar->background);
  copy->accent = g_strdup(bar->accent);
  copy->border_color = g_strdup(bar->border_color);
  copy->css = g_strdup(bar->css);

  copy_modules(copy->left, bar->left);
  copy_modules(copy->center, bar->center);
  copy_modules(copy->right, bar->right);

  return copy;
}

void whelk_bar_config_free(WhelkBarConfig *bar) {
  if (!bar)
    return;

  g_free(bar->name);
  g_free(bar->theme);
  g_free(bar->monitor);
  g_free(bar->font);
  g_free(bar->font_size);
  g_free(bar->icon_font);
  g_free(bar->foreground);
  g_free(bar->background);
  g_free(bar->accent);
  g_free(bar->border_color);
  g_free(bar->css);
  g_clear_pointer(&bar->left, g_ptr_array_unref);
  g_clear_pointer(&bar->center, g_ptr_array_unref);
  g_clear_pointer(&bar->right, g_ptr_array_unref);
  g_free(bar);
}

void whelk_config_free_bars(GPtrArray *bars) {
  if (!bars)
    return;

  g_ptr_array_unref(bars);
}

/* Adds one spec, given a kind and a NULL-terminated list of key/value pairs. */
static void add_spec(GPtrArray *target, const char *kind, ...) {
  WhelkModuleSpec *spec = whelk_module_spec_new(kind);
  va_list args;

  va_start(args, kind);
  for (;;) {
    const char *key = va_arg(args, const char *);
    if (!key)
      break;

    const char *value = va_arg(args, const char *);
    whelk_module_spec_set(spec, key, value);
  }
  va_end(args);

  g_ptr_array_add(target, spec);
}

GPtrArray *whelk_config_default_bars(void) {
  GPtrArray *bars = g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_bar_config_free);
  WhelkBarConfig *bar = whelk_bar_config_new("main");

  add_spec(bar->left, "custom", "exec", "printf '%s' \"$(date '+%a %d %b')\"",
           "interval", "60", "format", "{text}", NULL);
  add_spec(bar->right, "cpu", "interval", "2", NULL);
  add_spec(bar->right, "memory", "interval", "5", NULL);
  add_spec(bar->right, "tray", "spacing", "6", NULL);
  add_spec(bar->right, "clock", "interval", "5", "format", "%a %d %b  %H:%M",
           NULL);

  g_ptr_array_add(bars, bar);
  return bars;
}

WhelkClipboardConfig *whelk_clipboard_config_new(void) {
  WhelkClipboardConfig *config = g_new0(WhelkClipboardConfig, 1);

  return config;
}

void whelk_clipboard_config_free(WhelkClipboardConfig *config) {
  if (!config)
    return;

  g_free(config->persist_path);
  g_free(config);
}

const char *whelk_edge_name(GtkLayerShellEdge edge) {
  switch (edge) {
  case GTK_LAYER_SHELL_EDGE_BOTTOM:
    return "bottom";
  case GTK_LAYER_SHELL_EDGE_LEFT:
    return "left";
  case GTK_LAYER_SHELL_EDGE_RIGHT:
    return "right";
  case GTK_LAYER_SHELL_EDGE_TOP:
  case GTK_LAYER_SHELL_EDGE_ENTRY_NUMBER:
  default:
    return "top";
  }
}

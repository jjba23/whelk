// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "module.h"

#include "logger.h"
#include "modules/modules.h"
#include "util/click.h"
#include "util/command.h"
#include "util/format.h"

/* =========================================================================
 * THE FACTORY TABLE
 * =========================================================================
 * One row per module.  This table is the whole dispatch: the configuration's
 * kind symbol is looked up here, so a bar can name a module the shell does not
 * have without taking the rest of the bar down with it.
 */

static const WhelkModuleEntry factories[] = {
    {"audio", whelk_audio_module_new},
    {"backlight", whelk_backlight_module_new},
    {"battery", whelk_battery_module_new},
    {"bluetooth", whelk_bluetooth_module_new},
    {"clock", whelk_clock_module_new},
    {"clipboard", whelk_clipboard_module_new},
    {"cpu", whelk_cpu_module_new},
    {"custom", whelk_custom_module_new},
    {"scheme", whelk_scheme_module_new},
    {"dnd", whelk_dnd_module_new},
    {"keyboard", whelk_keyboard_module_new},
    {"language", whelk_language_module_new},
    {"memory", whelk_memory_module_new},
    {"menu", whelk_menu_module_new},
    {"notifications", whelk_notifications_module_new},
    {"power-profile", whelk_power_profile_module_new},
    {"start", whelk_start_module_new},
    {"tray", whelk_tray_module_new},
    {"wifi", whelk_wifi_module_new},
    {"window", whelk_window_module_new},
    {"workspaces", whelk_workspaces_module_new},
};

/* =========================================================================
 * LIVENESS
 * =========================================================================
 * A poll may outlive the bar that asked for it (a reload while `niri msg`
 * is still running).  The guard is a tiny refcounted token: the module owns one
 * reference, every in-flight command owns another, and a completed command that
 * finds `module` cleared just drops its reference and says nothing.
 */

typedef struct {
  WhelkModule *module;
  guint refs;
} ModuleGuard;

static ModuleGuard *guard_ref(ModuleGuard *guard) {
  guard->refs++;
  return guard;
}

static void guard_unref(ModuleGuard *guard) {
  if (--guard->refs > 0)
    return;

  g_free(guard);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

WhelkModule *whelk_module_base_new(const char *kind, const WhelkModuleSpec *spec,
                                   const WhelkBarConfig *bar) {
  WhelkModule *module = g_new0(WhelkModule, 1);

  module->kind = g_strdup(kind ? kind : "");
  module->spec = spec;
  module->bar = bar;
  module->root = gtk_box_new(GTK_ORIENTATION_HORIZONTAL,
                             bar && bar->spacing > 0 ? bar->spacing / 2 : 2);

  gtk_widget_add_css_class(module->root, "whelk-module");

  if (kind && *kind) {
    g_autofree char *kind_class = g_strdup_printf("whelk-%s", kind);
    gtk_widget_add_css_class(module->root, kind_class);
  }

  const char *extra_class = whelk_spec_str(spec, "css-class", NULL);
  if (extra_class && *extra_class)
    gtk_widget_add_css_class(module->root, extra_class);

  ModuleGuard *guard = g_new0(ModuleGuard, 1);
  guard->module = module;
  guard->refs = 1;
  module->guard = guard;

  return module;
}

GtkWidget *whelk_module_add_label(WhelkModule *module) {
  if (!module)
    return NULL;

  GtkWidget *label = gtk_label_new(NULL);

  gtk_widget_add_css_class(label, "whelk-label");
  gtk_widget_set_valign(label, GTK_ALIGN_CENTER);
  gtk_box_append(GTK_BOX(module->root), label);
  module->label = label;

  return label;
}

void whelk_module_free(WhelkModule *module) {
  if (!module)
    return;

  if (module->timer_id != 0) {
    g_source_remove(module->timer_id);
    module->timer_id = 0;
  }

  if (module->destroy)
    module->destroy(module);

  if (module->guard) {
    ModuleGuard *guard = module->guard;
    guard->module = NULL;
    guard_unref(guard);
    module->guard = NULL;
  }

  if (module->root) {
    if (gtk_widget_get_parent(module->root))
      gtk_widget_unparent(module->root);
    module->root = NULL;
  }

  g_free(module->kind);
  g_free(module->text);
  g_free(module);
}

/* =========================================================================
 * BUILDING
 * ========================================================================= */

WhelkModule *whelk_module_new(const WhelkModuleSpec *spec,
                              const WhelkBarConfig *bar) {
  if (!spec || !spec->kind)
    return NULL;

  for (guint i = 0; i < G_N_ELEMENTS(factories); i++) {
    if (g_strcmp0(factories[i].kind, spec->kind) == 0)
      return factories[i].create(spec, bar);
  }

  LOG_C_WARN("Module", "Unknown module kind '%s': it is left out of the bar",
             spec->kind);
  return NULL;
}

static gboolean on_refresh_timer(gpointer user_data) {
  WhelkModule *module = user_data;

  if (module->update)
    module->update(module);

  return G_SOURCE_CONTINUE;
}

void whelk_module_install(WhelkModule *module, guint fallback_interval) {
  if (!module)
    return;

  whelk_click_attach(module->root, module->spec);

  if (module->update)
    module->update(module);

  const int seconds = whelk_spec_int(module->spec, "interval",
                                     (int)fallback_interval);

  if (seconds > 0 && module->update)
    module->timer_id =
        g_timeout_add_seconds((guint)seconds, on_refresh_timer, module);
}

/* =========================================================================
 * RENDERING
 * ========================================================================= */

GHashTable *whelk_values_new(void) {
  return g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);
}

void whelk_values_set(GHashTable *values, const char *key, const char *value) {
  if (!values || !key)
    return;

  g_hash_table_insert(values, g_strdup(key), g_strdup(value ? value : ""));
}

void whelk_values_set_int(GHashTable *values, const char *key, int value) {
  g_autofree char *text = g_strdup_printf("%d", value);

  whelk_values_set(values, key, text);
}

/* Substitutes every value; @escape_marks decides whether values become markup
 * text or are inserted as they are. */
static char *render_values(const char *format, GHashTable *values,
                           gboolean escape_marks, const char *icon,
                           const char *icon_markup) {
  char *out = g_strdup(format ? format : "");
  GHashTableIter iter;
  gpointer key;
  gpointer value;

  g_hash_table_iter_init(&iter, values);
  while (g_hash_table_iter_next(&iter, &key, &value)) {
    const char *text = value;
    g_autofree char *escaped = NULL;

    if (escape_marks) {
      escaped = whelk_markup_escape(text);
      text = escaped;
    }

    char *next = whelk_str_subst(out, (const char *)key, text);
    g_free(out);
    out = next;
  }

  char *next = whelk_str_subst(out, "icon",
                               escape_marks ? icon_markup : (icon ? icon : ""));
  g_free(out);

  return next;
}

void whelk_module_set_text(WhelkModule *module, const char *text) {
  if (!module)
    return;

  g_free(module->text);
  module->text = g_strdup(text ? text : "");

  if (module->label) {
    g_autofree char *escaped = whelk_markup_escape(module->text);
    gtk_label_set_markup(GTK_LABEL(module->label), escaped);
  }

  whelk_module_refresh_tooltip(module);
}

void whelk_module_set_markup(WhelkModule *module, const char *markup) {
  if (!module)
    return;

  if (module->label)
    gtk_label_set_markup(GTK_LABEL(module->label), markup ? markup : "");
}

/* Renders @format into the module's hover text with @values. */
static void apply_tooltip(WhelkModule *module, const char *format,
                          GHashTable *values) {
  if (!format || *format == '\0')
    return;

  g_autofree char *tooltip = render_values(format, values, TRUE, "", "");

  gtk_widget_set_tooltip_markup(module->root, tooltip);
}

void whelk_module_refresh_tooltip(WhelkModule *module) {
  if (!module)
    return;

  const char *format = whelk_spec_str(module->spec, "tooltip", NULL);

  if (!format)
    return;

  GHashTable *values = whelk_values_new();
  whelk_values_set(values, "text", module->text ? module->text : "");

  apply_tooltip(module, format, values);
  g_hash_table_unref(values);
}

void whelk_module_present_format(WhelkModule *module, const char *format,
                                 const char *icon, const char *tooltip,
                                 GHashTable *values) {
  if (!module || !values) {
    if (values)
      g_hash_table_unref(values);
    return;
  }

  const char *glyph = icon ? icon : "";
  g_autofree char *icon_markup =
      whelk_icon_markup(glyph, module->bar ? module->bar->icon_font : NULL);

  /* The plain reading first: it is what {text} means in a tooltip, and it is
   * what a module without a label still knows it is saying. */
  g_autofree char *plain =
      render_values(format, values, FALSE, glyph, icon_markup);

  g_free(module->text);
  module->text = g_strdup(plain);

  if (module->label) {
    g_autofree char *markup =
        render_values(format, values, TRUE, glyph, icon_markup);
    gtk_label_set_markup(GTK_LABEL(module->label), markup);
  }

  if (tooltip) {
    whelk_values_set(values, "text", plain);
    apply_tooltip(module, tooltip, values);
  } else {
    whelk_module_refresh_tooltip(module);
  }

  g_hash_table_unref(values);
}

void whelk_module_present(WhelkModule *module, const char *default_format,
                          GHashTable *values) {
  if (!module) {
    if (values)
      g_hash_table_unref(values);
    return;
  }

  const char *format = whelk_spec_str(module->spec, "format", default_format);
  const char *icon = whelk_spec_str(module->spec, "icon", "");
  const char *tooltip = whelk_spec_str(module->spec, "tooltip", NULL);

  whelk_module_present_format(module, format, icon, tooltip, values);
}

void whelk_module_add_class(WhelkModule *module, const char *css_class) {
  if (!module || !css_class)
    return;

  gtk_widget_add_css_class(module->root, css_class);
}

void whelk_module_remove_class(WhelkModule *module, const char *css_class) {
  if (!module || !css_class)
    return;

  gtk_widget_remove_css_class(module->root, css_class);
}

/* =========================================================================
 * ASYNCHRONOUS READINGS
 * ========================================================================= */

typedef struct {
  ModuleGuard *guard;
  void (*callback)(WhelkModule *module, const char *text, gpointer data);
  gpointer user_data;
} CommandContext;

static void on_module_command(const char *text, gpointer user_data) {
  CommandContext *ctx = user_data;
  WhelkModule *module = ctx->guard->module;

  if (module) {
    module->busy = FALSE;

    if (ctx->callback)
      ctx->callback(module, text ? text : "", ctx->user_data);
  }

  guard_unref(ctx->guard);
  g_free(ctx);
}

void whelk_module_command(WhelkModule *module, const char *command,
                          void (*callback)(WhelkModule *module,
                                           const char *text, gpointer data),
                          gpointer user_data) {
  if (!module || !module->guard || !command || *command == '\0')
    return;

  if (module->busy)
    return;

  module->busy = TRUE;

  CommandContext *ctx = g_new0(CommandContext, 1);
  ctx->guard = guard_ref(module->guard);
  ctx->callback = callback;
  ctx->user_data = user_data;

  whelk_command_run(command, on_module_command, ctx);
}

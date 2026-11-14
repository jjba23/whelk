// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "preferences.h"

#include "config.h"
#include "guile_bridge.h"
#include "logger.h"

#include <string.h>

/* =========================================================================
 * THE SETTINGS WINDOW
 * =========================================================================
 * A normal GTK window, not a layer surface: settings are a task, not part of
 * the bar.  The window is a stack of small, named pages so a bar with a Wi-Fi
 * reading and a bar without one still have the same readable shape.
 *
 * Every control carries its preference key as object data.  That is what the
 * widget test finds, and it keeps the mapping from a widget to the state file
 * explicit instead of hiding it in a switch over labels.
 */

#define APPLY_DEBOUNCE_MS 180

typedef enum {
  CHANGE_BOOL,
  CHANGE_INT,
  CHANGE_DOUBLE,
  CHANGE_STRING,
  CHANGE_SYMBOL,
} ChangeKind;

typedef struct {
  ChangeKind kind;
  gboolean boolean_value;
  int int_value;
  double double_value;
  char *text;
} PendingChange;

typedef struct {
  char *config_path;
  GtkWidget *window; /* owned by the window; NULL in a content-only test */
  GtkWidget *host;   /* the box build_ui fills */
  WhelkBarConfig *bar;
  GHashTable *pending; /* key -> PendingChange */
  guint apply_id;
  WhelkPreferencesReloadFunc reload;
  gpointer reload_data;
  gboolean loading;
} PrefsState;

static GtkWindow *settings_window = NULL;

static void pending_change_free(PendingChange *change) {
  if (!change)
    return;

  g_free(change->text);
  g_free(change);
}

static void prefs_state_free(PrefsState *state) {
  if (!state)
    return;

  if (state->apply_id != 0)
    g_source_remove(state->apply_id);

  g_clear_pointer(&state->pending, g_hash_table_unref);
  g_clear_pointer(&state->bar, whelk_bar_config_free);
  g_free(state->config_path);
  g_free(state);
}

/* =========================================================================
 * LOADING THE CURRENT BAR
 * ========================================================================= */

static const WhelkModuleSpec *find_spec(const WhelkBarConfig *bar,
                                        const char *kind) {
  const GPtrArray *groups[] = {bar ? bar->left : NULL,
                               bar ? bar->center : NULL,
                               bar ? bar->right : NULL};

  for (gsize group = 0; group < G_N_ELEMENTS(groups); group++) {
    if (!groups[group])
      continue;

    for (guint i = 0; i < groups[group]->len; i++) {
      WhelkModuleSpec *spec = g_ptr_array_index(groups[group], i);

      if (g_strcmp0(spec->kind, kind) == 0)
        return spec;
    }
  }

  return NULL;
}

static gboolean has_module(const WhelkBarConfig *bar, const char *kind) {
  return find_spec(bar, kind) != NULL;
}

static WhelkBarConfig *copy_bar(const GPtrArray *bars) {
  if (!bars || bars->len == 0)
    return NULL;

  return whelk_bar_config_copy(g_ptr_array_index(bars, 0));
}

static WhelkBarConfig *load_current_bar(const char *config_path) {
  GPtrArray *bars = whelk_bridge_bars(config_path);
  WhelkBarConfig *bar = copy_bar(bars);

  whelk_config_free_bars(bars);

  if (!bar) {
    bars = whelk_config_default_bars();
    bar = copy_bar(bars);
    whelk_config_free_bars(bars);
  }

  return bar;
}

/* =========================================================================
 * WRITING A CHANGE
 * ========================================================================= */

static gboolean apply_pending(gpointer user_data) {
  PrefsState *state = user_data;
  GHashTableIter iter;
  gpointer key;
  gpointer value;
  gboolean wrote = FALSE;

  state->apply_id = 0;

  g_hash_table_iter_init(&iter, state->pending);
  while (g_hash_table_iter_next(&iter, &key, &value)) {
    PendingChange *change = value;

    switch (change->kind) {
    case CHANGE_BOOL:
      wrote |= whelk_bridge_save_preference_bool(state->config_path, key,
                                                 change->boolean_value);
      break;
    case CHANGE_INT:
      wrote |= whelk_bridge_save_preference_int(state->config_path, key,
                                                change->int_value);
      break;
    case CHANGE_DOUBLE:
      wrote |= whelk_bridge_save_preference_double(
          state->config_path, key, change->double_value);
      break;
    case CHANGE_STRING:
      wrote |= whelk_bridge_save_preference_string(state->config_path, key,
                                                   change->text);
      break;
    case CHANGE_SYMBOL:
      wrote |= whelk_bridge_save_preference_symbol(state->config_path, key,
                                                   change->text);
      break;
    }
  }

  g_hash_table_remove_all(state->pending);

  if (wrote && state->reload)
    state->reload(state->reload_data);

  return G_SOURCE_REMOVE;
}

static void queue_change(PrefsState *state, const char *key,
                         PendingChange *change) {
  if (state->loading) {
    pending_change_free(change);
    return;
  }

  g_hash_table_replace(state->pending, g_strdup(key), change);

  if (state->apply_id == 0)
    state->apply_id = g_timeout_add(APPLY_DEBOUNCE_MS, apply_pending, state);
}

static void queue_bool(PrefsState *state, const char *key, gboolean value) {
  PendingChange *change = g_new0(PendingChange, 1);

  change->kind = CHANGE_BOOL;
  change->boolean_value = value;
  queue_change(state, key, change);
}

static void queue_int(PrefsState *state, const char *key, int value) {
  PendingChange *change = g_new0(PendingChange, 1);

  change->kind = CHANGE_INT;
  change->int_value = value;
  queue_change(state, key, change);
}

static void queue_double(PrefsState *state, const char *key, double value) {
  PendingChange *change = g_new0(PendingChange, 1);

  change->kind = CHANGE_DOUBLE;
  change->double_value = value;
  queue_change(state, key, change);
}

static void queue_text(PrefsState *state, const char *key, const char *text,
                       ChangeKind kind) {
  PendingChange *change;

  if (!text || *text == '\0')
    return;

  change = g_new0(PendingChange, 1);
  change->kind = kind;
  change->text = g_strdup(text);
  queue_change(state, key, change);
}

/* =========================================================================
 * ROWS AND CARDS
 * ========================================================================= */

static GtkWidget *text_label(const char *text, const char *css_class) {
  GtkWidget *label = gtk_label_new(text);

  gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
  gtk_label_set_wrap(GTK_LABEL(label), TRUE);
  gtk_label_set_wrap_mode(GTK_LABEL(label), PANGO_WRAP_WORD_CHAR);
  gtk_widget_add_css_class(label, css_class);

  return label;
}

static GtkWidget *card_new(const char *title, const char *subtitle) {
  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  GtkWidget *heading = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  gtk_widget_add_css_class(card, "settings-card");
  gtk_widget_add_css_class(heading, "settings-card-heading");

  gtk_box_append(GTK_BOX(heading), text_label(title, "settings-card-title"));

  if (subtitle && *subtitle)
    gtk_box_append(GTK_BOX(heading),
                   text_label(subtitle, "settings-card-subtitle"));

  gtk_box_append(GTK_BOX(card), heading);
  return card;
}

static void row_add(GtkWidget *card, const char *title, const char *subtitle,
                    GtkWidget *control) {
  GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 16);
  GtkWidget *text = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

  gtk_widget_add_css_class(row, "settings-row");
  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(text), text_label(title, "settings-row-title"));

  if (subtitle && *subtitle)
    gtk_box_append(GTK_BOX(text),
                   text_label(subtitle, "settings-row-subtitle"));

  gtk_box_append(GTK_BOX(row), text);

  if (control) {
    gtk_widget_set_valign(control, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(row), control);
  }

  gtk_box_append(GTK_BOX(card), row);
}

static void card_note(GtkWidget *card, const char *text) {
  GtkWidget *note = text_label(text, "settings-note");

  gtk_widget_set_margin_top(note, 10);
  gtk_box_append(GTK_BOX(card), note);
}

static void set_preference_key(GtkWidget *widget, const char *key) {
  g_object_set_data_full(G_OBJECT(widget), "whelk-pref-key", g_strdup(key),
                         g_free);
}

static const char *preference_key(GtkWidget *widget) {
  return g_object_get_data(G_OBJECT(widget), "whelk-pref-key");
}

/* A module page with no module on this bar says so instead of pretending the
 * switch does something. */
static gboolean row_module_available(GtkWidget *card, const PrefsState *state,
                                     const char *kind, const char *label) {
  if (state->bar && has_module(state->bar, kind))
    return TRUE;

  g_autofree char *note =
      g_strdup_printf("This bar has no %s module; the setting will apply once "
                      "one is added to the configuration.",
                      label);
  card_note(card, note);
  return FALSE;
}

/* =========================================================================
 * CONTROL CALLBACKS
 * ========================================================================= */

static void on_switch_notify(GObject *object, GParamSpec *pspec,
                             gpointer user_data) {
  PrefsState *state = user_data;
  const char *key = preference_key(GTK_WIDGET(object));

  (void)pspec;

  if (key)
    queue_bool(state, key, gtk_switch_get_active(GTK_SWITCH(object)));
}

static void on_spin_changed(GtkSpinButton *spin, gpointer user_data) {
  PrefsState *state = user_data;
  const char *key = preference_key(GTK_WIDGET(spin));

  if (key)
    queue_int(state, key, gtk_spin_button_get_value_as_int(spin));
}

static void on_scale_changed(GtkRange *range, gpointer user_data) {
  PrefsState *state = user_data;
  const char *key = preference_key(GTK_WIDGET(range));

  if (key)
    queue_double(state, key, gtk_range_get_value(range));
}

static void on_color_notify(GObject *object, GParamSpec *pspec,
                            gpointer user_data) {
  PrefsState *state = user_data;
  const char *key = preference_key(GTK_WIDGET(object));
  const GdkRGBA *rgba =
      gtk_color_dialog_button_get_rgba(GTK_COLOR_DIALOG_BUTTON(object));

  (void)pspec;

  if (key && rgba) {
    g_autofree char *text = gdk_rgba_to_string(rgba);

    queue_text(state, key, text, CHANGE_STRING);
  }
}

static void on_entry_changed(GtkEditable *editable, gpointer user_data) {
  PrefsState *state = user_data;
  GtkWidget *widget = GTK_WIDGET(editable);
  const char *key = preference_key(widget);

  if (key)
    queue_text(state, key, gtk_editable_get_text(editable), CHANGE_STRING);
}

static void on_dropdown_notify(GObject *object, GParamSpec *pspec,
                               gpointer user_data) {
  PrefsState *state = user_data;
  GtkWidget *widget = GTK_WIDGET(object);
  const char *key = preference_key(widget);
  char **values = g_object_get_data(G_OBJECT(widget), "whelk-pref-values");
  guint selected = gtk_drop_down_get_selected(GTK_DROP_DOWN(widget));

  (void)pspec;

  if (key && values && values[selected])
    queue_text(state, key, values[selected], CHANGE_SYMBOL);
}

/* =========================================================================
 * CONTROL BUILDERS
 * ========================================================================= */

static GtkWidget *row_switch(PrefsState *state, GtkWidget *card,
                             const char *title, const char *subtitle,
                             const char *key, gboolean value,
                             const char *module_kind, const char *module_label) {
  GtkWidget *control = gtk_switch_new();
  gboolean available = TRUE;

  if (module_kind)
    available = row_module_available(card, state, module_kind, module_label);

  gtk_switch_set_active(GTK_SWITCH(control), value);
  gtk_widget_set_sensitive(control, available);
  set_preference_key(control, key);
  g_signal_connect(control, "notify::active",
                   G_CALLBACK(on_switch_notify), state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *row_spin(PrefsState *state, GtkWidget *card,
                           const char *title, const char *subtitle,
                           const char *key, int value, int minimum,
                           int maximum) {
  GtkWidget *control = gtk_spin_button_new_with_range(minimum, maximum, 1);

  gtk_spin_button_set_value(GTK_SPIN_BUTTON(control), value);
  gtk_spin_button_set_numeric(GTK_SPIN_BUTTON(control), TRUE);
  gtk_editable_set_width_chars(GTK_EDITABLE(control), 5);
  set_preference_key(control, key);
  g_signal_connect(control, "value-changed", G_CALLBACK(on_spin_changed),
                   state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *row_scale(PrefsState *state, GtkWidget *card,
                            const char *title, const char *subtitle,
                            const char *key, double value, double minimum,
                            double maximum, double step, int digits) {
  GtkWidget *control =
      gtk_scale_new_with_range(GTK_ORIENTATION_HORIZONTAL, minimum, maximum,
                               step);

  gtk_range_set_value(GTK_RANGE(control), value);
  gtk_scale_set_digits(GTK_SCALE(control), digits);
  gtk_scale_set_value_pos(GTK_SCALE(control), GTK_POS_RIGHT);
  gtk_widget_set_size_request(control, 220, -1);
  set_preference_key(control, key);
  g_signal_connect(control, "value-changed", G_CALLBACK(on_scale_changed),
                   state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *row_color(PrefsState *state, GtkWidget *card,
                            const char *title, const char *subtitle,
                            const char *key, const char *value) {
  GtkColorDialog *dialog = gtk_color_dialog_new();
  GtkWidget *control = gtk_color_dialog_button_new(dialog);
  GdkRGBA rgba;

  gtk_color_dialog_set_with_alpha(dialog, TRUE);

  if (!value || !gdk_rgba_parse(&rgba, value)) {
    rgba.red = 0.0f;
    rgba.green = 0.0f;
    rgba.blue = 0.0f;
    rgba.alpha = 1.0f;
  }

  gtk_color_dialog_button_set_rgba(GTK_COLOR_DIALOG_BUTTON(control), &rgba);
  gtk_widget_set_tooltip_text(control, "Click to choose a colour");
  set_preference_key(control, key);
  g_signal_connect(control, "notify::rgba", G_CALLBACK(on_color_notify),
                   state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *row_entry(PrefsState *state, GtkWidget *card,
                            const char *title, const char *subtitle,
                            const char *key, const char *value) {
  GtkWidget *control = gtk_entry_new();

  gtk_editable_set_text(GTK_EDITABLE(control), value ? value : "");
  gtk_editable_set_width_chars(GTK_EDITABLE(control), 18);
  gtk_widget_set_size_request(control, 200, -1);
  set_preference_key(control, key);
  g_signal_connect(control, "changed", G_CALLBACK(on_entry_changed), state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *row_dropdown(PrefsState *state, GtkWidget *card,
                               const char *title, const char *subtitle,
                               const char *key, const char *const *labels,
                               const char *const *values, const char *current) {
  GtkWidget *control = gtk_drop_down_new_from_strings(labels);
  gsize count = 0;
  guint selected = 0;

  while (values[count])
    count++;

  for (gsize i = 0; values[i]; i++) {
    if (g_strcmp0(values[i], current) == 0) {
      selected = (guint)i;
      break;
    }
  }

  gtk_drop_down_set_selected(GTK_DROP_DOWN(control), selected);

  char **copy = g_new0(char *, count + 1);
  for (gsize i = 0; values[i]; i++)
    copy[i] = g_strdup(values[i]);

  /* g_strdupv needs a NULL-terminated char **; copy is exactly that. */
  g_object_set_data_full(G_OBJECT(control), "whelk-pref-values", copy,
                         (GDestroyNotify)g_strfreev);
  set_preference_key(control, key);
  g_signal_connect(control, "notify::selected",
                   G_CALLBACK(on_dropdown_notify), state);
  row_add(card, title, subtitle, control);

  return control;
}

static GtkWidget *theme_dropdown(PrefsState *state, GtkWidget *card,
                                 const char *title, const char *subtitle,
                                 const char *key) {
  GPtrArray *names = whelk_bridge_themes();
  GPtrArray *labels = g_ptr_array_new_with_free_func(g_free);
  const char *current = state->bar && state->bar->theme ? state->bar->theme : "";

  for (guint i = 0; i < names->len; i++)
    g_ptr_array_add(labels, g_strdup(g_ptr_array_index(names, i)));

  if (labels->len == 0)
    g_ptr_array_add(labels, g_strdup("gloss-2007"));

  gboolean found = FALSE;
  for (guint i = 0; i < labels->len; i++) {
    if (g_strcmp0(g_ptr_array_index(labels, i), current) == 0) {
      found = TRUE;
      break;
    }
  }

  if (!found && *current)
    g_ptr_array_add(labels, g_strdup(current));

  const char **label_array = g_new0(const char *, labels->len + 1);
  const char **value_array = g_new0(const char *, labels->len + 1);

  for (guint i = 0; i < labels->len; i++) {
    label_array[i] = g_ptr_array_index(labels, i);
    value_array[i] = g_ptr_array_index(labels, i);
  }

  GtkWidget *control = row_dropdown(state, card, title, subtitle, key,
                                    label_array, value_array, current);

  g_free(label_array);
  g_free(value_array);
  g_ptr_array_unref(labels);
  g_ptr_array_unref(names);

  return control;
}

/* =========================================================================
 * PAGES
 * ========================================================================= */

static GtkWidget *page_body(GtkStack *stack, const char *name,
                            const char *title) {
  GtkWidget *scroller = gtk_scrolled_window_new();
  GtkWidget *body = gtk_box_new(GTK_ORIENTATION_VERTICAL, 14);

  gtk_widget_add_css_class(body, "settings-page");
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroller),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroller), body);
  gtk_stack_add_titled(stack, scroller, name, title);

  return body;
}

static void build_bar_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "bar", "Bar");
  GtkWidget *placement = card_new(
      "Placement", "Where the bar sits and how much room it takes.");
  static const char *const edge_labels[] = {"Top", "Bottom", "Left", "Right",
                                            NULL};
  static const char *const edge_values[] = {"top", "bottom", "left", "right",
                                             NULL};
  static const char *const layer_labels[] = {"Top layer", "Bottom layer",
                                             "Background", "Overlay", NULL};
  static const char *const layer_values[] = {"top", "bottom", "background",
                                              "overlay", NULL};
  WhelkBarConfig *bar = state->bar;

  gtk_box_append(GTK_BOX(body), placement);
  row_dropdown(state, placement, "Edge", "The screen edge the bar belongs to.",
               "bar-edge", edge_labels, edge_values,
               bar ? whelk_edge_name(bar->edge) : "top");
  row_dropdown(state, placement, "Layer",
               "Top stays over the overview; bottom moves with a workspace.",
               "bar-layer", layer_labels, layer_values,
               bar && bar->layer == GTK_LAYER_SHELL_LAYER_BOTTOM
                   ? "bottom"
                   : (bar && bar->layer == GTK_LAYER_SHELL_LAYER_BACKGROUND
                          ? "background"
                          : (bar && bar->layer == GTK_LAYER_SHELL_LAYER_OVERLAY
                                 ? "overlay"
                                 : "top")));
  row_switch(state, placement, "Reserve space",
             "Keep windows out of the bar's strip of the screen.",
             "bar-exclusive?", bar ? bar->exclusive : TRUE, NULL, NULL);
  row_spin(state, placement, "Height", "Thickness in pixels.", "bar-height",
           bar ? bar->height : 32, 20, 128);
  row_spin(state, placement, "Module spacing", "Gap between items.",
           "bar-spacing", bar ? bar->spacing : 6, 0, 64);
  row_spin(state, placement, "Padding", "Space at the bar's two ends.",
           "bar-padding", bar ? bar->padding : 12, 0, 96);

  GtkWidget *type = card_new(
      "Typography", "The text face used by the bar's readings and menus.");

  gtk_box_append(GTK_BOX(body), type);
  row_entry(state, type, "Font family", "For example Inter or Cantarell.",
            "bar-font", bar ? bar->font : "Inter");
  row_entry(state, type, "Font size", "A CSS/Pango size, for example 11pt.",
            "bar-font-size", bar ? bar->font_size : "11pt");
}

static void build_theme_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "theme", "Theme");
  GtkWidget *material = card_new(
      "Material", "The palette, the light on the surface and how much of the "
                  "desktop shows through it.");
  WhelkBarConfig *bar = state->bar;

  gtk_box_append(GTK_BOX(body), material);
  theme_dropdown(state, material, "Theme",
                 "The named material the bar is made from.", "bar-theme");
  card_note(material,
            "The theme supplies the starting colours and material numbers; "
            "every control below layers on top of it.");

  GtkWidget *colours = card_new(
      "Colours", "The bar's ink, surface, accent and hairline.  A theme "
                 "fills these in until one is chosen here.");

  gtk_box_append(GTK_BOX(body), colours);
  row_color(state, colours, "Surface",
            "The material's background colour; alpha is kept.", "bar-background",
            bar ? bar->background : NULL);
  row_color(state, colours, "Ink", "Text and glyphs.", "bar-foreground",
            bar ? bar->foreground : NULL);
  row_color(state, colours, "Accent", "Active workspaces and highlights.",
            "bar-accent", bar ? bar->accent : NULL);
  row_color(state, colours, "Hairline", "The one-pixel border.",
            "bar-border-color", bar ? bar->border_color : NULL);

  GtkWidget *light = card_new(
      "Light and depth", "The generated surface: transparency, shadow, top "
                         "shine, lower reflection and the accent flare.");

  gtk_box_append(GTK_BOX(body), light);
  row_scale(state, light, "Opacity",
            "Multiplies the surface colour's own alpha.  1 is the theme's "
            "designed opacity.",
            "bar-opacity", bar ? bar->opacity : 1.0, 0.2, 1.0, 0.01, 2);
  row_scale(state, light, "Shadow blur",
            "Softness of the shadow under the surface, in pixels.", "bar-blur",
            bar ? bar->blur : 0, 0, 64, 1, 0);
  row_scale(state, light, "Shine",
            "Strength of the light falling on the top edge.", "bar-shine",
            bar ? bar->shine : 0.0, 0.0, 1.0, 0.01, 2);
  row_scale(state, light, "Reflection",
            "Strength of the colour reflected up from the lower edge.",
            "bar-reflection", bar ? bar->reflection : 0.0, 0.0, 1.0, 0.01,
            2);
  row_scale(state, light, "Flare",
            "Strength of the accent glow along the top edge.", "bar-flare",
            bar ? bar->flare : 0.0, 0.0, 1.0, 0.01, 2);
  row_spin(state, light, "Corner radius",
           "The surface's corner radius in pixels.", "bar-radius",
           bar ? bar->radius : 0, 0, 48);

  GtkWidget *about = card_new(
      "How overrides work",
      "A GUI choice is saved separately from your configuration.");
  gtk_box_append(GTK_BOX(body), about);
  card_note(about,
            "Changes are written under your XDG state directory in a "
            "preferences file. Your settings.scm is never edited. Reset removes "
            "the overrides and returns to the configuration exactly as it is.");
}

static void build_notifications_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "notifications", "Notifications");
  GtkWidget *bell = card_new(
      "The bar's bell", "How the notification service appears on the bar.");

  gtk_box_append(GTK_BOX(body), bell);
  const WhelkModuleSpec *notifications =
      state->bar ? find_spec(state->bar, "notifications") : NULL;
  gboolean show_count =
      notifications &&
      strstr(whelk_spec_str(notifications, "format", "{icon}"), "{count}");

  row_switch(state, bell, "Show the notification count",
             "Draw the number of waiting notifications beside the bell.",
             "notifications-show-count?", show_count, "notifications",
             "notification");
  card_note(bell,
            "Do Not Disturb and the notification centre are available from the "
            "bar's bell and the shell menu; this page controls the reading at "
            "rest.");
}

static void build_wifi_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "wifi", "Wi-Fi");
  GtkWidget *wifi = card_new("Network reading",
                             "The Wi-Fi indicator's name and refresh rate.");
  const WhelkModuleSpec *spec = state->bar ? find_spec(state->bar, "wifi") : NULL;

  gtk_box_append(GTK_BOX(body), wifi);
  row_switch(state, wifi, "Show the network name",
             "Replace the bare Wi-Fi glyph with the SSID you are joined to.",
             "wifi-show-ssid?",
             spec ? whelk_spec_bool(spec, "show-ssid?", FALSE) : FALSE, "wifi",
             "Wi-Fi");
  row_spin(state, wifi, "Refresh interval",
           "Seconds between scans; a longer interval costs less power.",
           "wifi-interval", spec ? whelk_spec_int(spec, "interval", 5) : 5, 1,
           300);
  card_note(wifi,
            "The network panel, connect and disconnect are reached by clicking "
            "the Wi-Fi reading or from the shell menu.");
}

static void build_menu_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "menu", "Global menu");
  GtkWidget *menu = card_new(
      "The application menu",
      "The File and Edit menus that follow the focused window on the bar.");
  static const char *const style_labels[] = {"CUA (Ctrl+C)", "Emacs (C-c)",
                                              NULL};
  static const char *const style_values[] = {"cua", "emacs", NULL};
  const WhelkModuleSpec *spec = state->bar ? find_spec(state->bar, "menu") : NULL;

  gtk_box_append(GTK_BOX(body), menu);
  row_switch(state, menu, "Show shortcut menus",
             "Show each application's own menus beside the shell menu.",
             "menu-shortcuts?",
             spec ? whelk_spec_bool(spec, "shortcuts?", TRUE) : TRUE, "menu",
             "global menu");
  row_switch(state, menu, "Show the application name",
             "Put the focused window's name before its menus.",
             "menu-app-name?",
             spec ? whelk_spec_bool(spec, "app-name?", TRUE) : TRUE, "menu",
             "global menu");
  row_dropdown(state, menu, "Shortcut style",
               "How accelerators are written in the menus.", "menu-accelerator-style",
               style_labels, style_values,
               spec ? whelk_spec_str(spec, "accelerator-style", "cua") : "cua");
  card_note(menu,
            "The menu reads the application's exported menu over D-Bus. An "
            "application without one gets whelk's shared Edit and View menus.");
}

static void build_more_page(PrefsState *state, GtkStack *stack) {
  GtkWidget *body = page_body(stack, "more", "More");
  GtkWidget *tray = card_new("Tray", "The status icons hosted at the end of "
                                     "the bar.");
  const WhelkModuleSpec *tray_spec =
      state->bar ? find_spec(state->bar, "tray") : NULL;

  gtk_box_append(GTK_BOX(body), tray);
  row_spin(state, tray, "Maximum items", "Icons shown before the overflow "
                                         "chevron takes over.",
           "tray-max-items",
           tray_spec ? whelk_spec_int(tray_spec, "max-items", 6) : 6, 1, 32);

  GtkWidget *clock = card_new("Clock", "The time reading in the bar.");
  const WhelkModuleSpec *clock_spec =
      state->bar ? find_spec(state->bar, "clock") : NULL;

  gtk_box_append(GTK_BOX(body), clock);
  row_entry(state, clock, "Format", "A strftime(3) format.",
            "clock-format",
            clock_spec ? whelk_spec_str(clock_spec, "format",
                                       "%a %d %b  %H:%M")
                       : "%a %d %b  %H:%M");
  card_note(clock,
            "The clock's click and right-click actions, calendar tooltip and "
            "alternate format stay in settings.scm.");
}

/* =========================================================================
 * THE WINDOW'S CONTENT
 * ========================================================================= */

static void clear_host(PrefsState *state) {
  GtkWidget *child;

  while ((child = gtk_widget_get_first_child(state->host)) != NULL)
    gtk_widget_unparent(child);
}

static void on_close_clicked(GtkButton *button, gpointer user_data) {
  PrefsState *state = user_data;

  (void)button;

  if (state->window)
    gtk_window_destroy(GTK_WINDOW(state->window));
}

/* build_ui is defined below; the reset handler needs it. */
static void build_ui(PrefsState *state);

static void on_reset_clicked(GtkButton *button, gpointer user_data) {
  PrefsState *state = user_data;

  (void)button;

  if (state->apply_id != 0) {
    g_source_remove(state->apply_id);
    state->apply_id = 0;
  }

  g_hash_table_remove_all(state->pending);

  if (!whelk_bridge_reset_preferences(state->config_path))
    LOG_C_WARN("Preferences", "could not reset GUI preferences");

  if (state->reload)
    state->reload(state->reload_data);

  g_clear_pointer(&state->bar, whelk_bar_config_free);
  state->bar = load_current_bar(state->config_path);

  state->loading = TRUE;
  clear_host(state);
  build_ui(state);
  state->loading = FALSE;
}

static GtkWidget *header_new(void) {
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
  GtkWidget *title_row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);

  gtk_widget_add_css_class(header, "settings-header");
  gtk_widget_add_css_class(title_row, "settings-title-row");
  gtk_box_append(GTK_BOX(title_row), text_label("whelk settings",
                                                 "settings-heading"));
  GtkWidget *badge = text_label("GUI OVERRIDES", "settings-badge");
  gtk_widget_set_halign(badge, GTK_ALIGN_END);
  gtk_widget_set_hexpand(badge, TRUE);
  gtk_box_append(GTK_BOX(title_row), badge);
  gtk_box_append(GTK_BOX(header), title_row);
  gtk_box_append(GTK_BOX(header),
                 text_label("Shape the bar without editing your configuration. "
                            "Changes are saved separately and win when the bar "
                            "reloads; settings.scm is never rewritten.",
                            "settings-subtitle"));

  return header;
}

static GtkWidget *footer_new(PrefsState *state) {
  GtkWidget *footer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
  GtkWidget *note = text_label("Reset returns to settings.scm exactly as it is.",
                               "settings-footer-note");
  GtkWidget *reset = gtk_button_new_with_label("Reset GUI settings");
  GtkWidget *close = gtk_button_new_with_label("Close");

  gtk_widget_add_css_class(footer, "settings-footer");
  gtk_widget_set_hexpand(note, TRUE);
  gtk_box_append(GTK_BOX(footer), note);

  gtk_widget_add_css_class(reset, "destructive-action");
  gtk_widget_add_css_class(close, "suggested-action");
  g_signal_connect(reset, "clicked", G_CALLBACK(on_reset_clicked),
                   state);
  g_signal_connect(close, "clicked", G_CALLBACK(on_close_clicked), state);
  gtk_box_append(GTK_BOX(footer), reset);
  gtk_box_append(GTK_BOX(footer), close);

  return footer;
}

static void build_ui(PrefsState *state) {
  GtkWidget *stack = gtk_stack_new();
  GtkWidget *switcher = gtk_stack_switcher_new();

  gtk_widget_add_css_class(stack, "settings-stack");
  gtk_stack_set_transition_type(GTK_STACK(stack),
                                GTK_STACK_TRANSITION_TYPE_CROSSFADE);
  gtk_stack_set_transition_duration(GTK_STACK(stack), 160);
  gtk_stack_switcher_set_stack(GTK_STACK_SWITCHER(switcher),
                               GTK_STACK(stack));
  gtk_widget_set_halign(switcher, GTK_ALIGN_START);

  build_bar_page(state, GTK_STACK(stack));
  build_theme_page(state, GTK_STACK(stack));
  build_notifications_page(state, GTK_STACK(stack));
  build_wifi_page(state, GTK_STACK(stack));
  build_menu_page(state, GTK_STACK(stack));
  build_more_page(state, GTK_STACK(stack));

  gtk_box_append(GTK_BOX(state->host), header_new());
  gtk_box_append(GTK_BOX(state->host), switcher);
  gtk_box_append(GTK_BOX(state->host), stack);
  gtk_box_append(GTK_BOX(state->host), footer_new(state));

  gtk_widget_set_vexpand(stack, TRUE);
}

GtkWidget *whelk_preferences_content_for(const char *config_path,
                                         WhelkPreferencesReloadFunc reload,
                                         gpointer reload_data) {
  PrefsState *state = g_new0(PrefsState, 1);
  GtkWidget *root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  GtkWidget *host = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

  state->config_path = g_strdup(config_path ? config_path : "");
  state->reload = reload;
  state->reload_data = reload_data;
  state->bar = load_current_bar(config_path);
  state->pending = g_hash_table_new_full(
      g_str_hash, g_str_equal, g_free, (GDestroyNotify)pending_change_free);
  state->host = host;
  state->loading = TRUE;

  gtk_widget_add_css_class(root, "whelk-settings");
  gtk_widget_add_css_class(host, "settings-host");
  gtk_widget_set_vexpand(host, TRUE);
  gtk_box_append(GTK_BOX(root), host);

  build_ui(state);
  state->loading = FALSE;

  g_object_set_data_full(G_OBJECT(root), "whelk-prefs-state", state,
                         (GDestroyNotify)prefs_state_free);

  return root;
}

/* =========================================================================
 * THE WINDOW
 * ========================================================================= */

static gboolean on_key_pressed(GtkEventControllerKey *controller,
                               guint keyval, guint keycode,
                               GdkModifierType modifiers, gpointer user_data) {
  GtkWindow *window = user_data;

  (void)controller;
  (void)keycode;
  (void)modifiers;

  if (keyval == GDK_KEY_Escape) {
    gtk_window_destroy(window);
    return TRUE;
  }

  return FALSE;
}

static void on_settings_window_destroy(GtkWidget *widget, gpointer user_data) {
  (void)widget;
  (void)user_data;

  settings_window = NULL;
}

void whelk_preferences_show(GtkApplication *app, const char *config_path,
                            WhelkPreferencesReloadFunc reload,
                            gpointer reload_data) {
  if (settings_window) {
    gtk_window_present(settings_window);
    return;
  }

  GtkWidget *content =
      whelk_preferences_content_for(config_path, reload, reload_data);
  GtkWidget *window = gtk_window_new();
  PrefsState *state =
      g_object_get_data(G_OBJECT(content), "whelk-prefs-state");

  gtk_window_set_application(GTK_WINDOW(window), app);
  gtk_window_set_title(GTK_WINDOW(window), "whelk settings");
  gtk_window_set_default_size(GTK_WINDOW(window), 760, 760);
  gtk_widget_add_css_class(window, "whelk-settings-window");
  gtk_window_set_child(GTK_WINDOW(window), content);

  state->window = window;

  GtkEventController *keys = gtk_event_controller_key_new();
  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_key_pressed), window);
  gtk_widget_add_controller(window, keys);
  g_signal_connect(window, "destroy",
                   G_CALLBACK(on_settings_window_destroy), NULL);

  settings_window = GTK_WINDOW(window);
  gtk_window_present(GTK_WINDOW(window));
}

void whelk_preferences_shutdown(void) {
  if (settings_window) {
    GtkWindow *window = settings_window;

    settings_window = NULL;
    gtk_window_destroy(window);
  }
}

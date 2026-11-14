// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "style.h"

#include "logger.h"

/* A button is anything GTK paints as one: a plain button, a menu button or a
 * check button.  A switch is a control, not a button, and keeps its arrow. */
static gboolean is_button(GtkWidget *widget) {
  return GTK_IS_BUTTON(widget) || GTK_IS_MENU_BUTTON(widget) ||
         GTK_IS_CHECK_BUTTON(widget);
}

static void apply_pointer_cursors(GtkWidget *widget);

/* A popover is its own surface, so its children are not reached by the
 * controller on the window that opened it.  Rescanning on map also catches
 * menu rows that a module builds only when the menu is opened. */
static void on_popover_map(GtkWidget *popover, gpointer user_data) {
  (void)user_data;

  apply_pointer_cursors(popover);
}

static void apply_pointer_cursors(GtkWidget *widget) {
  if (!widget)
    return;

  if (is_button(widget)) {
    gtk_widget_set_cursor_from_name(
        widget, gtk_widget_get_sensitive(widget) ? "pointer" : NULL);
  }

  if (GTK_IS_POPOVER(widget) &&
      !g_object_get_data(G_OBJECT(widget), "whelk-pointer-cursors")) {
    g_object_set_data(G_OBJECT(widget), "whelk-pointer-cursors",
                      GINT_TO_POINTER(1));
    g_signal_connect(widget, "map", G_CALLBACK(on_popover_map), NULL);
  }

  for (GtkWidget *child = gtk_widget_get_first_child(widget); child;
       child = gtk_widget_get_next_sibling(child))
    apply_pointer_cursors(child);
}

void whelk_style_pointer_cursors(GtkWidget *root) {
  apply_pointer_cursors(root);
}

char *whelk_style_widget_id(const char *name) {
  GString *id = g_string_new("whelk-");
  const char *cursor = (name && *name) ? name : "surface";

  for (const char *p = cursor; *p; p++)
    g_string_append_c(id, (g_ascii_isalnum(*p) || *p == '-' || *p == '_')
                              ? *p
                              : '-');

  return g_string_free(id, FALSE);
}

/* The colour as CSS, with its alpha multiplied by SCALE.  GTK's parser is the
 * only one that has to understand the configuration, so the output is always
 * in the rgba() form GTK wrote itself.  A value the parser cannot read is
 * left alone: the stylesheet's cascade can still make sense of it. */
static char *color_with_alpha(const char *color, double scale) {
  GdkRGBA rgba;

  if (!color || !*color)
    return g_strdup("transparent");

  if (!gdk_rgba_parse(&rgba, color))
    return g_strdup(color);

  rgba.alpha = CLAMP(rgba.alpha * CLAMP(scale, 0.0, 1.0), 0.0, 1.0);

  return gdk_rgba_to_string(&rgba);
}

static void append_shadow(GString *css, const WhelkBarConfig *config) {
  gboolean comma = FALSE;

  if (config->shine <= 0.0 && config->blur <= 0)
    return;

  g_string_append(css, "  box-shadow:");

  if (config->shine > 0.0) {
    g_string_append_printf(css, " inset 0 1px rgba(255, 255, 255, %.3f)",
                           CLAMP(config->shine * 0.34, 0.0, 1.0));
    comma = TRUE;
  }

  if (config->blur > 0) {
    g_string_append_printf(css, "%s 0 4px %dpx rgba(0, 0, 0, 0.30)",
                           comma ? "," : "", config->blur);
  }

  g_string_append(css, ";\n");
}

/* The material effects are layers rather than a fixed theme gradient, so the
 * opacity, shine, reflection and flare controls can be independent.  A theme
 * supplies their defaults; a bar or the settings window can move one without
 * losing the others. */
static void append_effects(GString *css, const WhelkBarConfig *config) {
  GString *layers = g_string_new(NULL);
  const char *accent = (config->accent && *config->accent) ? config->accent
                                                            : "#ffffff";

  if (config->flare > 0.0) {
    g_autofree char *start = color_with_alpha(accent, config->flare * 0.30);
    g_autofree char *end = color_with_alpha(accent, 0.0);

    g_string_append_printf(
        layers,
        "    radial-gradient(ellipse at 50%% 0%%, %s 0%%, %s 48%%)", start,
        end);
  }

  if (config->shine > 0.0) {
    g_autofree char *start =
        color_with_alpha("#ffffff", config->shine * 0.18);
    g_autofree char *end = color_with_alpha("#ffffff", 0.0);

    if (layers->len > 0)
      g_string_append(layers, ",\n");

    g_string_append_printf(layers,
                           "    linear-gradient(to bottom, %s 0%%, %s 36%%)",
                           start, end);
  }

  if (config->reflection > 0.0) {
    g_autofree char *start =
        color_with_alpha("#ffffff", config->reflection * 0.10);
    g_autofree char *end = color_with_alpha("#ffffff", 0.0);

    if (layers->len > 0)
      g_string_append(layers, ",\n");

    g_string_append_printf(layers,
                           "    linear-gradient(to top, %s 0%%, %s 32%%)",
                           start, end);
  }

  if (layers->len > 0)
    g_string_append_printf(css, "  background-image:\n%s;\n", layers->str);

  g_string_free(layers, TRUE);
}

static void append_material(GString *css, const WhelkBarConfig *config,
                            const char *selector) {
  g_autofree char *background =
      color_with_alpha(config->background, config->opacity);

  g_string_append_printf(css, "%s {\n", selector);

  if (config->foreground && *config->foreground)
    g_string_append_printf(css, "  color: %s;\n", config->foreground);

  if (config->background && *config->background)
    g_string_append_printf(css, "  background-color: %s;\n", background);

  append_effects(css, config);

  if (config->border_color && *config->border_color)
    g_string_append_printf(css, "  border: 1px solid %s;\n",
                           config->border_color);

  g_string_append_printf(css, "  border-radius: %dpx;\n", config->radius);

  if (config->font && *config->font)
    g_string_append_printf(css, "  font-family: \"%s\";\n", config->font);

  if (config->font_size && *config->font_size)
    g_string_append_printf(css, "  font-size: %s;\n", config->font_size);

  append_shadow(css, config);
  g_string_append(css, "}\n");
}

char *whelk_style_css(const char *widget_id, const WhelkBarConfig *config,
                      const char *root_class, const char *extra_css) {
  GString *css = g_string_new(NULL);
  g_autofree char *root = g_strdup_printf("window#%s .%s", widget_id,
                                          root_class);

  if (!config)
    return g_string_free(css, FALSE);

  append_material(css, config, root);

  /* The bar's own padding is its material's business; a panel manages its own
   * inner spacing and should not inherit the bar's. */
  if (g_strcmp0(root_class, "whelk-bar") == 0)
    g_string_append_printf(css, "%s { padding: 0 %dpx; }\n", root,
                           config->padding);

  if (config->icon_font && *config->icon_font) {
    g_string_append_printf(
        css, "window#%s .whelk-icon {\n  font-family: \"%s\";\n}\n", widget_id,
        config->icon_font);
  }

  if (config->accent && *config->accent) {
    g_string_append_printf(
        css,
        "window#%s .whelk-workspace.active { color: %s; }\n"
        "window#%s .whelk-battery.charging { color: %s; }\n"
        "window#%s .whelk-accent { color: %s; }\n"
        "window#%s button.accent { background-color: %s; color: %s; }\n",
        widget_id, config->accent, widget_id, config->accent, widget_id,
        config->accent, widget_id, config->accent, config->background
                                                                 ? config->background
                                                                 : "transparent");
  }

  if (extra_css && *extra_css) {
    g_string_append(css, "/* The configuration's own rules */\n");
    g_string_append(css, extra_css);
    g_string_append_c(css, '\n');
  }

  return g_string_free(css, FALSE);
}

GtkCssProvider *whelk_style_apply(const char *widget_id,
                                  const WhelkBarConfig *config,
                                  const char *root_class,
                                  const char *extra_css) {
  GtkCssProvider *provider = gtk_css_provider_new();
  g_autofree char *css = whelk_style_css(widget_id, config, root_class,
                                         extra_css);

  gtk_css_provider_load_from_string(provider, css);

  GdkDisplay *display = gdk_display_get_default();

  if (display)
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(provider),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
  else
    LOG_C_DEBUG("Style", "no display to install the skin on");

  return provider;
}

void whelk_style_detach(GtkCssProvider *provider, GdkDisplay *display) {
  if (!provider)
    return;

  if (display)
    gtk_style_context_remove_provider_for_display(
        display, GTK_STYLE_PROVIDER(provider));

  g_object_unref(provider);
}

/* -------------------------------------------------------------------------
 * POPOVERS
 * -------------------------------------------------------------------------
 * A popover is a separate surface, and its material is generated for that
 * popover alone.  The provider is scoped to a unique class and removed with
 * the popover: a theme change therefore cannot leave yesterday's rule
 * installed above today's on the display.
 */

typedef struct {
  GtkCssProvider *provider;
  GdkDisplay *display;
} PopoverStyle;

static GPtrArray *open_popovers;
static guint popover_serial;

static void forget_popover(gpointer data, GObject *where_the_object_was) {
  if (open_popovers)
    g_ptr_array_remove(open_popovers, where_the_object_was);

  if (open_popovers && open_popovers->len == 0)
    g_clear_pointer(&open_popovers, g_ptr_array_unref);

  (void)data;
}

static void popover_style_free(gpointer data) {
  PopoverStyle *style = data;

  if (!style)
    return;

  if (style->provider && style->display)
    gtk_style_context_remove_provider_for_display(
        style->display, GTK_STYLE_PROVIDER(style->provider));

  g_clear_object(&style->provider);
  g_clear_object(&style->display);
  g_free(style);
}

void whelk_style_popover(GtkPopover *popover, const WhelkBarConfig *config) {
  if (!popover)
    return;

  gtk_popover_set_has_arrow(popover, FALSE);
  gtk_widget_add_css_class(GTK_WIDGET(popover), "whelk-popover");
  whelk_style_pointer_cursors(GTK_WIDGET(popover));

  if (!open_popovers)
    open_popovers = g_ptr_array_new();

  if (!g_object_get_data(G_OBJECT(popover), "whelk-popover-registered")) {
    g_object_set_data(G_OBJECT(popover), "whelk-popover-registered",
                      GINT_TO_POINTER(1));
    g_ptr_array_add(open_popovers, popover);
    g_object_weak_ref(G_OBJECT(popover), forget_popover, NULL);
  }

  if (!config || g_object_get_data(G_OBJECT(popover), "whelk-popover-style"))
    return;

  const int radius = config->radius > 0 ? config->radius : 8;
  g_autofree char *class_name =
      g_strdup_printf("whelk-popover-%u", ++popover_serial);
  GString *css = g_string_new(NULL);
  g_autofree char *contents =
      g_strdup_printf("popover.%s > contents", class_name);
  g_autofree char *background =
      color_with_alpha(config->background, config->opacity);

  gtk_widget_add_css_class(GTK_WIDGET(popover), class_name);

  g_string_append_printf(css, "%s {\n", contents);

  if (config->background && *config->background)
    g_string_append_printf(css, "  background-color: %s;\n", background);

  append_effects(css, config);

  if (config->foreground && *config->foreground)
    g_string_append_printf(css, "  color: %s;\n", config->foreground);

  if (config->border_color && *config->border_color)
    g_string_append_printf(css, "  border: 1px solid %s;\n",
                           config->border_color);

  g_string_append_printf(css,
                         "  border-radius: %dpx;\n"
                         "  padding: 4px;\n"
                         "}\n",
                         radius);

  if (config->foreground && *config->foreground) {
    g_string_append_printf(
        css,
        "popover.%s button {\n"
        "  color: %s;\n"
        "  background: transparent;\n"
        "  border: none;\n"
        "  box-shadow: none;\n"
        "}\n"
        "popover.%s button:hover {\n"
        "  background-color: alpha(%s, 0.14);\n"
        "}\n",
        class_name, config->foreground, class_name, config->foreground);
  }

  PopoverStyle *style = g_new0(PopoverStyle, 1);
  GdkDisplay *display = gtk_widget_get_display(GTK_WIDGET(popover));

  style->provider = gtk_css_provider_new();
  gtk_css_provider_load_from_string(style->provider, css->str);

  if (display) {
    style->display = g_object_ref(display);
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(style->provider),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
  }

  g_object_set_data_full(G_OBJECT(popover), "whelk-popover-style", style,
                         popover_style_free);
  g_string_free(css, TRUE);
}

void whelk_style_close_popovers(void) {
  if (!open_popovers)
    return;

  for (guint i = 0; i < open_popovers->len; i++) {
    GtkWidget *popover = g_ptr_array_index(open_popovers, i);

    if (popover && gtk_widget_get_visible(popover))
      gtk_popover_popdown(GTK_POPOVER(popover));
  }
}

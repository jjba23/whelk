// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "style.h"

#include "logger.h"

char *whelk_style_widget_id(const char *name) {
  GString *id = g_string_new("whelk-");
  const char *cursor = (name && *name) ? name : "surface";

  for (const char *p = cursor; *p; p++)
    g_string_append_c(id, (g_ascii_isalnum(*p) || *p == '-' || *p == '_')
                              ? *p
                              : '-');

  return g_string_free(id, FALSE);
}

static void append_colors(GString *css, const WhelkBarConfig *config,
                          const char *selector) {
  g_string_append_printf(css, "%s {\n", selector);

  if (config->foreground && *config->foreground)
    g_string_append_printf(css, "  color: %s;\n", config->foreground);

  if (config->background && *config->background)
    g_string_append_printf(css, "  background-color: %s;\n",
                           config->background);

  if (config->border_color && *config->border_color)
    g_string_append_printf(css, "  border: 1px solid %s;\n",
                           config->border_color);

  g_string_append_printf(css, "  border-radius: %dpx;\n", config->radius);

  if (config->font && *config->font)
    g_string_append_printf(css, "  font-family: \"%s\";\n", config->font);

  if (config->font_size && *config->font_size)
    g_string_append_printf(css, "  font-size: %s;\n", config->font_size);

  g_string_append(css, "}\n");
}

GtkCssProvider *whelk_style_apply(const char *widget_id,
                                  const WhelkBarConfig *config,
                                  const char *root_class,
                                  const char *extra_css) {
  GtkCssProvider *provider = gtk_css_provider_new();
  GString *css = g_string_new(NULL);
  g_autofree char *root = g_strdup_printf("window#%s .%s", widget_id,
                                          root_class);

  append_colors(css, config, root);

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

  gtk_css_provider_load_from_string(provider, css->str);
  g_string_free(css, TRUE);

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

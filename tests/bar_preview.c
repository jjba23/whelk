// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "bar.h"
#include "config.h"

#include <gdk/gdk.h>
#include <gsk/gsk.h>
#include <gtk/gtk.h>

#include <math.h>
#include <string.h>

/* A bar whose modules say fixed words: the clock's format has no directives,
 * so the picture is the same tomorrow.  The surface is presented by a real
 * (Broadway) display server; the snapshot is taken from the bar's own widget,
 * not from a mock. */

typedef struct {
  GtkApplication *app;
  WhelkBar *bar;
  const char *output;
  const char *reference;
  guint attempts;
} Preview;

static void add_clock(GPtrArray *row, const char *text) {
  WhelkModuleSpec *spec = whelk_module_spec_new("clock");

  whelk_module_spec_set(spec, "format", text);
  whelk_module_spec_set(spec, "interval", "0");
  g_ptr_array_add(row, spec);
}

static WhelkBar *make_bar(GtkApplication *app) {
  WhelkBarConfig *config = whelk_bar_config_new("golden");

  config->height = 32;
  config->padding = 12;
  config->spacing = 8;
  config->background = g_strdup("#1e1e2e");
  config->foreground = g_strdup("#cdd6f4");
  config->border_color = g_strdup("#313244");
  config->font = g_strdup("Sans");
  config->font_size = g_strdup("11pt");

  add_clock(config->left, "whelk");
  add_clock(config->center, "golden");
  add_clock(config->right, "bar");

  WhelkBar *bar = whelk_bar_new(app, config, NULL);

  whelk_bar_config_free(config);
  return bar;
}

static gboolean compare_png(const char *output, const char *reference) {
  g_autoptr(GError) error = NULL;
  GdkTexture *actual = gdk_texture_new_from_filename(output, &error);

  if (!actual) {
    g_printerr("golden: cannot read %s: %s\n", output, error->message);
    return FALSE;
  }

  g_clear_error(&error);

  GdkTexture *expected = gdk_texture_new_from_filename(reference, &error);

  if (!expected) {
    g_printerr("golden: cannot read %s: %s\n", reference, error->message);
    g_object_unref(actual);
    return FALSE;
  }

  const int width = gdk_texture_get_width(actual);
  const int height = gdk_texture_get_height(actual);

  if (width != gdk_texture_get_width(expected) ||
      height != gdk_texture_get_height(expected)) {
    g_printerr("golden: size is %dx%d, reference is %dx%d\n", width, height,
               gdk_texture_get_width(expected), gdk_texture_get_height(expected));
    g_object_unref(actual);
    g_object_unref(expected);
    return FALSE;
  }

  const gsize stride = (gsize)width * 4;
  g_autofree guchar *actual_pixels = g_malloc(stride * (gsize)height);
  g_autofree guchar *expected_pixels = g_malloc(stride * (gsize)height);

  gdk_texture_download(actual, actual_pixels, stride);
  gdk_texture_download(expected, expected_pixels, stride);

  guint changed = 0;
  guint worst = 0;

  for (gsize i = 0; i < stride * (gsize)height; i += 4) {
    guint delta = 0;

    for (guint channel = 0; channel < 3; channel++) {
      const int a = actual_pixels[i + channel];
      const int b = expected_pixels[i + channel];
      const guint difference = (guint)ABS(a - b);

      delta = MAX(delta, difference);
    }

    worst = MAX(worst, delta);

    if (delta > 2)
      changed++;
  }

  g_object_unref(actual);
  g_object_unref(expected);

  const gsize pixels = (gsize)width * (gsize)height;
  const gsize allowed = MAX((gsize)4, pixels / 200);

  if (worst > 2 && changed > allowed) {
    g_printerr("golden: %" G_GSIZE_FORMAT " of %" G_GSIZE_FORMAT
               " pixels differ (worst channel delta %u)\n",
               (gsize)changed, pixels, worst);
    return FALSE;
  }

  g_print("golden: %" G_GSIZE_FORMAT " changed pixel(s), worst delta %u\n",
          (gsize)changed, worst);
  return TRUE;
}

static gboolean save_preview(gpointer data) {
  Preview *preview = data;
  GtkWidget *widget = whelk_bar_widget(preview->bar);

  if (!widget) {
    g_application_quit(G_APPLICATION(preview->app));
    return G_SOURCE_REMOVE;
  }

  const int width = gtk_widget_get_width(widget);
  const int height = gtk_widget_get_height(widget);

  /* The first allocation is the first moment the window has a size.  A few
   * frames are allowed for the compositor to answer; after that the test says
   * what is wrong instead of hanging. */
  if ((width <= 0 || height <= 0) && preview->attempts++ < 50)
    return G_SOURCE_CONTINUE;

  if (width <= 0 || height <= 0) {
    g_printerr("golden: the bar never got an allocation\n");
    g_application_quit(G_APPLICATION(preview->app));
    return G_SOURCE_REMOVE;
  }

  GdkPaintable *paintable = gtk_widget_paintable_new(widget);
  GtkSnapshot *snapshot = gtk_snapshot_new();

  gdk_paintable_snapshot(paintable, snapshot, width, height);
  g_object_unref(paintable);

  GskRenderNode *node = gtk_snapshot_free_to_node(snapshot);
  GskRenderer *renderer = gsk_cairo_renderer_new();
  g_autoptr(GError) error = NULL;
  gboolean ok = FALSE;

  if (gsk_renderer_realize(renderer, NULL, &error)) {
    const graphene_rect_t viewport =
        GRAPHENE_RECT_INIT(0, 0, (float)width, (float)height);
    GdkTexture *texture =
        gsk_renderer_render_texture(renderer, node, &viewport);

    ok = texture && gdk_texture_save_to_png(texture, preview->output);
    g_clear_object(&texture);
    gsk_renderer_unrealize(renderer);
  } else {
    g_printerr("golden: the cairo renderer did not start: %s\n",
               error ? error->message : "unknown error");
  }

  g_object_unref(renderer);
  gsk_render_node_unref(node);

  if (ok && preview->reference)
    ok = compare_png(preview->output, preview->reference);
  else if (ok)
    g_print("golden: wrote %s (no reference to compare)\n", preview->output);

  g_application_quit(G_APPLICATION(preview->app));

  return G_SOURCE_REMOVE;
}

static void activate(GtkApplication *app, gpointer data) {
  Preview *preview = data;

  GtkCssProvider *css = gtk_css_provider_new();
  const char *css_path = g_getenv("WHELK_CSS_PATH");

  if (css_path && *css_path)
    gtk_css_provider_load_from_path(css, css_path);

  GdkDisplay *display = gdk_display_get_default();

  if (display)
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(css),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);

  g_object_unref(css);

  preview->bar = make_bar(app);
  g_timeout_add(50, save_preview, preview);
}

int main(int argc, char **argv) {
  (void)argc;

  const char *output = g_getenv("WHELK_GOLDEN_OUTPUT");
  const char *reference = g_getenv("WHELK_GOLDEN_REFERENCE");

  if (!output || !*output) {
    g_printerr("golden: WHELK_GOLDEN_OUTPUT is not set\n");
    return 2;
  }

  GtkApplication *app = gtk_application_new(
      "org.jointhefreeworld.whelk.golden", G_APPLICATION_NON_UNIQUE);
  Preview preview = {
      .app = app,
      .output = output,
      .reference = reference && *reference ? reference : NULL,
  };

  g_signal_connect(app, "activate", G_CALLBACK(activate), &preview);

  const int status = g_application_run(G_APPLICATION(app), 1, argv);

  g_object_unref(app);

  return status;
}

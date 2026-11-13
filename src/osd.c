// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "osd.h"

#include "logger.h"
#include "style.h"
#include "surface.h"

/* Long enough to read, short enough to be out of the way; macOS sits around
 * this. */
#define WHELK_OSD_TIMEOUT_MS 1400

struct _WhelkOsd {
  GObject parent_instance;

  GtkApplication *app;
  WhelkBarConfig *style;

  WhelkSurface *surface;
  GtkWidget *card;
  GtkWidget *icon;
  GtkWidget *progress;
  GtkWidget *value;

  GtkCssProvider *skin;
  guint hide_id;
};

G_DEFINE_TYPE(WhelkOsd, whelk_osd, G_TYPE_OBJECT)

static gboolean on_hide_timeout(gpointer user_data) {
  WhelkOsd *osd = user_data;

  osd->hide_id = 0;
  whelk_osd_hide(osd);

  return G_SOURCE_REMOVE;
}

static void build(WhelkOsd *osd) {
  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 14);
  GtkWidget *column = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
  GtkWidget *icon = gtk_label_new("");
  GtkWidget *progress = gtk_progress_bar_new();
  GtkWidget *value = gtk_label_new("");

  gtk_widget_add_css_class(card, "whelk-osd");
  gtk_widget_add_css_class(icon, "whelk-icon");
  gtk_widget_add_css_class(icon, "whelk-osd-icon");
  gtk_widget_add_css_class(value, "whelk-osd-value");
  gtk_widget_set_valign(icon, GTK_ALIGN_CENTER);
  gtk_label_set_xalign(GTK_LABEL(value), 0.0);
  gtk_widget_set_size_request(progress, 190, -1);
  gtk_progress_bar_set_show_text(GTK_PROGRESS_BAR(progress), FALSE);

  gtk_box_append(GTK_BOX(column), progress);
  gtk_box_append(GTK_BOX(column), value);
  gtk_widget_set_valign(column, GTK_ALIGN_CENTER);
  gtk_box_append(GTK_BOX(card), icon);
  gtk_box_append(GTK_BOX(card), column);

  /* A HUD is centred on the output, which is what anchoring both sides and
   * centring the child buys on a layer surface.  It is a strip, not a
   * full-screen layer: it must not block the window under the middle of the
   * screen while it is up. */
  GtkWidget *holder = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);

  gtk_widget_set_halign(card, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(card, GTK_ALIGN_CENTER);
  gtk_widget_set_hexpand(card, FALSE);
  gtk_box_append(GTK_BOX(holder), card);

  osd->surface = whelk_surface_new(osd->app, "osd");

  whelk_surface_set_layer(osd->surface, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_exclusive(osd->surface, FALSE);
  whelk_surface_set_anchors(osd->surface, TRUE, TRUE, FALSE, TRUE);
  whelk_surface_set_margins(
      osd->surface,
      (osd->style && osd->style->edge == GTK_LAYER_SHELL_EDGE_TOP)
          ? osd->style->height + 22
          : 22,
      0, 0, 0);
  whelk_surface_set_namespace(osd->surface, "whelk-osd");
  whelk_surface_set_keyboard_mode(osd->surface,
                                  GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);

  g_autofree char *widget_id = whelk_style_widget_id("osd");

  whelk_surface_set_widget_id(osd->surface, widget_id);
  osd->skin = whelk_style_apply(widget_id, osd->style, "whelk-osd", NULL);
  whelk_surface_set_child(osd->surface, holder);
  whelk_surface_present(osd->surface);

  osd->card = card;
  osd->icon = icon;
  osd->progress = progress;
  osd->value = value;

  GtkWindow *window = whelk_surface_get_window(osd->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

void whelk_osd_show(WhelkOsd *osd, const char *icon, int percent) {
  if (!osd || !osd->surface)
    return;

  gtk_label_set_text(GTK_LABEL(osd->icon), icon ? icon : "");
  gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(osd->progress),
                                CLAMP(percent, 0, 100) / 100.0);

  g_autofree char *text = g_strdup_printf("%d%%", percent);

  gtk_label_set_text(GTK_LABEL(osd->value), text);

  GtkWindow *window = whelk_surface_get_window(osd->surface);

  if (!window)
    return;

  gtk_widget_set_visible(GTK_WIDGET(window), TRUE);

  if (osd->hide_id != 0)
    g_source_remove(osd->hide_id);

  osd->hide_id = g_timeout_add(WHELK_OSD_TIMEOUT_MS, on_hide_timeout, osd);

  LOG_C_DEBUG("Osd", "showing %s at %d%%", icon ? icon : "(no icon)", percent);
}

void whelk_osd_hide(WhelkOsd *osd) {
  if (!osd || !osd->surface)
    return;

  GtkWindow *window = whelk_surface_get_window(osd->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

static void whelk_osd_dispose(GObject *object) {
  WhelkOsd *osd = WHELK_OSD(object);

  if (osd->hide_id != 0) {
    g_source_remove(osd->hide_id);
    osd->hide_id = 0;
  }

  if (osd->skin)
    whelk_style_detach(osd->skin, gdk_display_get_default());

  g_clear_object(&osd->surface);
  g_clear_pointer(&osd->style, whelk_bar_config_free);

  G_OBJECT_CLASS(whelk_osd_parent_class)->dispose(object);
}

static void whelk_osd_class_init(WhelkOsdClass *klass) {
  G_OBJECT_CLASS(klass)->dispose = whelk_osd_dispose;
}

static void whelk_osd_init(WhelkOsd *self) { (void)self; }

WhelkOsd *whelk_osd_new(GtkApplication *app, const WhelkBarConfig *style) {
  WhelkOsd *osd = g_object_new(WHELK_TYPE_OSD, NULL);

  osd->app = app;
  osd->style = style ? whelk_bar_config_copy(style) : whelk_bar_config_new("osd");

  build(osd);

  return osd;
}

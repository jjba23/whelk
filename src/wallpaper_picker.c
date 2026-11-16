// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "wallpaper_picker.h"

#include <gdk-pixbuf/gdk-pixbuf.h>
#include <string.h>

#include "logger.h"
#include "shell.h"
#include "style.h"
#include "surface.h"
#include "util/texture.h"

/* One thumbnail to decode, queued so opening the picker never waits on a
 * directory of 4K pictures. */
typedef struct {
  GtkWidget *picture; /* owned by the picker's tab; held while loading */
  char *path;
  int width;
  int height;
  guint generation;
} ThumbRequest;

struct _WhelkWallpaperPicker {
  GObject parent_instance;

  GtkApplication *app;
  WhelkWallpaperEngine *engine; /* borrowed */

  WhelkSurface *surface;
  GtkWidget *card;
  GtkWidget *flow;
  GtkWidget *scroller;
  GtkWidget *count;
  GtkWidget *warning;
  GtkWidget *empty;
  GtkWidget *expand_button;

  char *config_path;
  WhelkBarConfig *style;
  GtkCssProvider *skin;

  gboolean expanded;
  guint generation;
  guint thumb_idle;
  GQueue *thumb_queue; /* ThumbRequest* */
};

G_DEFINE_TYPE(WhelkWallpaperPicker, whelk_wallpaper_picker, G_TYPE_OBJECT)

static void rebuild(WhelkWallpaperPicker *picker);
static void update_card_size(WhelkWallpaperPicker *picker);
static void schedule_thumb(WhelkWallpaperPicker *picker, GtkWidget *picture,
                           const char *path, int width, int height);

/* =========================================================================
 * THUMBNAILS
 * ========================================================================= */

static void thumb_request_free(gpointer data) {
  ThumbRequest *request = data;

  if (!request)
    return;

  g_clear_object(&request->picture);
  g_free(request->path);
  g_free(request);
}

static void clear_thumb_queue(WhelkWallpaperPicker *picker) {
  if (picker->thumb_idle != 0) {
    g_source_remove(picker->thumb_idle);
    picker->thumb_idle = 0;
  }

  g_queue_clear_full(picker->thumb_queue, thumb_request_free);
}

static gboolean load_next_thumbnail(gpointer data) {
  WhelkWallpaperPicker *picker = data;

  picker->thumb_idle = 0;

  ThumbRequest *request = g_queue_pop_head(picker->thumb_queue);

  if (!request)
    return G_SOURCE_REMOVE;

  if (request->generation == picker->generation) {
    GError *error = NULL;
    GdkPixbuf *pixbuf = gdk_pixbuf_new_from_file_at_scale(
        request->path, request->width, request->height, TRUE, &error);

    if (pixbuf) {
      GdkTexture *texture = whelk_texture_from_pixbuf(pixbuf);

      if (texture) {
        gtk_picture_set_paintable(GTK_PICTURE(request->picture),
                                  GDK_PAINTABLE(texture));
        g_object_unref(texture);
      }

      g_object_unref(pixbuf);
    } else {
      /* gdk-pixbuf is the loader with the format plugins; GDK's own loader is
       * the second opinion for a format this build of pixbuf does not know. */
      g_clear_error(&error);

      g_autoptr(GFile) file = g_file_new_for_path(request->path);
      g_autoptr(GError) fallback_error = NULL;
      GdkTexture *texture = gdk_texture_new_from_file(file, &fallback_error);

      if (texture) {
        gtk_picture_set_paintable(GTK_PICTURE(request->picture),
                                  GDK_PAINTABLE(texture));
        g_object_unref(texture);
      } else {
        gtk_widget_add_css_class(request->picture,
                                 "whelk-wallpaper-thumb-missing");
      }
    }
  }

  thumb_request_free(request);

  if (!g_queue_is_empty(picker->thumb_queue))
    picker->thumb_idle = g_idle_add(load_next_thumbnail, picker);

  return G_SOURCE_REMOVE;
}

static void schedule_thumb(WhelkWallpaperPicker *picker, GtkWidget *picture,
                           const char *path, int width, int height) {
  ThumbRequest *request = g_new0(ThumbRequest, 1);

  request->picture = g_object_ref(picture);
  request->path = g_strdup(path);
  request->width = width;
  request->height = height;
  request->generation = picker->generation;

  g_queue_push_tail(picker->thumb_queue, request);

  if (picker->thumb_idle == 0)
    picker->thumb_idle = g_idle_add(load_next_thumbnail, picker);
}

/* =========================================================================
 * THE GRID
 * ========================================================================= */

static char *wallpaper_name(const char *path) {
  const char *base = strrchr(path, '/');

  base = base ? base + 1 : path;

  const char *dot = strrchr(base, '.');

  return dot && dot != base ? g_strndup(base, (gsize)(dot - base))
                            : g_strdup(base);
}

static void apply_tile(WhelkWallpaperPicker *picker, GtkWidget *button) {
  const char *path = g_object_get_data(G_OBJECT(button), "whelk-wallpaper");

  if (!path)
    return;

  WhelkShell *shell = whelk_shell_get_default();

  if (!shell || !whelk_shell_set_wallpaper(shell, path)) {
    LOG_C_WARN("Wallpaper", "'%s' could not be set", path);
    return;
  }

  whelk_wallpaper_picker_hide(picker);
}

static void on_tile_clicked(GtkButton *button, gpointer data) {
  apply_tile(data, GTK_WIDGET(button));
}

/* The flow box owns arrow-key navigation; Enter on the focused child is the
 * keyboard's answer to a click on the tile inside it. */
static void on_child_activated(GtkFlowBox *flow, GtkFlowBoxChild *child,
                               gpointer data) {
  (void)flow;

  GtkWidget *tile = gtk_widget_get_first_child(GTK_WIDGET(child));

  if (tile)
    apply_tile(data, tile);
}

static void clear_grid(WhelkWallpaperPicker *picker) {
  GtkWidget *child = NULL;

  while ((child = gtk_widget_get_first_child(picker->flow)) != NULL)
    gtk_flow_box_remove(GTK_FLOW_BOX(picker->flow), child);
}

static GtkWidget *make_tile(WhelkWallpaperPicker *picker,
                            const WhelkWallpaperItem *item, int thumb_width,
                            int thumb_height) {
  GtkWidget *button = gtk_button_new();
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
  GtkWidget *thumb = NULL;

  gtk_widget_add_css_class(button, "whelk-wallpaper-tile");
  gtk_widget_add_css_class(box, "whelk-wallpaper-tile-box");
  gtk_widget_set_valign(button, GTK_ALIGN_START);

  if (item->kind == WHELK_WALLPAPER_VIDEO) {
    GtkWidget *mark = gtk_label_new("▶");
    GtkWidget *kind = gtk_label_new("Video");

    gtk_widget_add_css_class(mark, "whelk-wallpaper-video-mark");
    gtk_widget_add_css_class(kind, "whelk-wallpaper-video-kind");
    gtk_widget_set_size_request(mark, thumb_width, thumb_height - 20);
    gtk_widget_set_halign(mark, GTK_ALIGN_CENTER);
    gtk_widget_set_valign(mark, GTK_ALIGN_END);

    thumb = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_add_css_class(thumb, "whelk-wallpaper-thumb");
    gtk_widget_add_css_class(thumb, "whelk-wallpaper-video-thumb");
    gtk_widget_set_size_request(thumb, thumb_width, thumb_height);
    gtk_box_append(GTK_BOX(thumb), mark);
    gtk_box_append(GTK_BOX(thumb), kind);
  } else {
    thumb = gtk_picture_new();
    gtk_widget_add_css_class(thumb, "whelk-wallpaper-thumb");
    gtk_picture_set_content_fit(GTK_PICTURE(thumb),
                                GTK_CONTENT_FIT_COVER);
    gtk_widget_set_size_request(thumb, thumb_width, thumb_height);
    schedule_thumb(picker, thumb, item->path, thumb_width * 2,
                   thumb_height * 2);
  }

  g_autofree char *name = wallpaper_name(item->path);
  GtkWidget *label = gtk_label_new(name);

  gtk_widget_add_css_class(label, "whelk-wallpaper-tile-name");
  gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_MIDDLE);
  gtk_label_set_max_width_chars(GTK_LABEL(label), 22);
  gtk_widget_set_halign(label, GTK_ALIGN_CENTER);

  gtk_box_append(GTK_BOX(box), thumb);
  gtk_box_append(GTK_BOX(box), label);
  gtk_button_set_child(GTK_BUTTON(button), box);

  g_object_set_data_full(G_OBJECT(button), "whelk-wallpaper",
                         g_strdup(item->path), g_free);
  gtk_widget_set_tooltip_text(button, item->path);

  if (g_strcmp0(whelk_wallpaper_engine_current(picker->engine), item->path) ==
      0)
    gtk_widget_add_css_class(button, "selected");

  g_signal_connect(button, "clicked", G_CALLBACK(on_tile_clicked), picker);

  return button;
}

static void rebuild(WhelkWallpaperPicker *picker) {
  picker->generation++;
  clear_thumb_queue(picker);
  clear_grid(picker);

  if (picker->engine)
    whelk_wallpaper_engine_refresh(picker->engine);

  GPtrArray *items =
      picker->engine ? whelk_wallpaper_engine_items(picker->engine) : NULL;
  const guint count = items ? items->len : 0;

  g_autofree char *summary =
      g_strdup_printf("%u wallpaper%s", count, count == 1 ? "" : "s");

  gtk_label_set_text(GTK_LABEL(picker->count), summary);
  gtk_widget_set_visible(picker->empty, count == 0);
  gtk_widget_set_visible(picker->scroller, count > 0);

  const gboolean inactive = count > 0 && picker->engine &&
                            !whelk_wallpaper_engine_is_active(picker->engine);

  gtk_widget_set_visible(picker->warning, inactive);

  if (count == 0)
    return;

  const WhelkWallpaperConfig *config =
      picker->engine ? whelk_wallpaper_engine_config(picker->engine) : NULL;
  const int card_width = config ? config->picker_width : 960;
  const int columns = config ? config->picker_columns : 4;
  const int thumb_width =
      MAX(96, (card_width - 64 - (columns - 1) * 12) / MAX(1, columns));
  const int thumb_height = MAX(72, thumb_width * 9 / 16);

  for (guint i = 0; i < count; i++)
    gtk_flow_box_append(
        GTK_FLOW_BOX(picker->flow),
        make_tile(picker, g_ptr_array_index(items, i), thumb_width,
                  thumb_height));
}

/* =========================================================================
 * GEOMETRY AND KEYS
 * ========================================================================= */

static void update_card_size(WhelkWallpaperPicker *picker) {
  const WhelkWallpaperConfig *config =
      picker->engine ? whelk_wallpaper_engine_config(picker->engine) : NULL;
  const int configured_width = config ? config->picker_width : 960;
  const int configured_height = config ? config->picker_height : 640;
  const int columns = config ? config->picker_columns : 4;

  int width = configured_width;
  int height = configured_height;
  GdkDisplay *display = gdk_display_get_default();
  GdkMonitor *monitor = NULL;

  if (display) {
    GtkWindow *window = whelk_surface_get_window(picker->surface);
    GdkSurface *gdk_surface =
        window ? gtk_native_get_surface(GTK_NATIVE(window)) : NULL;

    if (gdk_surface)
      monitor = gdk_display_get_monitor_at_surface(display, gdk_surface);

    if (!monitor) {
      GListModel *monitors = gdk_display_get_monitors(display);

      if (monitors)
        monitor = GDK_MONITOR(g_list_model_get_item(monitors, 0));
    } else {
      /* monitor_at_surface() borrows; the branch below owns a reference. */
      g_object_ref(monitor);
    }
  }

  if (monitor) {
    GdkRectangle geometry = {0};

    gdk_monitor_get_geometry(monitor, &geometry);

    const int limit_width = MAX(320, geometry.width - 96);
    const int limit_height = MAX(240, geometry.height - 96);

    /* The compact card is the configured size, unless the monitor is
     * smaller than that; expanding always asks for the whole limit. */
    width = picker->expanded ? limit_width : MIN(configured_width, limit_width);
    height = picker->expanded ? limit_height
                              : MIN(configured_height, limit_height);

    g_object_unref(monitor);
  }

  gtk_widget_set_size_request(picker->card, width, height);
  gtk_flow_box_set_min_children_per_line(GTK_FLOW_BOX(picker->flow), columns);
  gtk_flow_box_set_max_children_per_line(GTK_FLOW_BOX(picker->flow),
                                         picker->expanded ? 12 : columns);
  gtk_widget_set_visible(picker->expand_button, TRUE);
  gtk_button_set_icon_name(
      GTK_BUTTON(picker->expand_button),
      picker->expanded ? "view-restore-symbolic" : "view-fullscreen-symbolic");
}

static void toggle_expanded(WhelkWallpaperPicker *picker) {
  picker->expanded = !picker->expanded;
  gtk_widget_set_size_request(picker->card, -1, -1);
  update_card_size(picker);
}

static gboolean on_key_pressed(GtkEventControllerKey *controller,
                               guint keyval, guint keycode,
                               GdkModifierType state, gpointer data) {
  (void)controller;
  (void)keycode;

  WhelkWallpaperPicker *picker = data;

  if (keyval == GDK_KEY_Escape) {
    whelk_wallpaper_picker_hide(picker);
    return TRUE;
  }

  if (keyval == GDK_KEY_F11 ||
      ((state & GDK_CONTROL_MASK) &&
       (keyval == GDK_KEY_f || keyval == GDK_KEY_F))) {
    toggle_expanded(picker);
    return TRUE;
  }

  return FALSE;
}

static gboolean on_outside_pressed(GtkGestureClick *gesture, int n_press,
                                   double x, double y, gpointer data) {
  (void)gesture;
  (void)n_press;

  WhelkWallpaperPicker *picker = data;
  GtkWidget *window = GTK_WIDGET(whelk_surface_get_window(picker->surface));
  graphene_rect_t bounds;

  if (!window || !picker->card ||
      !gtk_widget_compute_bounds(picker->card, window, &bounds))
    return FALSE;

  if (x < bounds.origin.x || y < bounds.origin.y ||
      x > bounds.origin.x + bounds.size.width ||
      y > bounds.origin.y + bounds.size.height) {
    whelk_wallpaper_picker_hide(picker);
    return TRUE;
  }

  return FALSE;
}

static void on_close_clicked(GtkButton *button, gpointer data) {
  (void)button;

  whelk_wallpaper_picker_hide(data);
}

static void on_expand_clicked(GtkButton *button, gpointer data) {
  (void)button;

  toggle_expanded(data);
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

static void whelk_wallpaper_picker_dispose(GObject *object) {
  WhelkWallpaperPicker *picker = WHELK_WALLPAPER_PICKER(object);

  clear_thumb_queue(picker);
  g_clear_pointer(&picker->thumb_queue, g_queue_free);

  if (picker->skin) {
    whelk_style_detach(picker->skin, gdk_display_get_default());
    picker->skin = NULL;
  }

  g_clear_object(&picker->surface);
  g_clear_pointer(&picker->style, whelk_bar_config_free);
  g_free(picker->config_path);

  G_OBJECT_CLASS(whelk_wallpaper_picker_parent_class)->dispose(object);
}

static void whelk_wallpaper_picker_class_init(
    WhelkWallpaperPickerClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_wallpaper_picker_dispose;
}

static void whelk_wallpaper_picker_init(WhelkWallpaperPicker *picker) {
  picker->thumb_queue = g_queue_new();
}

WhelkWallpaperPicker *whelk_wallpaper_picker_new(
    GtkApplication *app, WhelkWallpaperEngine *engine, const char *config_path,
    const WhelkBarConfig *style) {
  WhelkWallpaperPicker *picker =
      g_object_new(WHELK_TYPE_WALLPAPER_PICKER, NULL);

  picker->app = app;
  picker->engine = engine;
  picker->config_path = g_strdup(config_path);

  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
  GtkWidget *title = gtk_label_new("Wallpaper");
  GtkWidget *spacer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
  GtkWidget *close =
      gtk_button_new_from_icon_name("window-close-symbolic");
  GtkWidget *expand =
      gtk_button_new_from_icon_name("view-fullscreen-symbolic");

  gtk_widget_add_css_class(card, "whelk-wallpaper-picker");
  gtk_widget_add_css_class(header, "whelk-wallpaper-picker-header");
  gtk_widget_add_css_class(title, "whelk-wallpaper-picker-title");

  picker->card = card;
  picker->count = gtk_label_new(NULL);
  gtk_widget_add_css_class(picker->count, "whelk-wallpaper-picker-count");

  gtk_widget_add_css_class(close, "flat");
  gtk_widget_add_css_class(expand, "flat");
  gtk_widget_set_tooltip_text(close, "Close (Esc)");
  gtk_widget_set_tooltip_text(expand, "Expand or restore (F11)");

  gtk_widget_set_hexpand(spacer, TRUE);
  gtk_box_append(GTK_BOX(header), title);
  gtk_box_append(GTK_BOX(header), picker->count);
  gtk_box_append(GTK_BOX(header), spacer);
  gtk_box_append(GTK_BOX(header), expand);
  gtk_box_append(GTK_BOX(header), close);

  picker->warning = gtk_label_new(
      "whelk is not drawing a background.  Turn on #:enabled? in "
      "(wallpaper-settings …); drawing also needs a wlr-layer-shell "
      "compositor.");
  gtk_widget_add_css_class(picker->warning, "whelk-wallpaper-picker-warning");
  gtk_label_set_wrap(GTK_LABEL(picker->warning), TRUE);
  gtk_widget_set_halign(picker->warning, GTK_ALIGN_START);
  gtk_widget_set_visible(picker->warning, FALSE);

  picker->flow = gtk_flow_box_new();
  gtk_widget_add_css_class(picker->flow, "whelk-wallpaper-grid");
  gtk_flow_box_set_homogeneous(GTK_FLOW_BOX(picker->flow), TRUE);
  gtk_flow_box_set_column_spacing(GTK_FLOW_BOX(picker->flow), 12);
  gtk_flow_box_set_row_spacing(GTK_FLOW_BOX(picker->flow), 12);
  gtk_flow_box_set_selection_mode(GTK_FLOW_BOX(picker->flow),
                                  GTK_SELECTION_NONE);
  gtk_widget_set_halign(picker->flow, GTK_ALIGN_CENTER);
  g_signal_connect(picker->flow, "child-activated",
                   G_CALLBACK(on_child_activated), picker);

  picker->scroller = gtk_scrolled_window_new();
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(picker->scroller),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(picker->scroller),
                                picker->flow);
  gtk_widget_set_vexpand(picker->scroller, TRUE);
  gtk_widget_add_css_class(picker->scroller, "whelk-wallpaper-scroller");

  picker->empty = gtk_label_new(
      "No wallpapers found.\n"
      "Add PNG, JPEG, WebP, AVIF or GIF images, or a video, to the "
      "directories in #:paths.");
  gtk_widget_add_css_class(picker->empty, "whelk-wallpaper-picker-empty");
  gtk_label_set_wrap(GTK_LABEL(picker->empty), TRUE);
  gtk_widget_set_halign(picker->empty, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(picker->empty, GTK_ALIGN_CENTER);
  gtk_widget_set_vexpand(picker->empty, TRUE);
  gtk_widget_set_visible(picker->empty, FALSE);

  GtkWidget *hint =
      gtk_label_new("Click a wallpaper to use it · F11 expands · Esc closes");
  gtk_widget_add_css_class(hint, "whelk-wallpaper-picker-hint");
  gtk_widget_set_halign(hint, GTK_ALIGN_START);

  gtk_box_append(GTK_BOX(card), header);
  gtk_box_append(GTK_BOX(card), picker->warning);
  gtk_box_append(GTK_BOX(card), picker->scroller);
  gtk_box_append(GTK_BOX(card), picker->empty);
  gtk_box_append(GTK_BOX(card), hint);

  picker->expand_button = expand;
  g_signal_connect(close, "clicked", G_CALLBACK(on_close_clicked), picker);
  g_signal_connect(expand, "clicked", G_CALLBACK(on_expand_clicked), picker);

  GtkWidget *holder = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

  gtk_widget_set_halign(card, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(card, GTK_ALIGN_CENTER);
  gtk_widget_set_hexpand(holder, TRUE);
  gtk_widget_set_vexpand(holder, TRUE);
  gtk_box_append(GTK_BOX(holder), card);

  picker->surface = whelk_surface_new(app, "wallpaper-picker");
  whelk_surface_set_edge(picker->surface, GTK_LAYER_SHELL_EDGE_TOP);
  whelk_surface_set_layer(picker->surface, GTK_LAYER_SHELL_LAYER_OVERLAY);
  whelk_surface_set_keyboard_mode(picker->surface,
                                  GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE);
  whelk_surface_set_exclusive(picker->surface, FALSE);
  whelk_surface_set_anchors(picker->surface, TRUE, TRUE, TRUE, TRUE);
  whelk_surface_set_namespace(picker->surface, "whelk-wallpaper-picker");
  whelk_surface_set_widget_id(picker->surface, "whelk-wallpaper-picker");
  whelk_surface_set_child(picker->surface, holder);

  whelk_wallpaper_picker_set_style(picker, style);

  GtkEventController *keys = gtk_event_controller_key_new();

  g_signal_connect(keys, "key-pressed", G_CALLBACK(on_key_pressed), picker);
  gtk_widget_add_controller(
      GTK_WIDGET(whelk_surface_get_window(picker->surface)), keys);

  GtkGesture *outside = gtk_gesture_click_new();

  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(outside),
                                GDK_BUTTON_PRIMARY);
  g_signal_connect(outside, "pressed", G_CALLBACK(on_outside_pressed), picker);
  gtk_widget_add_controller(
      GTK_WIDGET(whelk_surface_get_window(picker->surface)),
      GTK_EVENT_CONTROLLER(outside));

  update_card_size(picker);

  whelk_surface_present(picker->surface);
  gtk_widget_set_visible(
      GTK_WIDGET(whelk_surface_get_window(picker->surface)), FALSE);

  return picker;
}

void whelk_wallpaper_picker_set_style(WhelkWallpaperPicker *picker,
                                      const WhelkBarConfig *style) {
  if (!picker)
    return;

  if (picker->skin) {
    whelk_style_detach(picker->skin, gdk_display_get_default());
    picker->skin = NULL;
  }

  g_clear_pointer(&picker->style, whelk_bar_config_free);

  if (!style)
    return;

  picker->style = whelk_bar_config_copy(style);

  if (picker->style->radius < 14)
    picker->style->radius = 14;

  picker->skin = whelk_style_apply(
      "whelk-wallpaper-picker", picker->style, "whelk-wallpaper-picker",
      "window#whelk-wallpaper-picker { "
      "background-color: rgba(0, 0, 0, 0.35); }");
}

void whelk_wallpaper_picker_show(WhelkWallpaperPicker *picker) {
  if (!picker)
    return;

  /* A reload can change the configured card size while the picker is hidden. */
  update_card_size(picker);
  rebuild(picker);

  GtkWindow *window = whelk_surface_get_window(picker->surface);

  if (!window)
    return;

  gtk_widget_set_visible(GTK_WIDGET(window), TRUE);
  gtk_window_present(window);

  /* An overlay with exclusive keyboard must land on something a key can
   * reach; the first flow-box child is what arrow keys navigate. */
  GtkWidget *first = gtk_widget_get_first_child(picker->flow);

  if (first)
    gtk_widget_grab_focus(first);
  else
    gtk_widget_grab_focus(picker->expand_button);
}

void whelk_wallpaper_picker_hide(WhelkWallpaperPicker *picker) {
  if (!picker)
    return;

  GtkWindow *window = whelk_surface_get_window(picker->surface);

  if (window)
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
}

gboolean whelk_wallpaper_picker_is_visible(WhelkWallpaperPicker *picker) {
  if (!picker)
    return FALSE;

  GtkWindow *window = whelk_surface_get_window(picker->surface);

  return window && gtk_widget_get_visible(GTK_WIDGET(window));
}

void whelk_wallpaper_picker_toggle(WhelkWallpaperPicker *picker) {
  if (!picker)
    return;

  if (whelk_wallpaper_picker_is_visible(picker))
    whelk_wallpaper_picker_hide(picker);
  else
    whelk_wallpaper_picker_show(picker);
}

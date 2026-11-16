// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "wallpaper.h"

#include <gdk-pixbuf/gdk-pixbuf.h>
#include <gdk/wayland/gdkwayland.h>
#include <math.h>
#include <string.h>

#include "config.h"
#include "guile_bridge.h"
#include "logger.h"
#include "surface.h"
#include "util/texture.h"
#include "wallpaper_view.h"

/* A movie is not an image: it has to keep playing, so its frames arrive from
 * a GtkMediaStream.  A still picture gets one texture; an animated image gets
 * a timer that hands the view one frame at a time. */
typedef struct {
  WhelkWallpaperEngine *engine;
  WhelkSurface *surface;
  WhelkWallpaperView *view;
  char *connector;
  char *path;

  GtkMediaStream *video;

  guint frame_timeout;
  GdkPixbufAnimation *animation;
  GdkPixbufAnimationIter *iter;
} WallpaperSurface;

struct _WhelkWallpaperEngine {
  GtkApplication *app;
  WhelkWallpaperConfig *config;
  char *config_path;
  char *selected;

  GPtrArray *items;    /* WhelkWallpaperItem* */
  GPtrArray *surfaces; /* WallpaperSurface* */

  guint rotate_timeout;
  guint monitor_idle;
  guint64 shuffle_state;

  char *monitor_signature;
  int max_width;
  int max_height;

  GtkCssProvider *skin;

  GListModel *monitors;
  gulong monitor_handler;

  gboolean explained_unsupported;
};

/* =========================================================================
 * THE COLLECTION
 * ========================================================================= */

static void wallpaper_item_free(gpointer data) {
  WhelkWallpaperItem *item = data;

  if (!item)
    return;

  g_free(item->path);
  g_free(item);
}

/* "~/Pictures" is how a person writes a home directory, and a settings file is
 * written by a person. */
static char *expand_path(const char *path) {
  if (!path || *path == '\0')
    return NULL;

  if (g_str_has_prefix(path, "~/"))
    return g_build_filename(g_get_home_dir(), path + 2, NULL);

  return g_strdup(path);
}

static void add_item(WhelkWallpaperEngine *engine, const char *path,
                     GHashTable *seen) {
  if (!path || *path == '\0' || g_hash_table_contains(seen, path))
    return;

  if (!whelk_wallpaper_is_supported(path))
    return;

  const WhelkWallpaperKind kind = whelk_wallpaper_kind_for_path(path);

  if (kind == WHELK_WALLPAPER_VIDEO && !engine->config->video)
    return;

  if (!g_file_test(path, G_FILE_TEST_IS_REGULAR))
    return;

  WhelkWallpaperItem *item = g_new0(WhelkWallpaperItem, 1);

  item->path = g_strdup(path);
  item->kind = kind;

  g_ptr_array_add(engine->items, item);
  g_hash_table_add(seen, g_strdup(path));
}

/* A directory is the normal case; a single file is allowed too, so a settings
 * file can name one wallpaper without making it a directory of one.  A
 * directory symlink is not followed: a link that points at its own parent
 * would make the walk run forever. */
static void scan_path(WhelkWallpaperEngine *engine, const char *path,
                      GHashTable *seen, guint depth) {
  if (!path || depth > 12)
    return;

  if (g_file_test(path, G_FILE_TEST_IS_DIR) &&
      !g_file_test(path, G_FILE_TEST_IS_SYMLINK)) {
    GError *error = NULL;
    GDir *directory = g_dir_open(path, 0, &error);

    if (!directory) {
      LOG_C_WARN("Wallpaper", "cannot read '%s': %s", path, error->message);
      g_clear_error(&error);
      return;
    }

    const char *name = NULL;

    while ((name = g_dir_read_name(directory)) != NULL) {
      if (name[0] == '.')
        continue;

      g_autofree char *child = g_build_filename(path, name, NULL);

      scan_path(engine, child, seen, depth + 1);
    }

    g_dir_close(directory);
    return;
  }

  add_item(engine, path, seen);
}

static gint compare_items(gconstpointer a, gconstpointer b) {
  const WhelkWallpaperItem *left = *(WhelkWallpaperItem *const *)a;
  const WhelkWallpaperItem *right = *(WhelkWallpaperItem *const *)b;

  return g_strcmp0(left->path, right->path);
}

static void rescan(WhelkWallpaperEngine *engine) {
  g_ptr_array_set_size(engine->items, 0);

  if (!engine->config)
    return;

  GHashTable *seen = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                           NULL);

  for (guint i = 0; i < engine->config->paths->len; i++) {
    g_autofree char *path =
        expand_path(g_ptr_array_index(engine->config->paths, i));

    scan_path(engine, path, seen, 0);
  }

  /* An explicit starting file outside the configured directories is still a
   * wallpaper, and the rotation should be able to come back to it. */
  if (engine->config->current) {
    g_autofree char *current = expand_path(engine->config->current);

    add_item(engine, current, seen);
  }

  g_hash_table_destroy(seen);
  g_ptr_array_sort(engine->items, compare_items);

  LOG_C_DEBUG("Wallpaper", "%u wallpaper(s) in the collection",
              engine->items->len);
}

static gboolean path_is_usable(WhelkWallpaperEngine *engine, const char *path) {
  if (!path || !whelk_wallpaper_is_supported(path) ||
      !g_file_test(path, G_FILE_TEST_IS_REGULAR))
    return FALSE;

  if (whelk_wallpaper_kind_for_path(path) == WHELK_WALLPAPER_VIDEO &&
      engine->config && !engine->config->video)
    return FALSE;

  return TRUE;
}

static void resolve_selected(WhelkWallpaperEngine *engine) {
  if (path_is_usable(engine, engine->selected))
    return;

  g_clear_pointer(&engine->selected, g_free);

  if (engine->config && path_is_usable(engine, engine->config->current)) {
    engine->selected = expand_path(engine->config->current);
    return;
  }

  if (engine->items->len > 0) {
    WhelkWallpaperItem *first = g_ptr_array_index(engine->items, 0);

    engine->selected = g_strdup(first->path);
  }
}

/* =========================================================================
 * THE SURFACES
 * ========================================================================= */

static gboolean layer_shell_supported(void) {
  GdkDisplay *display = gdk_display_get_default();

  if (!display)
    return FALSE;

  G_GNUC_BEGIN_IGNORE_DEPRECATIONS
  const gboolean wayland = GDK_IS_WAYLAND_DISPLAY(display);
  G_GNUC_END_IGNORE_DEPRECATIONS

  return wayland && gtk_layer_is_supported();
}

static void clear_media(WallpaperSurface *wallpaper) {
  if (wallpaper->frame_timeout != 0) {
    g_source_remove(wallpaper->frame_timeout);
    wallpaper->frame_timeout = 0;
  }

  g_clear_object(&wallpaper->iter);
  g_clear_object(&wallpaper->animation);

  if (wallpaper->video) {
    gtk_media_stream_pause(wallpaper->video);
    g_clear_object(&wallpaper->video);
  }
}

static void wallpaper_surface_free(gpointer data) {
  WallpaperSurface *wallpaper = data;

  if (!wallpaper)
    return;

  clear_media(wallpaper);
  g_clear_object(&wallpaper->surface);
  g_free(wallpaper->connector);
  g_free(wallpaper->path);
  g_free(wallpaper);
}

static void destroy_surfaces(WhelkWallpaperEngine *engine) {
  g_ptr_array_set_size(engine->surfaces, 0);
  g_clear_pointer(&engine->monitor_signature, g_free);
}

static GdkTexture *texture_for_pixbuf(WhelkWallpaperEngine *engine,
                                      GdkPixbuf *pixbuf) {
  if (!pixbuf)
    return NULL;

  const int width = gdk_pixbuf_get_width(pixbuf);
  const int height = gdk_pixbuf_get_height(pixbuf);
  const int max_width = engine->max_width > 0 ? engine->max_width : 3840;
  const int max_height = engine->max_height > 0 ? engine->max_height : 2160;

  if (width > max_width || height > max_height) {
    const double scale =
        MIN((double)max_width / (double)width,
            (double)max_height / (double)height);
    const int scaled_width = MAX(1, (int)(width * scale));
    const int scaled_height = MAX(1, (int)(height * scale));
    GdkPixbuf *scaled = gdk_pixbuf_scale_simple(
        pixbuf, scaled_width, scaled_height, GDK_INTERP_BILINEAR);

    if (scaled) {
      GdkTexture *texture = whelk_texture_from_pixbuf(scaled);

      g_object_unref(scaled);
      return texture;
    }
  }

  return whelk_texture_from_pixbuf(pixbuf);
}

/* gdk-pixbuf 2.44 deprecates the animation API in favour of the newer image
 * stack, but no replacement reads an animated GIF/WebP frame by frame and
 * hands back the delay; the ABI is unchanged.  The deprecated X11 calls in
 * surface.c are used the same way: a working, stable API behind one pair of
 * pragmas rather than a feature dropped for a rename. */
G_GNUC_BEGIN_IGNORE_DEPRECATIONS
static gboolean on_animation_frame(gpointer user_data) {
  WallpaperSurface *wallpaper = user_data;

  wallpaper->frame_timeout = 0;

  if (!wallpaper->animation || !wallpaper->iter)
    return G_SOURCE_REMOVE;

  gdk_pixbuf_animation_iter_advance(wallpaper->iter, NULL);

  GdkPixbuf *pixbuf = gdk_pixbuf_animation_iter_get_pixbuf(wallpaper->iter);

  if (pixbuf) {
    GdkTexture *texture = texture_for_pixbuf(wallpaper->engine, pixbuf);

    if (texture) {
      whelk_wallpaper_view_set_frame(wallpaper->view,
                                     GDK_PAINTABLE(texture));
      g_object_unref(texture);
    }
  }

  int delay = gdk_pixbuf_animation_iter_get_delay_time(wallpaper->iter);

  /* A GIF's zero-delay frame is not "as fast as the CPU can decode"; browsers
   * have read it as 100 ms for thirty years, and so does this. */
  if (delay < 20)
    delay = 100;

  wallpaper->frame_timeout =
      g_timeout_add((guint)delay, on_animation_frame, wallpaper);

  return G_SOURCE_REMOVE;
}
G_GNUC_END_IGNORE_DEPRECATIONS

static void on_video_error(GObject *object, GParamSpec *pspec, gpointer data) {
  (void)pspec;

  WallpaperSurface *wallpaper = data;
  const GError *error = gtk_media_stream_get_error(GTK_MEDIA_STREAM(object));

  if (error)
    LOG_C_WARN("Wallpaper", "video '%s' could not play: %s",
               wallpaper->path ? wallpaper->path : "(unknown)", error->message);
}

G_GNUC_BEGIN_IGNORE_DEPRECATIONS
static void load_surface(WhelkWallpaperEngine *engine,
                         WallpaperSurface *wallpaper, const char *path,
                         gboolean animate) {
  clear_media(wallpaper);

  g_free(wallpaper->path);
  wallpaper->path = g_strdup(path);

  if (!path) {
    whelk_wallpaper_view_clear(wallpaper->view);
    return;
  }

  const WhelkWallpaperAnimation animation =
      (animate && engine->config) ? engine->config->animation
                                  : WHELK_WALLPAPER_ANIMATION_NONE;
  const guint duration =
      (animate && engine->config) ? (guint)engine->config->animation_ms : 0;

  if (whelk_wallpaper_kind_for_path(path) == WHELK_WALLPAPER_VIDEO) {
    if (!engine->config || !engine->config->video)
      return;

    g_autoptr(GFile) file = g_file_new_for_path(path);
    GtkMediaStream *stream = gtk_media_file_new_for_file(file);

    gtk_media_stream_set_loop(stream, TRUE);
    gtk_media_stream_set_muted(stream, engine->config->mute_video);
    g_signal_connect(stream, "notify::error", G_CALLBACK(on_video_error),
                     wallpaper);
    gtk_media_stream_play(stream);

    wallpaper->video = stream;

    /* The stream is a GdkPaintable: while its first frame decodes, the old
     * picture is still there and the crossfade has already started, which is
     * what keeps a video wallpaper from flashing black. */
    whelk_wallpaper_view_set_paintable(wallpaper->view, GDK_PAINTABLE(stream),
                                       animation, duration);
    return;
  }

  GError *error = NULL;
  GdkPixbufAnimation *animation_object =
      gdk_pixbuf_animation_new_from_file(path, &error);

  if (!animation_object) {
    /* The loader with the format plugins has the final word, but GDK's own
     * loader may know a format it does not (AVIF and JPEG XL move faster
     * than gdk-pixbuf's plugin set on some distributions). */
    g_autoptr(GFile) file = g_file_new_for_path(path);
    g_autoptr(GError) fallback_error = NULL;
    GdkTexture *texture = gdk_texture_new_from_file(file, &fallback_error);

    if (texture) {
      whelk_wallpaper_view_set_paintable(wallpaper->view,
                                         GDK_PAINTABLE(texture), animation,
                                         duration);
      g_object_unref(texture);
    } else {
      LOG_C_WARN("Wallpaper", "cannot decode '%s': %s", path, error->message);
    }

    g_clear_error(&error);
    return;
  }

  if (gdk_pixbuf_animation_is_static_image(animation_object)) {
    GdkPixbuf *pixbuf =
        gdk_pixbuf_animation_get_static_image(animation_object);
    GdkTexture *texture = texture_for_pixbuf(engine, pixbuf);

    if (texture) {
      whelk_wallpaper_view_set_paintable(wallpaper->view,
                                         GDK_PAINTABLE(texture), animation,
                                         duration);
      g_object_unref(texture);
    } else {
      LOG_C_WARN("Wallpaper", "'%s' decoded but could not become a texture",
                 path);
    }

    g_object_unref(animation_object);
    return;
  }

  wallpaper->animation = animation_object;
  wallpaper->iter = gdk_pixbuf_animation_get_iter(animation_object, NULL);

  GdkPixbuf *pixbuf = gdk_pixbuf_animation_iter_get_pixbuf(wallpaper->iter);

  if (pixbuf) {
    GdkTexture *texture = texture_for_pixbuf(engine, pixbuf);

    if (texture) {
      whelk_wallpaper_view_set_paintable(wallpaper->view,
                                         GDK_PAINTABLE(texture), animation,
                                         duration);
      g_object_unref(texture);
    }
  }

  int delay = gdk_pixbuf_animation_iter_get_delay_time(wallpaper->iter);

  if (delay < 20)
    delay = 100;

  if (wallpaper->frame_timeout != 0)
    g_source_remove(wallpaper->frame_timeout);

  wallpaper->frame_timeout =
      g_timeout_add((guint)delay, on_animation_frame, wallpaper);
}
G_GNUC_END_IGNORE_DEPRECATIONS

static void apply_current(WhelkWallpaperEngine *engine, gboolean animate) {
  for (guint i = 0; i < engine->surfaces->len; i++) {
    WallpaperSurface *wallpaper = g_ptr_array_index(engine->surfaces, i);

    load_surface(engine, wallpaper, engine->selected, animate);
  }
}

static void create_surface(WhelkWallpaperEngine *engine, GdkMonitor *monitor) {
  WallpaperSurface *wallpaper = g_new0(WallpaperSurface, 1);

  wallpaper->engine = engine;
  wallpaper->connector =
      g_strdup(monitor ? gdk_monitor_get_connector(monitor) : NULL);

  wallpaper->surface = whelk_surface_new(engine->app, "wallpaper");
  whelk_surface_set_edge(wallpaper->surface, GTK_LAYER_SHELL_EDGE_TOP);
  whelk_surface_set_layer(wallpaper->surface,
                          GTK_LAYER_SHELL_LAYER_BACKGROUND);
  whelk_surface_set_keyboard_mode(wallpaper->surface,
                                  GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
  whelk_surface_set_exclusive(wallpaper->surface, FALSE);
  whelk_surface_set_exclusive_zone(wallpaper->surface, -1);
  whelk_surface_set_anchors(wallpaper->surface, TRUE, TRUE, TRUE, TRUE);
  whelk_surface_set_namespace(wallpaper->surface, "whelk-wallpaper");
  whelk_surface_set_widget_id(wallpaper->surface, "whelk-wallpaper");

  if (monitor)
    whelk_surface_set_monitor(wallpaper->surface, monitor);

  GtkWidget *view = whelk_wallpaper_view_new();

  wallpaper->view = WHELK_WALLPAPER_VIEW(view);

  if (engine->config) {
    whelk_wallpaper_view_set_fit(wallpaper->view, engine->config->fit);
    whelk_wallpaper_view_set_ken_burns(wallpaper->view,
                                       engine->config->ken_burns);
  }

  whelk_surface_set_child(wallpaper->surface, view);
  whelk_surface_present(wallpaper->surface);

  g_ptr_array_add(engine->surfaces, wallpaper);

  /* A surface that appears after the current picture was chosen has to be
   * given it; one that was already there is left alone so a settings save does
   * not restart a movie. */
  load_surface(engine, wallpaper, engine->selected, FALSE);
}

/* Brings the surface set in line with the outputs.  Returns TRUE when the
 * surfaces were rebuilt, in which case they already show the current picture
 * and the caller must not load it a second time. */
static gboolean sync_surfaces(WhelkWallpaperEngine *engine) {
  GdkDisplay *display = gdk_display_get_default();
  GListModel *monitors = display ? gdk_display_get_monitors(display) : NULL;

  if (!engine->config || !engine->config->enabled || !engine->selected ||
      !monitors) {
    destroy_surfaces(engine);
    return FALSE;
  }

  if (!layer_shell_supported()) {
    if (!engine->explained_unsupported) {
      engine->explained_unsupported = TRUE;
      LOG_C_INFO("Wallpaper",
                 "no wlr-layer-shell on this display: the wallpaper engine "
                 "needs one (niri, sway, Hyprland, river); the picker still "
                 "works");
    }

    destroy_surfaces(engine);
    return FALSE;
  }

  const guint count = g_list_model_get_n_items(monitors);
  GString *signature = g_string_new(NULL);

  engine->max_width = 0;
  engine->max_height = 0;

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

    if (!monitor)
      continue;

    GdkRectangle geometry = {0};

    gdk_monitor_get_geometry(monitor, &geometry);

    engine->max_width = MAX(engine->max_width, geometry.width);
    engine->max_height = MAX(engine->max_height, geometry.height);

    g_string_append_printf(
        signature, "%s:%dx%d;",
        gdk_monitor_get_connector(monitor) ? gdk_monitor_get_connector(monitor)
                                           : "?",
        geometry.width, geometry.height);
    g_object_unref(monitor);
  }

  const gboolean unchanged =
      engine->surfaces->len == count && engine->monitor_signature &&
      g_strcmp0(engine->monitor_signature, signature->str) == 0;

  g_string_free(signature, TRUE);

  if (unchanged)
    return FALSE;

  destroy_surfaces(engine);

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

    if (!monitor)
      continue;

    create_surface(engine, monitor);
    g_object_unref(monitor);
  }

  /* Rebuild the signature after creation: destroy_surfaces() cleared it. */
  GString *rebuilt = g_string_new(NULL);

  for (guint i = 0; i < count; i++) {
    GdkMonitor *monitor = GDK_MONITOR(g_list_model_get_item(monitors, i));

    if (!monitor)
      continue;

    GdkRectangle geometry = {0};

    gdk_monitor_get_geometry(monitor, &geometry);
    g_string_append_printf(
        rebuilt, "%s:%dx%d;",
        gdk_monitor_get_connector(monitor) ? gdk_monitor_get_connector(monitor)
                                           : "?",
        geometry.width, geometry.height);
    g_object_unref(monitor);
  }

  engine->monitor_signature = g_string_free(rebuilt, FALSE);

  return TRUE;
}

/* =========================================================================
 * THE CLOCK
 * ========================================================================= */

static void rotate_once(WhelkWallpaperEngine *engine) {
  if (!engine->config || engine->items->len == 0 || !engine->selected)
    return;

  guint current = 0;

  for (guint i = 0; i < engine->items->len; i++) {
    WhelkWallpaperItem *item = g_ptr_array_index(engine->items, i);

    if (g_strcmp0(item->path, engine->selected) == 0) {
      current = i;
      break;
    }
  }

  const guint next = whelk_wallpaper_next_index(
      engine->items->len, current, engine->config->shuffle,
      &engine->shuffle_state);
  WhelkWallpaperItem *item = g_ptr_array_index(engine->items, next);

  whelk_wallpaper_engine_apply(engine, item->path, TRUE);
}

static gboolean on_rotate(gpointer user_data) {
  WhelkWallpaperEngine *engine = user_data;

  engine->rotate_timeout = 0;
  rotate_once(engine);

  return G_SOURCE_REMOVE;
}

static void restart_rotation(WhelkWallpaperEngine *engine) {
  if (engine->rotate_timeout != 0) {
    g_source_remove(engine->rotate_timeout);
    engine->rotate_timeout = 0;
  }

  if (!engine->config || !engine->config->enabled ||
      engine->config->interval_seconds <= 0 || engine->items->len < 2 ||
      !engine->selected)
    return;

  engine->rotate_timeout =
      g_timeout_add_seconds((guint)engine->config->interval_seconds, on_rotate,
                            engine);
}

/* =========================================================================
 * MONITORS
 * ========================================================================= */

static gboolean on_monitors_idle(gpointer user_data) {
  WhelkWallpaperEngine *engine = user_data;

  engine->monitor_idle = 0;

  if (sync_surfaces(engine)) {
    /* The new surfaces were given the current picture by create_surface(). */
    LOG_C_DEBUG("Wallpaper", "rebuilt the background for the new outputs");
  }

  return G_SOURCE_REMOVE;
}

static void on_monitors_changed(GListModel *monitors, guint position,
                                guint removed, guint added, gpointer data) {
  (void)monitors;
  (void)position;
  (void)removed;
  (void)added;

  WhelkWallpaperEngine *engine = data;

  if (engine->monitor_idle == 0)
    engine->monitor_idle = g_idle_add(on_monitors_idle, engine);
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

WhelkWallpaperEngine *whelk_wallpaper_engine_new(GtkApplication *app) {
  WhelkWallpaperEngine *engine = g_new0(WhelkWallpaperEngine, 1);

  engine->app = app;
  engine->config = whelk_wallpaper_config_new();
  engine->items = g_ptr_array_new_with_free_func(wallpaper_item_free);
  engine->surfaces = g_ptr_array_new_with_free_func(wallpaper_surface_free);
  engine->shuffle_state = ((guint64)g_random_int() << 32) ^ g_random_int();

  engine->skin = gtk_css_provider_new();
  gtk_css_provider_load_from_string(
      engine->skin,
      "window#whelk-wallpaper { background-color: #000000; }\n");

  GdkDisplay *display = gdk_display_get_default();

  if (display) {
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(engine->skin),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    engine->monitors = g_object_ref(gdk_display_get_monitors(display));

    if (engine->monitors)
      engine->monitor_handler = g_signal_connect(
          engine->monitors, "items-changed", G_CALLBACK(on_monitors_changed),
          engine);
  }

  return engine;
}

void whelk_wallpaper_engine_set_config(WhelkWallpaperEngine *engine,
                                       const WhelkWallpaperConfig *config) {
  if (!engine || !config)
    return;

  g_autofree char *previous_current =
      g_strdup(engine->config ? engine->config->current : NULL);
  g_autofree char *previous_selected = g_strdup(engine->selected);
  WhelkWallpaperConfig *copy = whelk_wallpaper_config_copy(config);

  whelk_wallpaper_config_normalize(copy);
  g_clear_pointer(&engine->config, whelk_wallpaper_config_free);
  engine->config = copy;

  rescan(engine);

  /* A picker choice survives a reload; an explicit #:current that changed is
   * honoured when the selected file was that old value and nothing else. */
  if (!engine->selected) {
    engine->selected = expand_path(engine->config->current);
  } else if (g_strcmp0(previous_current, engine->config->current) != 0 &&
             g_strcmp0(engine->selected, previous_current) == 0) {
    g_free(engine->selected);
    engine->selected = expand_path(engine->config->current);
  }

  resolve_selected(engine);

  for (guint i = 0; i < engine->surfaces->len; i++) {
    WallpaperSurface *wallpaper = g_ptr_array_index(engine->surfaces, i);

    whelk_wallpaper_view_set_fit(wallpaper->view, engine->config->fit);
    whelk_wallpaper_view_set_ken_burns(wallpaper->view,
                                       engine->config->ken_burns);
  }

  const gboolean rebuilt = sync_surfaces(engine);

  /* A save that changed a colour, a bar or an interval must not restart the
   * movie someone is watching; only a different picture is loaded. */
  if (!rebuilt && engine->surfaces->len > 0 &&
      g_strcmp0(previous_selected, engine->selected) != 0)
    apply_current(engine, TRUE);

  restart_rotation(engine);
}

void whelk_wallpaper_engine_set_config_path(WhelkWallpaperEngine *engine,
                                            const char *path) {
  if (!engine)
    return;

  g_free(engine->config_path);
  engine->config_path = g_strdup(path);
}

void whelk_wallpaper_engine_refresh(WhelkWallpaperEngine *engine) {
  if (!engine)
    return;

  g_autofree char *previous = g_strdup(engine->selected);

  rescan(engine);
  resolve_selected(engine);

  const gboolean rebuilt = sync_surfaces(engine);

  if (!rebuilt && engine->surfaces->len > 0 &&
      g_strcmp0(previous, engine->selected) != 0)
    apply_current(engine, TRUE);

  restart_rotation(engine);
}

gboolean whelk_wallpaper_engine_apply(WhelkWallpaperEngine *engine,
                                      const char *path, gboolean persist) {
  if (!engine || !engine->config || !path || *path == '\0')
    return FALSE;

  g_autofree char *expanded = expand_path(path);

  if (!path_is_usable(engine, expanded)) {
    LOG_C_WARN("Wallpaper", "'%s' is not a wallpaper whelk can show", path);
    return FALSE;
  }

  if (g_strcmp0(engine->selected, expanded) == 0) {
    restart_rotation(engine);

    if (persist && engine->config_path)
      whelk_bridge_save_preference_string(engine->config_path,
                                          "wallpaper-selected", expanded);

    return TRUE;
  }

  g_free(engine->selected);
  engine->selected = g_strdup(expanded);

  const gboolean rebuilt = sync_surfaces(engine);

  if (!rebuilt)
    apply_current(engine, TRUE);

  restart_rotation(engine);

  if (persist && engine->config_path &&
      !whelk_bridge_save_preference_string(engine->config_path,
                                           "wallpaper-selected", expanded))
    LOG_C_WARN("Wallpaper", "the choice could not be saved; it lasts until "
                            "the shell reloads");

  LOG_C_INFO("Wallpaper", "showing '%s'", expanded);

  return TRUE;
}

GPtrArray *whelk_wallpaper_engine_items(WhelkWallpaperEngine *engine) {
  return engine ? engine->items : NULL;
}

const WhelkWallpaperConfig *whelk_wallpaper_engine_config(
    WhelkWallpaperEngine *engine) {
  return engine ? engine->config : NULL;
}

const char *whelk_wallpaper_engine_current(WhelkWallpaperEngine *engine) {
  return engine ? engine->selected : NULL;
}

gboolean whelk_wallpaper_engine_is_active(WhelkWallpaperEngine *engine) {
  return engine && engine->config && engine->config->enabled &&
         engine->surfaces && engine->surfaces->len > 0;
}

void whelk_wallpaper_engine_free(WhelkWallpaperEngine *engine) {
  if (!engine)
    return;

  if (engine->rotate_timeout != 0)
    g_source_remove(engine->rotate_timeout);

  if (engine->monitor_idle != 0)
    g_source_remove(engine->monitor_idle);

  if (engine->monitors) {
    if (engine->monitor_handler != 0)
      g_signal_handler_disconnect(engine->monitors, engine->monitor_handler);

    g_clear_object(&engine->monitors);
  }

  destroy_surfaces(engine);
  g_clear_pointer(&engine->surfaces, g_ptr_array_unref);
  g_clear_pointer(&engine->items, g_ptr_array_unref);
  g_clear_pointer(&engine->config, whelk_wallpaper_config_free);

  GdkDisplay *display = gdk_display_get_default();

  if (display && engine->skin)
    gtk_style_context_remove_provider_for_display(
        display, GTK_STYLE_PROVIDER(engine->skin));

  g_clear_object(&engine->skin);
  g_free(engine->config_path);
  g_free(engine->selected);
  g_free(engine->monitor_signature);
  g_free(engine);
}

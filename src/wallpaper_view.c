// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "wallpaper_view.h"

#include <math.h>

/* How long one Ken Burns breath takes.  Slow enough that the wallpaper is
 * still a background, not an animation somebody watches. */
#define WHELK_WALLPAPER_KEN_BURNS_US (60 * G_TIME_SPAN_SECOND)
#define WHELK_WALLPAPER_KEN_BURNS_ZOOM 0.06

struct _WhelkWallpaperView {
  GtkWidget parent_instance;

  GdkPaintable *current;
  GdkPaintable *previous;
  WhelkWallpaperAnimation animation;
  WhelkWallpaperFit fit;
  guint duration_ms;
  gint64 transition_started;
  double progress;
  gboolean transitioning;
  gboolean ken_burns;
  gint64 ken_burns_started;
  gint64 last_draw;
  guint tick_id;
};

G_DEFINE_TYPE(WhelkWallpaperView, whelk_wallpaper_view, GTK_TYPE_WIDGET)

static void whelk_wallpaper_view_snapshot(GtkWidget *widget,
                                          GtkSnapshot *snapshot);

static void whelk_wallpaper_view_dispose(GObject *object) {
  WhelkWallpaperView *self = WHELK_WALLPAPER_VIEW(object);

  if (self->tick_id != 0) {
    gtk_widget_remove_tick_callback(GTK_WIDGET(self), self->tick_id);
    self->tick_id = 0;
  }

  g_clear_object(&self->current);
  g_clear_object(&self->previous);

  G_OBJECT_CLASS(whelk_wallpaper_view_parent_class)->dispose(object);
}

static void whelk_wallpaper_view_class_init(WhelkWallpaperViewClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);
  GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

  object_class->dispose = whelk_wallpaper_view_dispose;

  /* The widget takes the whole surface; a wallpaper that measured itself
   * would be a small picture in the corner. */
  widget_class->snapshot = whelk_wallpaper_view_snapshot;
}

static void whelk_wallpaper_view_init(WhelkWallpaperView *self) {
  self->animation = WHELK_WALLPAPER_ANIMATION_CROSSFADE;
  self->fit = WHELK_WALLPAPER_FIT_COVER;
  self->ken_burns_started = g_get_monotonic_time();
  gtk_widget_add_css_class(GTK_WIDGET(self), "whelk-wallpaper-view");
  gtk_widget_set_hexpand(GTK_WIDGET(self), TRUE);
  gtk_widget_set_vexpand(GTK_WIDGET(self), TRUE);
}

GtkWidget *whelk_wallpaper_view_new(void) {
  return g_object_new(WHELK_TYPE_WALLPAPER_VIEW, NULL);
}

/* The zoom Ken Burns is at now: one slow breath from the fit to 6% larger
 * and back, centred on the picture. */
static double ken_burns_zoom(WhelkWallpaperView *self) {
  if (!self->ken_burns)
    return 1.0;

  const gint64 elapsed = g_get_monotonic_time() - self->ken_burns_started;
  const double phase =
      fmod((double)elapsed / (double)WHELK_WALLPAPER_KEN_BURNS_US, 1.0);
  const double breath = 0.5 - 0.5 * cos(phase * 2.0 * G_PI);

  return 1.0 + WHELK_WALLPAPER_KEN_BURNS_ZOOM * breath;
}

static gboolean on_tick(GtkWidget *widget, GdkFrameClock *clock,
                        gpointer user_data) {
  WhelkWallpaperView *self = WHELK_WALLPAPER_VIEW(widget);

  (void)clock;
  (void)user_data;

  const gboolean was_transitioning = self->transitioning;
  const gint64 now = g_get_monotonic_time();

  if (self->transitioning) {
    const double elapsed =
        (double)(now - self->transition_started) / 1000.0;

    self->progress = self->duration_ms > 0
                         ? MIN(1.0, elapsed / (double)self->duration_ms)
                         : 1.0;

    if (self->progress >= 1.0) {
      self->transitioning = FALSE;
      g_clear_object(&self->previous);
    }
  }

  /* A transition wants every frame the display offers.  Ken Burns moves a
   * fraction of a percent per second; thirty frames a second is already more
   * than the eye can tell from sixty, and it leaves the CPU for real work. */
  if (was_transitioning ||
      now - self->last_draw >= 33 * G_TIME_SPAN_MILLISECOND) {
    gtk_widget_queue_draw(widget);
    self->last_draw = now;
  }

  if (!self->transitioning && !(self->ken_burns && self->current)) {
    self->tick_id = 0;
    return G_SOURCE_REMOVE;
  }

  return G_SOURCE_CONTINUE;
}

static void ensure_tick(WhelkWallpaperView *self) {
  if (self->tick_id == 0)
    self->tick_id = gtk_widget_add_tick_callback(GTK_WIDGET(self), on_tick,
                                                 NULL, NULL);
}

/* Draws PAINTABLE into the view with the configured fit, then the transition's
 * offset, zoom and opacity.  The intrinsic size is used when the paintable
 * reports one and the view's own size otherwise, which is the case while a
 * movie has not decoded its first frame yet. */
static void draw_paintable(WhelkWallpaperView *self, GdkPaintable *paintable,
                           GtkSnapshot *snapshot, double view_width,
                           double view_height, double offset_x,
                           double offset_y, double zoom, double alpha) {
  if (!paintable || alpha <= 0.0)
    return;

  int image_width = gdk_paintable_get_intrinsic_width(paintable);
  int image_height = gdk_paintable_get_intrinsic_height(paintable);

  if (image_width <= 0 || image_height <= 0) {
    image_width = MAX(1, (int)view_width);
    image_height = MAX(1, (int)view_height);
  }

  double scale_x = 1.0;
  double scale_y = 1.0;
  double fit_x = 0.0;
  double fit_y = 0.0;

  whelk_wallpaper_fit(self->fit, view_width, view_height, image_width,
                      image_height, zoom, &scale_x, &scale_y, &fit_x, &fit_y);

  gtk_snapshot_push_opacity(snapshot, CLAMP(alpha, 0.0, 1.0));
  gtk_snapshot_save(snapshot);
  gtk_snapshot_translate(snapshot,
                         &GRAPHENE_POINT_INIT(offset_x + fit_x,
                                              offset_y + fit_y));
  gtk_snapshot_scale(snapshot, scale_x, scale_y);
  gdk_paintable_snapshot(paintable, snapshot, image_width, image_height);
  gtk_snapshot_restore(snapshot);
  gtk_snapshot_pop(snapshot);
}

static void whelk_wallpaper_view_snapshot(GtkWidget *widget,
                                          GtkSnapshot *snapshot) {
  WhelkWallpaperView *self = WHELK_WALLPAPER_VIEW(widget);
  const int width = gtk_widget_get_width(widget);
  const int height = gtk_widget_get_height(widget);

  if (width <= 0 || height <= 0)
    return;

  /* A wallpaper is opaque; the black underneath is what a fade goes through
   * and what a letterboxed contain shows. */
  const graphene_rect_t full = GRAPHENE_RECT_INIT(0, 0, width, height);
  const GdkRGBA black = {0.0, 0.0, 0.0, 1.0};

  gtk_snapshot_append_color(snapshot, &black, &full);

  if (!self->current)
    return;

  const double progress =
      self->transitioning ? whelk_wallpaper_ease(self->progress) : 1.0;
  double out_x = 0.0;
  double out_y = 0.0;
  double in_x = 0.0;
  double in_y = 0.0;

  if (self->transitioning) {
    whelk_wallpaper_offsets(self->animation, progress, (double)width,
                            (double)height, &out_x, &out_y, &in_x, &in_y);

    if (self->previous)
      draw_paintable(self, self->previous, snapshot, (double)width,
                     (double)height, out_x, out_y, 1.0,
                     whelk_wallpaper_alpha_out(self->animation, progress));
  }

  const double zoom =
      self->transitioning
          ? whelk_wallpaper_zoom_in(self->animation, progress)
          : ken_burns_zoom(self);

  draw_paintable(self, self->current, snapshot, (double)width, (double)height,
                 in_x, in_y, zoom,
                 self->transitioning
                     ? whelk_wallpaper_alpha_in(self->animation, progress)
                     : 1.0);
}

void whelk_wallpaper_view_set_paintable(WhelkWallpaperView *self,
                                        GdkPaintable *paintable,
                                        WhelkWallpaperAnimation animation,
                                        guint duration_ms) {
  if (!self)
    return;

  if (self->current == paintable) {
    gtk_widget_queue_draw(GTK_WIDGET(self));
    return;
  }

  g_set_object(&self->previous, self->current);
  g_set_object(&self->current, paintable);

  self->animation = animation;
  self->duration_ms = duration_ms;
  self->progress = 0.0;
  self->transition_started = g_get_monotonic_time();
  self->ken_burns_started = self->transition_started;
  self->last_draw = 0;

  /* The first picture has nothing to transition from. */
  self->transitioning = paintable && self->previous &&
                        animation != WHELK_WALLPAPER_ANIMATION_NONE &&
                        duration_ms > 0;

  if (!self->transitioning)
    g_clear_object(&self->previous);

  /* A still picture with Ken Burns on needs frames of its own, even when
   * there was nothing to transition from. */
  if (self->transitioning || (self->ken_burns && self->current))
    ensure_tick(self);

  gtk_widget_queue_draw(GTK_WIDGET(self));
}

void whelk_wallpaper_view_set_frame(WhelkWallpaperView *self,
                                    GdkPaintable *paintable) {
  if (!self || self->current == paintable)
    return;

  g_set_object(&self->current, paintable);

  /* A frame is not a new wallpaper: Ken Burns keeps its place in the slow
   * breath instead of restarting at the beginning of the zoom. */
  gtk_widget_queue_draw(GTK_WIDGET(self));
}

void whelk_wallpaper_view_set_fit(WhelkWallpaperView *self,
                                  WhelkWallpaperFit fit) {
  if (!self)
    return;

  self->fit = fit;
  gtk_widget_queue_draw(GTK_WIDGET(self));
}

void whelk_wallpaper_view_set_ken_burns(WhelkWallpaperView *self,
                                        gboolean enabled) {
  if (!self)
    return;

  self->ken_burns = enabled;
  self->ken_burns_started = g_get_monotonic_time();

  if (enabled && self->current)
    ensure_tick(self);

  gtk_widget_queue_draw(GTK_WIDGET(self));
}

void whelk_wallpaper_view_clear(WhelkWallpaperView *self) {
  if (!self)
    return;

  self->transitioning = FALSE;
  self->progress = 0.0;
  g_clear_object(&self->previous);
  g_clear_object(&self->current);
  gtk_widget_queue_draw(GTK_WIDGET(self));
}

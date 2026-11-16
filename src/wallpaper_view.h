// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WALLPAPER_VIEW_H
#define WHELK_WALLPAPER_VIEW_H

#include "util/wallpaper.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE WALLPAPER VIEW
 * =========================================================================
 * One widget that draws one wallpaper and the transition to the next one.
 * Two paintables are enough for every transition here: the outgoing picture,
 * the incoming one, an opacity and an offset each.  The widget owns both, so
 * the engine can hand over a texture or a GtkMediaStream and forget it.
 *
 * A GdkPaintable invalidates itself when it has a new frame (a movie, a
 * media stream) and the view repaints; an animated image is driven by the
 * engine, which calls whelk_wallpaper_view_set_frame().
 */

#define WHELK_TYPE_WALLPAPER_VIEW (whelk_wallpaper_view_get_type())
G_DECLARE_FINAL_TYPE(WhelkWallpaperView, whelk_wallpaper_view, WHELK,
                     WALLPAPER_VIEW, GtkWidget)

GtkWidget *whelk_wallpaper_view_new(void);

/* Starts a transition from the current picture to @paintable.  With no
 * current picture, or with ANIMATION_NONE or a zero duration, the new
 * picture appears at once. */
void whelk_wallpaper_view_set_paintable(WhelkWallpaperView *view,
                                        GdkPaintable *paintable,
                                        WhelkWallpaperAnimation animation,
                                        guint duration_ms);

/* Replaces the picture on screen without a transition: an animated image's
 * next frame, or a movie that is already the current paintable. */
void whelk_wallpaper_view_set_frame(WhelkWallpaperView *view,
                                    GdkPaintable *paintable);

void whelk_wallpaper_view_set_fit(WhelkWallpaperView *view,
                                  WhelkWallpaperFit fit);

/* A slow, continuous zoom for a still picture.  A movie is not zoomed: the
 * picture is already moving, and scaling it every frame is a cost with no
 * meaning. */
void whelk_wallpaper_view_set_ken_burns(WhelkWallpaperView *view,
                                        gboolean enabled);

/* Drops both pictures, leaving the black surface. */
void whelk_wallpaper_view_clear(WhelkWallpaperView *view);

G_END_DECLS

#endif // WHELK_WALLPAPER_VIEW_H

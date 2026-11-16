// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WALLPAPER_H
#define WHELK_WALLPAPER_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE WALLPAPER, AS DATA
 * =========================================================================
 * A wallpaper engine is a collection of files, a clock that moves through it,
 * a way to fit one picture into one output, and a transition between two
 * pictures.  All of that is arithmetic and string matching, so it lives here,
 * away from GTK and away from the display server, and is tested without a
 * screen.
 *
 * The view (src/wallpaper_view.c) turns the numbers below into a snapshot; the
 * engine (src/wallpaper.c) decides which file is current.  Neither one owns a
 * rule about what a slide looks like or which extensions are pictures.
 */

typedef enum {
  WHELK_WALLPAPER_IMAGE,
  WHELK_WALLPAPER_VIDEO,
  WHELK_WALLPAPER_UNKNOWN,
} WhelkWallpaperKind;

typedef enum {
  WHELK_WALLPAPER_ANIMATION_NONE,
  WHELK_WALLPAPER_ANIMATION_CROSSFADE,
  WHELK_WALLPAPER_ANIMATION_FADE,
  WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT,
  WHELK_WALLPAPER_ANIMATION_SLIDE_RIGHT,
  WHELK_WALLPAPER_ANIMATION_SLIDE_UP,
  WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN,
  WHELK_WALLPAPER_ANIMATION_ZOOM,
} WhelkWallpaperAnimation;

typedef enum {
  WHELK_WALLPAPER_FIT_COVER,
  WHELK_WALLPAPER_FIT_CONTAIN,
  WHELK_WALLPAPER_FIT_FILL,
} WhelkWallpaperFit;

/* The kind from the file's extension, case-insensitively.  A file with no
 * extension is UNKNOWN, not a guess. */
WhelkWallpaperKind whelk_wallpaper_kind_for_path(const char *path);

/* IMAGE or VIDEO: the two kinds the collection keeps. */
gboolean whelk_wallpaper_is_supported(const char *path);

const char *whelk_wallpaper_animation_name(WhelkWallpaperAnimation animation);

/* Parses the names a settings file writes: none, crossfade, fade,
 * slide-left, slide-right, slide-up, slide-down, zoom.  Returns FALSE and
 * leaves @animation untouched for anything else. */
gboolean whelk_wallpaper_animation_parse(const char *text,
                                         WhelkWallpaperAnimation *animation);

const char *whelk_wallpaper_fit_name(WhelkWallpaperFit fit);

/* cover, contain or fill. */
gboolean whelk_wallpaper_fit_parse(const char *text, WhelkWallpaperFit *fit);

/* Smoothstep: 0 at 0, 1 at 1, and no visible kick at either end. */
double whelk_wallpaper_ease(double progress);

/* The incoming picture's opacity at PROGRESS (0..1).  A slide keeps both
 * pictures opaque; a crossfade raises the new one over the old; a fade first
 * takes the old one to black and then brings the new one up. */
double whelk_wallpaper_alpha_in(WhelkWallpaperAnimation animation,
                                double progress);

/* The outgoing picture's opacity at PROGRESS. */
double whelk_wallpaper_alpha_out(WhelkWallpaperAnimation animation,
                                 double progress);

/* The incoming picture's extra scale for the zoom transition, 1.0 when the
 * animation does not zoom. */
double whelk_wallpaper_zoom_in(WhelkWallpaperAnimation animation,
                               double progress);

/* Where the outgoing and incoming pictures sit during a slide, relative to
 * the surface's top-left corner.  Offsets are 0 for every other animation. */
void whelk_wallpaper_offsets(WhelkWallpaperAnimation animation, double progress,
                             double width, double height, double *out_x,
                             double *out_y, double *in_x, double *in_y);

/* The scale and offset that fit an IMAGE_WIDTH x IMAGE_HEIGHT picture into a
 * VIEW_WIDTH x VIEW_HEIGHT surface.  ZOOM multiplies the fit (1.0 is exact);
 * the offsets centre the scaled picture, so cover crops both sides equally. */
void whelk_wallpaper_fit(WhelkWallpaperFit fit, double view_width,
                         double view_height, double image_width,
                         double image_height, double zoom, double *scale_x,
                         double *scale_y, double *offset_x, double *offset_y);

/* The next item in a collection.  Sequential wraps around; shuffled never
 * repeats the current index and advances a caller-owned xorshift state, which
 * makes the order reproducible in a test. */
guint whelk_wallpaper_next_index(guint count, guint current, gboolean shuffle,
                                 guint64 *state);

G_END_DECLS

#endif // WHELK_WALLPAPER_H

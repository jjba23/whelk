// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/wallpaper.h"

#include <math.h>
#include <string.h>

static gboolean has_extension(const char *path, const char *const *extensions) {
  if (!path)
    return FALSE;

  const char *dot = strrchr(path, '.');

  if (!dot || dot[1] == '\0')
    return FALSE;

  for (guint i = 0; extensions[i]; i++) {
    if (g_ascii_strcasecmp(dot + 1, extensions[i]) == 0)
      return TRUE;
  }

  return FALSE;
}

static const char *const image_extensions[] = {
    "png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff", "svg",
    NULL};

static const char *const video_extensions[] = {
    "mp4", "webm", "mkv", "mov", "m4v", "ogv", "avi", "mpg", "mpeg", NULL};

WhelkWallpaperKind whelk_wallpaper_kind_for_path(const char *path) {
  if (has_extension(path, image_extensions))
    return WHELK_WALLPAPER_IMAGE;

  if (has_extension(path, video_extensions))
    return WHELK_WALLPAPER_VIDEO;

  return WHELK_WALLPAPER_UNKNOWN;
}

gboolean whelk_wallpaper_is_supported(const char *path) {
  return whelk_wallpaper_kind_for_path(path) != WHELK_WALLPAPER_UNKNOWN;
}

static const struct {
  const char *name;
  WhelkWallpaperAnimation animation;
} animation_names[] = {
    {"none", WHELK_WALLPAPER_ANIMATION_NONE},
    {"crossfade", WHELK_WALLPAPER_ANIMATION_CROSSFADE},
    {"fade", WHELK_WALLPAPER_ANIMATION_FADE},
    {"slide-left", WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT},
    {"slide-right", WHELK_WALLPAPER_ANIMATION_SLIDE_RIGHT},
    {"slide-up", WHELK_WALLPAPER_ANIMATION_SLIDE_UP},
    {"slide-down", WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN},
    {"zoom", WHELK_WALLPAPER_ANIMATION_ZOOM},
};

const char *whelk_wallpaper_animation_name(WhelkWallpaperAnimation animation) {
  for (guint i = 0; i < G_N_ELEMENTS(animation_names); i++) {
    if (animation_names[i].animation == animation)
      return animation_names[i].name;
  }

  return "crossfade";
}

gboolean whelk_wallpaper_animation_parse(const char *text,
                                         WhelkWallpaperAnimation *animation) {
  if (!text || !animation)
    return FALSE;

  for (guint i = 0; i < G_N_ELEMENTS(animation_names); i++) {
    if (g_ascii_strcasecmp(text, animation_names[i].name) == 0) {
      *animation = animation_names[i].animation;
      return TRUE;
    }
  }

  return FALSE;
}

const char *whelk_wallpaper_fit_name(WhelkWallpaperFit fit) {
  switch (fit) {
  case WHELK_WALLPAPER_FIT_CONTAIN:
    return "contain";
  case WHELK_WALLPAPER_FIT_FILL:
    return "fill";
  case WHELK_WALLPAPER_FIT_COVER:
  default:
    return "cover";
  }
}

gboolean whelk_wallpaper_fit_parse(const char *text, WhelkWallpaperFit *fit) {
  if (!text || !fit)
    return FALSE;

  if (g_ascii_strcasecmp(text, "cover") == 0)
    *fit = WHELK_WALLPAPER_FIT_COVER;
  else if (g_ascii_strcasecmp(text, "contain") == 0)
    *fit = WHELK_WALLPAPER_FIT_CONTAIN;
  else if (g_ascii_strcasecmp(text, "fill") == 0)
    *fit = WHELK_WALLPAPER_FIT_FILL;
  else
    return FALSE;

  return TRUE;
}

double whelk_wallpaper_ease(double progress) {
  const double t = CLAMP(progress, 0.0, 1.0);

  return t * t * (3.0 - 2.0 * t);
}

double whelk_wallpaper_alpha_in(WhelkWallpaperAnimation animation,
                                double progress) {
  const double t = CLAMP(progress, 0.0, 1.0);

  switch (animation) {
  case WHELK_WALLPAPER_ANIMATION_NONE:
  case WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT:
  case WHELK_WALLPAPER_ANIMATION_SLIDE_RIGHT:
  case WHELK_WALLPAPER_ANIMATION_SLIDE_UP:
  case WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN:
    return 1.0;
  case WHELK_WALLPAPER_ANIMATION_FADE:
    return t < 0.5 ? 0.0 : (t - 0.5) * 2.0;
  case WHELK_WALLPAPER_ANIMATION_CROSSFADE:
  case WHELK_WALLPAPER_ANIMATION_ZOOM:
  default:
    return t;
  }
}

double whelk_wallpaper_alpha_out(WhelkWallpaperAnimation animation,
                                 double progress) {
  const double t = CLAMP(progress, 0.0, 1.0);

  if (animation == WHELK_WALLPAPER_ANIMATION_FADE)
    return t < 0.5 ? 1.0 - t * 2.0 : 0.0;

  if (animation == WHELK_WALLPAPER_ANIMATION_CROSSFADE ||
      animation == WHELK_WALLPAPER_ANIMATION_ZOOM ||
      animation == WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT ||
      animation == WHELK_WALLPAPER_ANIMATION_SLIDE_RIGHT ||
      animation == WHELK_WALLPAPER_ANIMATION_SLIDE_UP ||
      animation == WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN)
    return 1.0;

  return 0.0;
}

double whelk_wallpaper_zoom_in(WhelkWallpaperAnimation animation,
                               double progress) {
  if (animation != WHELK_WALLPAPER_ANIMATION_ZOOM)
    return 1.0;

  return 1.0 + 0.12 * (1.0 - CLAMP(progress, 0.0, 1.0));
}

void whelk_wallpaper_offsets(WhelkWallpaperAnimation animation, double progress,
                             double width, double height, double *out_x,
                             double *out_y, double *in_x, double *in_y) {
  const double t = CLAMP(progress, 0.0, 1.0);

  *out_x = 0.0;
  *out_y = 0.0;
  *in_x = 0.0;
  *in_y = 0.0;

  switch (animation) {
  case WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT:
    *out_x = -width * t;
    *in_x = width * (1.0 - t);
    break;
  case WHELK_WALLPAPER_ANIMATION_SLIDE_RIGHT:
    *out_x = width * t;
    *in_x = -width * (1.0 - t);
    break;
  case WHELK_WALLPAPER_ANIMATION_SLIDE_UP:
    *out_y = -height * t;
    *in_y = height * (1.0 - t);
    break;
  case WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN:
    *out_y = height * t;
    *in_y = -height * (1.0 - t);
    break;
  default:
    break;
  }
}

void whelk_wallpaper_fit(WhelkWallpaperFit fit, double view_width,
                         double view_height, double image_width,
                         double image_height, double zoom, double *scale_x,
                         double *scale_y, double *offset_x, double *offset_y) {
  double sx = 1.0;
  double sy = 1.0;

  if (view_width <= 0.0 || view_height <= 0.0 || image_width <= 0.0 ||
      image_height <= 0.0) {
    *scale_x = 1.0;
    *scale_y = 1.0;
    *offset_x = 0.0;
    *offset_y = 0.0;
    return;
  }

  const double z = zoom > 0.0 ? zoom : 1.0;

  switch (fit) {
  case WHELK_WALLPAPER_FIT_CONTAIN:
    sx = sy = MIN(view_width / image_width, view_height / image_height) * z;
    break;
  case WHELK_WALLPAPER_FIT_FILL:
    sx = view_width / image_width * z;
    sy = view_height / image_height * z;
    break;
  case WHELK_WALLPAPER_FIT_COVER:
  default:
    sx = sy = MAX(view_width / image_width, view_height / image_height) * z;
    break;
  }

  *scale_x = sx;
  *scale_y = sy;
  *offset_x = (view_width - image_width * sx) / 2.0;
  *offset_y = (view_height - image_height * sy) / 2.0;
}

guint whelk_wallpaper_next_index(guint count, guint current, gboolean shuffle,
                                 guint64 *state) {
  if (count <= 1)
    return 0;

  if (!shuffle)
    return (current + 1) % count;

  guint64 x = (state && *state) ? *state : 0x9e3779b97f4a7c15ULL;

  x ^= x >> 12;
  x ^= x << 25;
  x ^= x >> 27;

  if (state)
    *state = x;

  const guint candidate =
      (guint)((x * 0x2545f4914f6cdd1dULL) >> 33) % count;

  /* A rotation that can land on the picture already on screen is not a
   * rotation; step over it. */
  return (candidate == current) ? (candidate + 1) % count : candidate;
}

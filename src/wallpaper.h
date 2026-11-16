// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WALLPAPER_ENGINE_H
#define WHELK_WALLPAPER_ENGINE_H

#include "util/wallpaper.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE WALLPAPER ENGINE
 * =========================================================================
 * One engine owns the whole background: the collection the settings file
 * named, the picture currently selected, a timer that moves through the
 * collection, and one background layer surface per output.  It draws the
 * picture itself -- there is no swaybg, no feh, no external daemon -- so the
 * same collection, transition and rotation are the shell's own configuration.
 *
 * The picker opens and closes; the engine does not.  A reload hands the engine
 * a new configuration and it reconciles its surfaces, its collection and its
 * timer in place, so saving settings.scm does not flash the desktop.
 */

/**
 * WhelkWallpaperConfig:
 * @configured: whether the settings file described a wallpaper at all.
 * @enabled: whether whelk should draw the background.
 * @paths: (element-type utf8): directories and files to collect.
 * @interval_seconds: seconds between automatic changes; 0 stops the clock.
 * @shuffle: visit the collection in a random order instead of its sorted one.
 * @animation: the transition between two pictures.
 * @animation_ms: how long that transition takes.
 * @fit: cover, contain or fill.
 * @ken_burns: slowly zoom a still picture while it is on screen.
 * @video: whether movie files are part of the collection and may be shown.
 * @mute_video: play a movie wallpaper without its soundtrack.
 * @current: (nullable): a file to start on; the picker's choice is saved as a
 *   GUI preference and wins over this after the first selection.
 * @picker_width: the picker card's compact width in pixels.
 * @picker_height: the picker card's compact height in pixels.
 * @picker_columns: thumbnails per row in the compact card.
 *
 * One wallpaper policy, resolved from `(wallpaper-settings ...)'.
 */
typedef struct {
  gboolean configured;
  gboolean enabled;
  GPtrArray *paths;
  int interval_seconds;
  gboolean shuffle;
  WhelkWallpaperAnimation animation;
  int animation_ms;
  WhelkWallpaperFit fit;
  gboolean ken_burns;
  gboolean video;
  gboolean mute_video;
  char *current;
  int picker_width;
  int picker_height;
  int picker_columns;
} WhelkWallpaperConfig;

WhelkWallpaperConfig *whelk_wallpaper_config_new(void);
WhelkWallpaperConfig *whelk_wallpaper_config_copy(
    const WhelkWallpaperConfig *config);
void whelk_wallpaper_config_free(WhelkWallpaperConfig *config);
void whelk_wallpaper_config_normalize(WhelkWallpaperConfig *config);

G_DEFINE_AUTOPTR_CLEANUP_FUNC(WhelkWallpaperConfig,
                              whelk_wallpaper_config_free)

/**
 * WhelkWallpaperItem:
 * @path: (owned by the engine): the file.
 * @kind: image or video.
 *
 * One entry of the current collection, as the picker reads it.  The engine
 * owns the array; a caller must not free an item or keep it across a reload.
 */
typedef struct {
  char *path;
  WhelkWallpaperKind kind;
} WhelkWallpaperItem;

typedef struct _WhelkWallpaperEngine WhelkWallpaperEngine;

WhelkWallpaperEngine *whelk_wallpaper_engine_new(GtkApplication *app);
void whelk_wallpaper_engine_free(WhelkWallpaperEngine *engine);

/* Replaces the policy, reconciles the surfaces and restarts the rotation. */
void whelk_wallpaper_engine_set_config(WhelkWallpaperEngine *engine,
                                       const WhelkWallpaperConfig *config);

/* The settings file the picker's choice is saved against. */
void whelk_wallpaper_engine_set_config_path(WhelkWallpaperEngine *engine,
                                            const char *path);

/* Rescans the configured directories, so the picker sees files added since
 * the settings file was last loaded. */
void whelk_wallpaper_engine_refresh(WhelkWallpaperEngine *engine);

/* Shows PATH now.  With @persist the choice is written to the GUI preference
 * file, which is what the picker and the rotation want; a test or a one-off
 * call leaves it alone.  Returns FALSE for a missing or unsupported file. */
gboolean whelk_wallpaper_engine_apply(WhelkWallpaperEngine *engine,
                                      const char *path, gboolean persist);

/* The current collection, owned by the engine.  Do not free or modify it. */
GPtrArray *whelk_wallpaper_engine_items(WhelkWallpaperEngine *engine);

/* The resolved policy, owned by the engine.  The picker reads its card size
 * and column count here so a reload cannot leave it with a stale copy. */
const WhelkWallpaperConfig *whelk_wallpaper_engine_config(
    WhelkWallpaperEngine *engine);

/* The file on screen, or NULL when nothing is drawn. */
const char *whelk_wallpaper_engine_current(WhelkWallpaperEngine *engine);

/* TRUE while a background surface is actually up. */
gboolean whelk_wallpaper_engine_is_active(WhelkWallpaperEngine *engine);

G_END_DECLS

#endif // WHELK_WALLPAPER_ENGINE_H

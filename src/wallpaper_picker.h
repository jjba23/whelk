// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_WALLPAPER_PICKER_H
#define WHELK_WALLPAPER_PICKER_H

#include "config.h"
#include "wallpaper.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE WALLPAPER PICKER
 * =========================================================================
 * The background's own overlay: a card in the middle of the output with one
 * thumbnail per wallpaper, opened from a keybinding and dismissed with Esc.
 * The card starts compact and expands to the output, where the same
 * thumbnails fill the screen; the eye chooses, not a file dialog.
 *
 * The picker reads the engine's collection and asks the shell to apply one.
 * It never decodes a wallpaper at full size: a thumbnail is decoded at
 * thumbnail size, and a movie is shown as a film mark rather than a frame.
 */

#define WHELK_TYPE_WALLPAPER_PICKER (whelk_wallpaper_picker_get_type())
G_DECLARE_FINAL_TYPE(WhelkWallpaperPicker, whelk_wallpaper_picker, WHELK,
                     WALLPAPER_PICKER, GObject)

/* @engine is borrowed: the shell owns it and frees the picker first. */
WhelkWallpaperPicker *whelk_wallpaper_picker_new(
    GtkApplication *app, WhelkWallpaperEngine *engine, const char *config_path,
    const WhelkBarConfig *style);

void whelk_wallpaper_picker_set_style(WhelkWallpaperPicker *picker,
                                      const WhelkBarConfig *style);

void whelk_wallpaper_picker_show(WhelkWallpaperPicker *picker);
void whelk_wallpaper_picker_hide(WhelkWallpaperPicker *picker);
void whelk_wallpaper_picker_toggle(WhelkWallpaperPicker *picker);
gboolean whelk_wallpaper_picker_is_visible(WhelkWallpaperPicker *picker);

G_END_DECLS

#endif // WHELK_WALLPAPER_PICKER_H

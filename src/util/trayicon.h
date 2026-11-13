// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_TRAYICON_H
#define WHELK_TRAYICON_H

#include <glib.h>

G_BEGIN_DECLS

/**
 * whelk_tray_icon_file:
 * @theme_path: the directory an item publishes as IconThemePath.
 * @icon_name: the item's IconName.
 *
 * StatusNotifierItem apps may ship their own icon theme beside the running
 * binary and publish its directory as IconThemePath.  GTK does not search that
 * directory by default, so an icon such as JetBrains Toolbox's
 * "toolbox-tray-color" can fall back to a monochrome desktop-theme icon.
 *
 * Returns: (transfer full) (nullable): the app-provided file, preferring PNG
 * over SVG and XPM, or %NULL when the path is relative, the name would escape
 * it, or no such file exists.
 */
char *whelk_tray_icon_file(const char *theme_path, const char *icon_name);

G_END_DECLS

#endif // WHELK_TRAYICON_H

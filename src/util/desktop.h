// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_DESKTOP_H
#define WHELK_DESKTOP_H

#include <gio/gdesktopappinfo.h>
#include <glib.h>

/* =========================================================================
 * THE DESKTOP ENTRY
 * =========================================================================
 * A window's application id is not always the desktop file's name.  Alacritty
 * reports "Alacritty" on sway and its entry is
 * "org.alacritty.Alacritty.desktop"; Firefox reports "firefox", GTK
 * applications report their reverse-DNS id.  The global menu reads a desktop
 * entry to build the File menu -- its actions, and what they are called -- so
 * the lookup tries the two obvious names and then asks the desktop database,
 * keeping only a candidate whose file name actually contains the id.
 */

/**
 * whelk_desktop_for_app_id:
 * @app_id: (nullable): a window's application id or class.
 *
 * Returns: (transfer full) (nullable): the application's desktop entry, or
 *   NULL when the desktop has none.
 */
GDesktopAppInfo *whelk_desktop_for_app_id(const char *app_id);

/**
 * whelk_desktop_short_name:
 * @app_id: (nullable): a window's application id or class.
 * @info: (nullable): its desktop entry, when one was found.
 *
 * The short name a menu bar puts before an application's menus: the desktop
 * entry's name, or -- for an application with no entry -- the last component
 * of its id, with '-' and '_' read as spaces and its first letter capitalised.
 * "com.jetbrains.toolbox" becomes "Toolbox", not the whole reverse-DNS id.
 *
 * Returns: (transfer full): the name; never NULL.
 */
char *whelk_desktop_short_name(const char *app_id, GDesktopAppInfo *info);

#endif // WHELK_DESKTOP_H

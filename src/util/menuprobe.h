// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_MENUPROBE_H
#define WHELK_MENUPROBE_H

#include <glib.h>

#include "util/session.h"

G_BEGIN_DECLS

/* =========================================================================
 * THE ORDER OF THE MENU PROBES
 * =========================================================================
 * The D-Bus side of the global menu is a fallback chain, and the order is the
 * contract: the object paths a GTK window published are tried before the
 * conventional guesses, the X11 registrar is asked only when a window id can
 * answer it, and DBusMenu comes after org.gtk.Menus.
 *
 * Keeping the path construction here, rather than in the async callback maze,
 * is what makes the chain testable.  The module still performs the calls; this
 * only says which ones, in which order, for a focused window.
 */

typedef struct {
  /* org.gtk.Menus object paths: exact X11 paths first, then the conventional
   * paths derived from the application object. */
  GPtrArray *gtk_paths;
  /* com.canonical.dbusmenu paths, with the registrar's exact path inserted at
   * the front when the registrar answers. */
  GPtrArray *dbus_paths;
  /* TRUE when the focused window has an X11 id and may be resolved through
   * com.canonical.AppMenu.Registrar. */
  gboolean ask_registrar;
} WhelkMenuProbeOrder;

/**
 * whelk_menu_probe_order_new:
 * @paths: (nullable): the focused window's X11 properties.
 * @bus_name_is_valid: whether @app_path's application id is a D-Bus name.
 * @app_path: the application object path, used for conventional guesses.
 *
 * Builds the ordered path lists.  A bus name that cannot exist gets no
 * org.gtk.Menus attempts at all; the registrar can still rescue the window.
 *
 * Returns: (transfer full): the order; free with
 *   whelk_menu_probe_order_free().
 */
WhelkMenuProbeOrder *whelk_menu_probe_order_new(
    const WhelkFocusedPaths *paths, gboolean bus_name_is_valid,
    const char *app_path);

/**
 * whelk_menu_probe_use_registrar_path:
 * @dbus_paths: the conventional DBusMenu paths, in place.
 * @path: the exact object path the registrar returned.
 *
 * Puts the registrar's answer at the front of the DBusMenu attempt list,
 * without duplicating a path that was already conventional.  The registrar
 * therefore wins over the guesses, and the guesses remain as fallback when the
 * application moved or the registrar's answer went stale.
 */
void whelk_menu_probe_use_registrar_path(GPtrArray *dbus_paths,
                                         const char *path);

void whelk_menu_probe_order_free(WhelkMenuProbeOrder *order);

G_END_DECLS

#endif // WHELK_MENUPROBE_H

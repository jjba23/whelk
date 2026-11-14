// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NETWORK_PANEL_H
#define WHELK_NETWORK_PANEL_H

#include "config.h"

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE NETWORK PANEL
 * =========================================================================
 * Wi-Fi and Bluetooth, as a panel instead of a tray icon.  The two things a
 * network applet needs are the two things a bar popover cannot do: be wide
 * enough to show a list of access points, and take the keyboard long enough to
 * type a passphrase.  Both are properties of a layer surface, so this is one.
 *
 * The panel speaks NetworkManager and BlueZ through their command-line clients
 * (nmcli, bluetoothctl), which are the interfaces those projects ship as their
 * stable scripting surface: no private D-Bus API, no agent of our own to keep
 * alive, and the same results a terminal would give.
 */

#define WHELK_TYPE_NETWORK_PANEL (whelk_network_panel_get_type())
G_DECLARE_FINAL_TYPE(WhelkNetworkPanel, whelk_network_panel, WHELK,
                     NETWORK_PANEL, GObject)

/**
 * whelk_network_panel_new:
 * @app: the application the surface belongs to.
 * @style: (nullable): the bar whose palette the panel wears.
 *
 * Returns: (transfer full): the panel, presented and hidden.
 */
WhelkNetworkPanel *whelk_network_panel_new(GtkApplication *app,
                                           const WhelkBarConfig *style);

/* Re-skins the open panel after a reload; its selected page and rows stay. */
void whelk_network_panel_set_style(WhelkNetworkPanel *self,
                                   const WhelkBarConfig *style);

/**
 * whelk_network_panel_show:
 * @self: the panel.
 * @page: (nullable): "wifi" or "bluetooth"; NULL leaves the last page up.
 *
 * Shows the panel and re-reads the state of the visible page.
 */
void whelk_network_panel_show(WhelkNetworkPanel *self, const char *page);

void whelk_network_panel_hide(WhelkNetworkPanel *self);

gboolean whelk_network_panel_is_visible(WhelkNetworkPanel *self);

/* TRUE while @self is showing @page ("wifi"/"bluetooth"). */
gboolean whelk_network_panel_showing(WhelkNetworkPanel *self, const char *page);

G_END_DECLS

#endif // WHELK_NETWORK_PANEL_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_APPLETS_H
#define WHELK_APPLETS_H

#include <glib.h>

/* =========================================================================
 * THE APPLETS A BAR ALREADY SPEAKS FOR
 * =========================================================================
 * A bar with its own Wi-Fi and Bluetooth readings does not need an applet that
 * reads them too.  The applets are named by their StatusNotifierItem Id, and
 * only those exact ids are recognised: an item whose *title* happens to say
 * "network" is somebody else's icon and must stay.
 */

/**
 * whelk_applet_is_network:
 * @id: (nullable): a StatusNotifierItem Id.
 *
 * TRUE if @id is one of the known network applets.  Case-insensitive; the
 * match is on the whole id, never a substring.
 */
gboolean whelk_applet_is_network(const char *id);

/**
 * whelk_applet_is_bluetooth:
 * @id: (nullable): a StatusNotifierItem Id.
 *
 * TRUE if @id is one of the known Bluetooth applets.
 */
gboolean whelk_applet_is_bluetooth(const char *id);

#endif // WHELK_APPLETS_H

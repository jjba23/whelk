// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/applets.h"

#include <string.h>

/* NetworkManager's own applets (nm-applet, nm-tray), the stand-alone
 * network-manager-applet, and GNOME's and KDE's network Plasmoids. */
static const char *const network_ids[] = {
    "nm-applet", "nm-tray", "network-manager-applet",
    "org.gnome.networkmanager", "org.kde.networkmanagement", NULL};

/* Blueman's applet and its tray id, Blueberry, and KDE's Bluetooth applets. */
static const char *const bluetooth_ids[] = {"blueman",
                                            "blueman-tray",
                                            "blueberry",
                                            "org.blueman.applet",
                                            "org.kde.bluetooth",
                                            "org.kde.bluedevil",
                                            NULL};

static gboolean id_in(const char *id, const char *const *ids) {
  if (!id)
    return FALSE;

  g_autofree char *needle = g_ascii_strdown(id, -1);

  g_strstrip(needle);

  if (*needle == '\0')
    return FALSE;

  for (guint i = 0; ids[i]; i++) {
    if (strcmp(needle, ids[i]) == 0)
      return TRUE;
  }

  return FALSE;
}

gboolean whelk_applet_is_network(const char *id) {
  return id_in(id, network_ids);
}

gboolean whelk_applet_is_bluetooth(const char *id) {
  return id_in(id, bluetooth_ids);
}

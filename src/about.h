// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_ABOUT_H
#define WHELK_ABOUT_H

#include <gtk/gtk.h>

#include "config.h"

G_BEGIN_DECLS

/* =========================================================================
 * ABOUT THIS SYSTEM
 * =========================================================================
 * The window behind the shell menu's first item: what machine this is, in the
 * terms a person asks about their own computer -- the name, the system, the
 * processor, the memory that is actually there, the disk that is actually
 * free.  A GtkAboutDialog would only say the program's name; this says the
 * machine's, which is what the menu item promises.
 */

/**
 * whelk_about_show:
 * @style: (nullable): the bar whose palette the window wears.
 *
 * Builds and presents the window.  Each call makes a new one; a second call
 * while one is open raises it through the window manager.
 */
void whelk_about_show(const WhelkBarConfig *style);

G_END_DECLS

#endif // WHELK_ABOUT_H

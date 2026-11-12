// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CLICK_H
#define WHELK_CLICK_H

#include "config.h"

#include <gtk/gtk.h>

/* =========================================================================
 * MOUSE ACTIONS
 * =========================================================================
 * Waybar's `on-click`, `on-click-right`, `on-click-middle`, `on-scroll-up` and
 * `on-scroll-down` are the bar's whole input vocabulary, and every module
 * shares them: a module is a reading, and a reading that does nothing when it
 * is clicked is a module with no actions configured -- not a module with no
 * code.
 *
 * The options are read from the module spec, so a new module gets the whole set
 * by calling whelk_click_attach() and nothing else.
 */

/**
 * whelk_click_attach:
 * @widget: the module's root widget.
 * @spec: the module's configuration.
 *
 * Installs one gesture per configured button and a scroll controller, and puts
 * a pointer cursor on @widget when at least one action exists.
 */
void whelk_click_attach(GtkWidget *widget, const WhelkModuleSpec *spec);

#endif // WHELK_CLICK_H

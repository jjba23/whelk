// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_STYLE_H
#define WHELK_STYLE_H

#include "config.h"

#include <gtk/gtk.h>

/* =========================================================================
 * THE GENERATED SKIN
 * =========================================================================
 * The structural stylesheet in resources/css/whelk.css is what a bar, a panel
 * and a launcher have in common.  What they do not have in common -- colours,
 * fonts, radius -- comes from the configuration, and is generated once per
 * surface and scoped to that surface's window id so two bars can wear two
 * materials.
 *
 * Every surface whelk draws uses this: the bar, the launcher, the notification
 * centre and its toasts.  One generator means a palette change is one edit.
 */

/**
 * whelk_style_widget_id:
 * @name: a surface name ("main", "launcher").
 *
 * Returns: (transfer full): the CSS id, "whelk-" plus a CSS-safe @name.
 */
char *whelk_style_widget_id(const char *name);

/**
 * whelk_style_apply:
 * @widget_id: the window's widget name, from whelk_style_widget_id().
 * @config: the bar whose palette the surface wears.
 * @root_class: the CSS class of the surface's material box.
 * @extra_css: (nullable): free-form rules appended verbatim.
 *
 * Returns: (transfer full): the provider, installed on the default display at
 * application priority.  Remove it with whelk_style_detach().
 */
GtkCssProvider *whelk_style_apply(const char *widget_id,
                                  const WhelkBarConfig *config,
                                  const char *root_class,
                                  const char *extra_css);

/* Removes and frees a provider installed by whelk_style_apply(). */
void whelk_style_detach(GtkCssProvider *provider, GdkDisplay *display);

#endif // WHELK_STYLE_H

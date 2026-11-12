// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_SURFACE_H
#define WHELK_SURFACE_H

#include <gtk4-layer-shell.h>
#include <gtk/gtk.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE SURFACE
 * =========================================================================
 * One anchored layer surface: a window the compositor places on an edge, with
 * the anchors, margins and exclusivity a Wayland shell needs.  This is the part
 * of whelk that is not a bar.
 *
 * A bar is a surface with a row of modules in it (src/bar.c).  A notification
 * stack, a launcher, a floating widget or a start menu are surfaces too, with
 * their own child and their own geometry -- which is why the layer-shell
 * plumbing lives here and not inside the bar: the shell's next surface does not
 * have to re-learn 0px, exclusive zones or monitor assignment.
 */

#define WHELK_TYPE_SURFACE (whelk_surface_get_type())
G_DECLARE_FINAL_TYPE(WhelkSurface, whelk_surface, WHELK, SURFACE, GObject)

/**
 * whelk_surface_new:
 * @app: the application the window belongs to.
 * @name: the surface's name; its CSS id becomes "whelk-@name".
 *
 * Returns: (transfer full): a hidden surface.  Nothing is presented until
 * whelk_surface_present().
 */
WhelkSurface *whelk_surface_new(GtkApplication *app, const char *name);

void whelk_surface_set_edge(WhelkSurface *surface, GtkLayerShellEdge edge);

/* The layer the surface is drawn on: TOP for a bar, OVERLAY for a launcher or
 * a notification centre that must sit above everything else. */
void whelk_surface_set_layer(WhelkSurface *surface, GtkLayerShellLayer layer);

/* Whether the surface takes the keyboard.  A bar takes none; a launcher is the
 * one surface in the shell that must take it exclusively. */
void
whelk_surface_set_keyboard_mode(WhelkSurface *surface,
                                GtkLayerShellKeyboardMode mode);
void whelk_surface_set_thickness(WhelkSurface *surface, int thickness);
void whelk_surface_set_margins(WhelkSurface *surface, int top, int right,
                               int bottom, int left);
void whelk_surface_set_exclusive(WhelkSurface *surface, gboolean exclusive);

/* The monitor to pin to, or NULL for the compositor's default. */
void whelk_surface_set_monitor(WhelkSurface *surface, GdkMonitor *monitor);

/* The compositor-visible name of the surface ("whelk-bar", "whelk-launcher"),
 * which a niri window rule can match on. */
void whelk_surface_set_namespace(WhelkSurface *surface,
                                 const char *namespace_name);

/* The widget filling the surface.  The surface takes ownership. */
void whelk_surface_set_child(WhelkSurface *surface, GtkWidget *child);

/*
 * whelk_surface_set_anchors:
 *
 * Anchors the surface explicitly instead of edge-wise, which is what a surface
 * that is not a bar needs: a toast stack hugs its top-right corner, and a
 * launcher covers the output (all four anchors).  Once set, these replace the
 * edge-derived defaults of whelk_surface_present().
 */
void whelk_surface_set_anchors(WhelkSurface *surface, gboolean top,
                               gboolean right, gboolean bottom,
                               gboolean left);

/* gtk_widget_set_name()'s value, and so the CSS id the skin selects on. */
void whelk_surface_set_widget_id(WhelkSurface *surface, const char *widget_id);

GtkWindow *whelk_surface_get_window(WhelkSurface *surface);

/* Configures layer-shell for the window and shows it.  Call it last. */
void whelk_surface_present(WhelkSurface *surface);

/* TRUE while the surface's edge is top or bottom. */
gboolean whelk_surface_is_horizontal(WhelkSurface *surface);

G_END_DECLS

#endif // WHELK_SURFACE_H

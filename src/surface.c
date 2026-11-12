// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "surface.h"

#include "config.h"
#include "logger.h"

struct _WhelkSurface {
  GObject parent_instance;

  GtkWindow *window;
  GtkWidget *child; /* owned through the window */

  char *name;
  char *namespace_name;
  GtkLayerShellEdge edge;
  GtkLayerShellLayer layer;
  GtkLayerShellKeyboardMode keyboard_mode;
  int thickness;
  int margin_top;
  int margin_right;
  int margin_bottom;
  int margin_left;
  gboolean exclusive;
  gboolean explicit_anchors;
  gboolean anchor_top;
  gboolean anchor_right;
  gboolean anchor_bottom;
  gboolean anchor_left;
  GdkMonitor *monitor; /* borrowed from the display's model */
};

G_DEFINE_TYPE(WhelkSurface, whelk_surface, G_TYPE_OBJECT)

static void whelk_surface_dispose(GObject *object) {
  WhelkSurface *self = WHELK_SURFACE(object);

  LOG_C_DEBUG("Surface", "disposing '%s' (window %p, monitor %p)", self->name,
              (void *)self->window, (void *)self->monitor);

  if (self->window) {
    /* Hidden before it is destroyed: the compositor withdraws the layer
     * surface first, which is both the polite order and the one GTK is
     * happiest with when the application keeps running (a reload). */
    gtk_widget_set_visible(GTK_WIDGET(self->window), FALSE);
    gtk_window_destroy(self->window);
    g_clear_object(&self->window);
  }

  self->child = NULL;
  g_clear_object(&self->monitor);

  G_OBJECT_CLASS(whelk_surface_parent_class)->dispose(object);
}

static void whelk_surface_finalize(GObject *object) {
  WhelkSurface *self = WHELK_SURFACE(object);

  g_free(self->name);
  g_free(self->namespace_name);

  G_OBJECT_CLASS(whelk_surface_parent_class)->finalize(object);
}

static void whelk_surface_class_init(WhelkSurfaceClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_surface_dispose;
  object_class->finalize = whelk_surface_finalize;
}

static void whelk_surface_init(WhelkSurface *self) {
  self->edge = GTK_LAYER_SHELL_EDGE_TOP;
  self->layer = GTK_LAYER_SHELL_LAYER_TOP;
  self->keyboard_mode = GTK_LAYER_SHELL_KEYBOARD_MODE_NONE;
  self->thickness = 32;
  self->exclusive = TRUE;
  self->namespace_name = g_strdup("whelk");
}

WhelkSurface *whelk_surface_new(GtkApplication *app, const char *name) {
  WhelkSurface *self = g_object_new(WHELK_TYPE_SURFACE, NULL);

  self->name = g_strdup(name ? name : "surface");

  /* GtkWidget is GInitiallyUnowned: the constructor hands back a floating
   * reference, which GTK sinks when the window joins the application.  The
   * surface takes a real reference of its own, so destroying the window and
   * then dropping it cannot free it twice. */
  self->window =
      GTK_WINDOW(g_object_ref_sink(gtk_application_window_new(app)));

  gtk_widget_add_css_class(GTK_WIDGET(self->window), "whelk-surface");
  gtk_window_set_decorated(self->window, FALSE);
  /* A layer surface is sized by the compositor: an exclusive zone from another
   * bar, a scale change or a margin all shrink or grow it without our asking.
   * A non-resizable window ignores those configure events and keeps laying out
   * at its first size, which is how a vertical bar's last group ends up below
   * the surface's bottom edge, painted nowhere. */
  gtk_window_set_resizable(self->window, TRUE);
  gtk_window_set_title(self->window, self->name);

  return self;
}

void whelk_surface_set_edge(WhelkSurface *surface, GtkLayerShellEdge edge) {
  if (!surface)
    return;

  surface->edge = edge;
}

void whelk_surface_set_layer(WhelkSurface *surface, GtkLayerShellLayer layer) {
  if (!surface)
    return;

  surface->layer = layer;
}

void whelk_surface_set_keyboard_mode(
    WhelkSurface *surface, GtkLayerShellKeyboardMode mode) {
  if (!surface)
    return;

  surface->keyboard_mode = mode;
}

gboolean whelk_surface_is_horizontal(WhelkSurface *surface) {
  if (!surface)
    return TRUE;

  return surface->edge == GTK_LAYER_SHELL_EDGE_TOP ||
         surface->edge == GTK_LAYER_SHELL_EDGE_BOTTOM;
}

void whelk_surface_set_thickness(WhelkSurface *surface, int thickness) {
  if (!surface)
    return;

  surface->thickness = thickness;
}

void whelk_surface_set_margins(WhelkSurface *surface, int top, int right,
                               int bottom, int left) {
  if (!surface)
    return;

  surface->margin_top = top;
  surface->margin_right = right;
  surface->margin_bottom = bottom;
  surface->margin_left = left;
}

void whelk_surface_set_exclusive(WhelkSurface *surface, gboolean exclusive) {
  if (!surface)
    return;

  surface->exclusive = exclusive;
}

void whelk_surface_set_monitor(WhelkSurface *surface, GdkMonitor *monitor) {
  if (!surface)
    return;

  g_set_object(&surface->monitor, monitor);
}

void whelk_surface_set_namespace(WhelkSurface *surface,
                                 const char *namespace_name) {
  if (!surface)
    return;

  g_free(surface->namespace_name);
  surface->namespace_name = g_strdup(namespace_name ? namespace_name : "whelk");
}

void whelk_surface_set_anchors(WhelkSurface *surface, gboolean top,
                               gboolean right, gboolean bottom,
                               gboolean left) {
  if (!surface)
    return;

  surface->explicit_anchors = TRUE;
  surface->anchor_top = top;
  surface->anchor_right = right;
  surface->anchor_bottom = bottom;
  surface->anchor_left = left;
}

void whelk_surface_set_child(WhelkSurface *surface, GtkWidget *child) {
  if (!surface || !surface->window)
    return;

  gtk_window_set_child(surface->window, child);
  surface->child = child;
}

void whelk_surface_set_widget_id(WhelkSurface *surface, const char *widget_id) {
  if (!surface || !surface->window)
    return;

  gtk_widget_set_name(GTK_WIDGET(surface->window), widget_id);
}

GtkWindow *whelk_surface_get_window(WhelkSurface *surface) {
  return surface ? surface->window : NULL;
}

/* The size a surface should have.
 *
 * A GTK window does not take its size from the compositor's configure: a layer
 * client asks for one.  Without this, an anchored surface is only as large as
 * its content -- a right-hand panel as tall as its first two rows, a bar as
 * wide as its modules -- while the compositor happily anchors that small
 * window.  So the size is computed from the monitor and the anchors: the bar
 * spans its edge's width, a panel spans the screen's height, and a surface
 * anchored to both sides of an axis takes that axis. */
static void apply_default_size(WhelkSurface *surface) {
  GdkDisplay *display = gdk_display_get_default();
  GdkMonitor *monitor = surface->monitor;
  g_autoptr(GdkMonitor) fallback = NULL;

  if (!monitor && display)
    monitor = fallback = g_list_model_get_item(
                  G_LIST_MODEL(gdk_display_get_monitors(display)), 0);

  if (!monitor)
    return;

  GdkRectangle geometry = {0};

  gdk_monitor_get_geometry(monitor, &geometry);

  const gboolean horizontal = whelk_surface_is_horizontal(surface);
  int width = -1;
  int height = -1;

  if (surface->explicit_anchors) {
    if (surface->anchor_left && surface->anchor_right)
      width = geometry.width - surface->margin_left - surface->margin_right;

    if (surface->anchor_top && surface->anchor_bottom)
      height = geometry.height - surface->margin_top - surface->margin_bottom;
  } else if (horizontal) {
    width = geometry.width - surface->margin_left - surface->margin_right;
    height = surface->thickness;
  } else {
    width = surface->thickness;
    height = geometry.height - surface->margin_top - surface->margin_bottom;
  }

  gtk_window_set_default_size(surface->window, width, height);
}

void whelk_surface_present(WhelkSurface *surface) {
  if (!surface || !surface->window)
    return;

  GtkWindow *window = surface->window;
  const gboolean horizontal = whelk_surface_is_horizontal(surface);

  gtk_layer_init_for_window(window);
  gtk_layer_set_namespace(window, surface->namespace_name);
  gtk_layer_set_layer(window, surface->layer);

  /* A bar takes no keyboard: it is read and clicked, and grabbing focus from
   * the window you are working in is the one thing a bar must never do.  A
   * launcher is the opposite, and says so when it is built. */
  gtk_layer_set_keyboard_mode(window, surface->keyboard_mode);

  /* A bar spans its edge: the two corners of that edge plus the edge itself.
   * A top bar is top+left+right, a left bar is left+top+bottom, and so on for
   * the four edges -- anchoring all four would make the surface fill the
   * output instead of hugging its edge. */
  if (surface->explicit_anchors) {
    /* A toast stack or a launcher: the caller knows which corners it wants,
     * and the surface is exactly as large as its child says. */
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_TOP,
                         surface->anchor_top);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_BOTTOM,
                         surface->anchor_bottom);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_LEFT,
                         surface->anchor_left);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_RIGHT,
                         surface->anchor_right);
  } else {
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_TOP, FALSE);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_BOTTOM, FALSE);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_LEFT, FALSE);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_RIGHT, FALSE);
    gtk_layer_set_anchor(window, surface->edge, TRUE);

    if (horizontal) {
      gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_LEFT, TRUE);
      gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_RIGHT, TRUE);
    } else {
      gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_TOP, TRUE);
      gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_BOTTOM, TRUE);
    }
  }

  gtk_layer_set_margin(window, GTK_LAYER_SHELL_EDGE_TOP, surface->margin_top);
  gtk_layer_set_margin(window, GTK_LAYER_SHELL_EDGE_RIGHT,
                       surface->margin_right);
  gtk_layer_set_margin(window, GTK_LAYER_SHELL_EDGE_BOTTOM,
                       surface->margin_bottom);
  gtk_layer_set_margin(window, GTK_LAYER_SHELL_EDGE_LEFT, surface->margin_left);

  if (surface->monitor)
    gtk_layer_set_monitor(window, surface->monitor);

  /* The exclusive zone is the bar's thickness: the compositor reserves exactly
   * that much, which is what stops a maximised window from hiding under it. */
  gtk_layer_set_exclusive_zone(window,
                               surface->exclusive ? surface->thickness : 0);

  LOG_C_DEBUG("Surface", "presenting '%s' on the %s edge", surface->name,
              whelk_edge_name(surface->edge));

  apply_default_size(surface);
  gtk_window_present(window);
}

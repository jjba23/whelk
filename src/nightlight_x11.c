// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "nightlight_backend.h"

#include <glib-unix.h>
#include <math.h>
#include <string.h>

#include "logger.h"
#include "util/nightlight.h"

#ifdef HAVE_XRANDR
#include <X11/Xlib.h>
#include <X11/extensions/Xrandr.h>
#endif

/* =========================================================================
 * X11: THE RANDR GAMMA TABLE
 * =========================================================================
 * On an X server the gamma table belongs to a CRTC, and RandR 1.2 is the
 * extension that exposes it: XRRGetCrtcGammaSize says how large it is and
 * XRRSetCrtcGamma writes it.  This is the same table a colour calibrator uses,
 * so the compositor, video players and colour-managed applications all see
 * one calibrated screen rather than a warm sheet of glass over it.
 *
 * RandR tells us when the screen changes (a mode switch, a monitor plugged in,
 * a CRTC moved), and the table is written again after each.  Without that, a
 * laptop closing its lid would come back with the gamma reset to identity.
 */

#ifdef HAVE_XRANDR

typedef struct {
  RRCrtc crtc;
  int size;
  guint16 *red;
  guint16 *green;
  guint16 *blue;
} WhelkOriginalGamma;

typedef struct {
  Display *display;
  Window root;
  int event_base;
  int randr_minor;
  guint fd_source;
  guint reapply_id;
  double rgb[3];
  /* What each CRTC held before whelk first wrote it.  Wayland restores the
   * compositor's table when the gamma control is destroyed; RandR has no such
   * contract, so the table has to be read and put back by hand.  Without this,
   * turning the night light off would replace a colour profile loaded by a
   * calibrator with a bare identity ramp. */
  GList *originals;
} WhelkX11NightLight;

static void original_free(WhelkOriginalGamma *original) {
  if (!original)
    return;

  g_free(original->red);
  g_free(original->green);
  g_free(original->blue);
  g_free(original);
}

static WhelkOriginalGamma *original_for(WhelkX11NightLight *backend,
                                        RRCrtc crtc) {
  for (GList *item = backend->originals; item; item = item->next) {
    WhelkOriginalGamma *original = item->data;

    if (original->crtc == crtc)
      return original;
  }

  return NULL;
}

static void capture_original(WhelkX11NightLight *backend, RRCrtc crtc,
                             int size) {
  XRRCrtcGamma *gamma = XRRGetCrtcGamma(backend->display, crtc);

  if (!gamma || gamma->size != size) {
    if (gamma)
      XRRFreeGamma(gamma);
    return;
  }

  WhelkOriginalGamma *original = g_new0(WhelkOriginalGamma, 1);

  original->crtc = crtc;
  original->size = size;
  original->red = g_memdup2(gamma->red, (gsize)size * sizeof(guint16));
  original->green = g_memdup2(gamma->green, (gsize)size * sizeof(guint16));
  original->blue = g_memdup2(gamma->blue, (gsize)size * sizeof(guint16));
  backend->originals = g_list_append(backend->originals, original);

  XRRFreeGamma(gamma);
}

static gboolean neutral(const double rgb[3]) {
  return fabs(rgb[0] - 1.0) < 0.000001 && fabs(rgb[1] - 1.0) < 0.000001 &&
         fabs(rgb[2] - 1.0) < 0.000001;
}

static void copy_original(const WhelkOriginalGamma *original,
                          XRRCrtcGamma *gamma) {
  memcpy(gamma->red, original->red,
         (gsize)original->size * sizeof(guint16));
  memcpy(gamma->green, original->green,
         (gsize)original->size * sizeof(guint16));
  memcpy(gamma->blue, original->blue,
         (gsize)original->size * sizeof(guint16));
}

static XRRScreenResources *screen_resources(WhelkX11NightLight *backend) {
  /* The Current request arrived in RandR 1.3 and is the cheap one; a 1.2
   * server only knows the full request. */
  return backend->randr_minor >= 3
             ? XRRGetScreenResourcesCurrent(backend->display, backend->root)
             : XRRGetScreenResources(backend->display, backend->root);
}

/* Puts every CRTC whelk touched back to the table it had before. */
static void restore_originals(WhelkX11NightLight *backend) {
  if (!backend->display || !backend->originals)
    return;

  XRRScreenResources *resources = screen_resources(backend);

  if (!resources)
    return;

  for (GList *item = backend->originals; item; item = item->next) {
    WhelkOriginalGamma *original = item->data;
    gboolean present = FALSE;

    /* A monitor that was unplugged took its CRTC with it; asking the server
     * to restore a CRTC that no longer exists is an X error, not a restore. */
    for (int i = 0; i < resources->ncrtc; i++) {
      if (resources->crtcs[i] == original->crtc) {
        present = TRUE;
        break;
      }
    }

    if (!present)
      continue;

    XRRCrtcGamma *gamma = XRRAllocGamma(original->size);

    if (!gamma)
      continue;

    copy_original(original, gamma);
    XRRSetCrtcGamma(backend->display, original->crtc, gamma);
    XRRFreeGamma(gamma);
  }

  XRRFreeScreenResources(resources);
  XFlush(backend->display);
}

static gboolean apply_all(WhelkX11NightLight *backend) {
  XRRScreenResources *resources = screen_resources(backend);

  if (!resources)
    return FALSE;

  for (int i = 0; i < resources->ncrtc; i++) {
    const int size =
        XRRGetCrtcGammaSize(backend->display, resources->crtcs[i]);

    /* A CRTC with no gamma table is a real answer, not an error: some virtual
     * and remote outputs have none.  Leave it alone. */
    if (size <= 0)
      continue;

    WhelkOriginalGamma *original = original_for(backend, resources->crtcs[i]);

    if (original && original->size != size) {
      backend->originals = g_list_remove(backend->originals, original);
      original_free(original);
      original = NULL;
    }

    if (!original) {
      capture_original(backend, resources->crtcs[i], size);
      original = original_for(backend, resources->crtcs[i]);
    }

    XRRCrtcGamma *gamma = XRRAllocGamma(size);

    if (!gamma)
      continue;

    if (neutral(backend->rgb) && original) {
      copy_original(original, gamma);
    } else {
      whelk_night_light_fill_ramp((guint)size, backend->rgb, gamma->red,
                                  gamma->green, gamma->blue);
    }

    XRRSetCrtcGamma(backend->display, resources->crtcs[i], gamma);
    XRRFreeGamma(gamma);
  }

  XRRFreeScreenResources(resources);
  XFlush(backend->display);

  return TRUE;
}

static gboolean on_reapply(gpointer data) {
  WhelkX11NightLight *backend = data;

  backend->reapply_id = 0;
  apply_all(backend);

  return G_SOURCE_REMOVE;
}

static void schedule_reapply(WhelkX11NightLight *backend) {
  if (backend->reapply_id == 0)
    backend->reapply_id = g_idle_add(on_reapply, backend);
}

static gboolean on_x11_event(gint fd, GIOCondition condition, gpointer data) {
  WhelkX11NightLight *backend = data;
  gboolean screen_changed = FALSE;

  (void)fd;
  (void)condition;

  while (XPending(backend->display) > 0) {
    XEvent event;

    XNextEvent(backend->display, &event);

    if (event.type == backend->event_base + RRScreenChangeNotify) {
      XRRUpdateConfiguration(&event);
      screen_changed = TRUE;
    } else if (event.type == backend->event_base + RRNotify) {
      screen_changed = TRUE;
    }
  }

  if (screen_changed)
    schedule_reapply(backend);

  return G_SOURCE_CONTINUE;
}

static gboolean set_color(gpointer data, const double rgb[3]) {
  WhelkX11NightLight *backend = data;

  memcpy(backend->rgb, rgb, sizeof(backend->rgb));
  return apply_all(backend);
}

static void destroy(gpointer data) {
  WhelkX11NightLight *backend = data;

  if (!backend)
    return;

  if (backend->fd_source != 0)
    g_source_remove(backend->fd_source);

  if (backend->reapply_id != 0)
    g_source_remove(backend->reapply_id);

  /* RandR has no restore-on-destroy contract, so a shutdown while the screen
   * is still warm owes the user their old calibration. */
  restore_originals(backend);

  g_list_free_full(backend->originals,
                   (GDestroyNotify)original_free);

  if (backend->display)
    XCloseDisplay(backend->display);

  g_free(backend);
}

WhelkNightLightBackend *whelk_night_light_x11_backend_new(void) {
  const char *display_name = g_getenv("DISPLAY");

  if (!display_name || *display_name == '\0')
    return NULL;

  WhelkX11NightLight *backend = g_new0(WhelkX11NightLight, 1);

  backend->display = XOpenDisplay(NULL);

  if (!backend->display) {
    g_free(backend);
    return NULL;
  }

  int error_base = 0;

  if (!XRRQueryExtension(backend->display, &backend->event_base, &error_base)) {
    XCloseDisplay(backend->display);
    g_free(backend);
    return NULL;
  }

  int major = 1;
  int minor = 6;

  if (!XRRQueryVersion(backend->display, &major, &minor) || major < 1 ||
      (major == 1 && minor < 2)) {
    XCloseDisplay(backend->display);
    g_free(backend);
    return NULL;
  }

  backend->root = DefaultRootWindow(backend->display);
  backend->randr_minor = minor;
  backend->rgb[0] = 1.0;
  backend->rgb[1] = 1.0;
  backend->rgb[2] = 1.0;

  XRRSelectInput(backend->display, backend->root,
                 RRScreenChangeNotifyMask | RRCrtcChangeNotifyMask |
                     RROutputChangeNotifyMask);

  backend->fd_source = g_unix_fd_add(
      ConnectionNumber(backend->display), G_IO_IN | G_IO_ERR | G_IO_HUP,
      on_x11_event, backend);

  WhelkNightLightBackend *result = g_new0(WhelkNightLightBackend, 1);

  result->name = "randr";
  result->set_color = set_color;
  result->destroy = destroy;
  result->data = backend;

  return result;
}

#else

WhelkNightLightBackend *whelk_night_light_x11_backend_new(void) {
  /* A build without libXrandr has no way to set an X server's gamma table.
   * Saying so is better than pretending the setting worked. */
  return NULL;
}

#endif

void whelk_night_light_backend_free(WhelkNightLightBackend *backend) {
  if (!backend)
    return;

  if (backend->destroy)
    backend->destroy(backend->data);

  g_free(backend);
}

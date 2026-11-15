// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The X11 backend against a real server.  The pure night-light tests prove the
// ramp arithmetic; this proves the other half: that the RandR CRTC tables are
// actually written, that a warm white point reaches every CRTC, and that the
// table each CRTC held before is put back both when the controller goes neutral
// and when the backend is destroyed.
//
// It needs a server with RandR gamma tables.  The test exits 77 (Meson's skip
// status) on a server without them, such as XWayland; Meson runs it through
// tests/nightlight-x11-test.sh, which starts a private Xvfb when xvfb-run is
// available.

#include "nightlight_backend.h"
#include "util/nightlight.h"

#include <X11/Xlib.h>
#include <X11/extensions/Xrandr.h>
#include <glib.h>
#include <math.h>
#include <string.h>

typedef struct {
  RRCrtc crtc;
  int size;
  guint16 *red;
  guint16 *green;
  guint16 *blue;
} SavedGamma;

typedef struct {
  Display *display;
  Window root;
  int randr_major;
  int randr_minor;
  /* The table each CRTC had before this test touched it.  It is put back when
   * the test finishes, so running the binary against a display someone else
   * owns does not leave a calibration ramp behind. */
  GPtrArray *ambient;
  /* The distinctive calibration table installed for the test, and the table
   * the backend must restore. */
  GPtrArray *saved;
} Fixture;

static void saved_gamma_free(gpointer data) {
  SavedGamma *gamma = data;

  if (!gamma)
    return;

  g_free(gamma->red);
  g_free(gamma->green);
  g_free(gamma->blue);
  g_free(gamma);
}

G_DEFINE_AUTOPTR_CLEANUP_FUNC(SavedGamma, saved_gamma_free)

static SavedGamma *gamma_capture(Display *display, RRCrtc crtc, int size) {
  XRRCrtcGamma *gamma = XRRGetCrtcGamma(display, crtc);

  if (!gamma)
    return NULL;

  if (gamma->size != size) {
    XRRFreeGamma(gamma);
    return NULL;
  }

  SavedGamma *saved = g_new0(SavedGamma, 1);

  saved->crtc = crtc;
  saved->size = size;
  saved->red = g_memdup2(gamma->red, (gsize)size * sizeof(guint16));
  saved->green = g_memdup2(gamma->green, (gsize)size * sizeof(guint16));
  saved->blue = g_memdup2(gamma->blue, (gsize)size * sizeof(guint16));

  XRRFreeGamma(gamma);
  return saved;
}

/* A calibration table no night light would produce: each channel has its own
 * slope and the blue channel starts above zero.  A backend that writes a
 * neutral identity table instead of restoring this cannot pass by accident. */
static void install_calibration(Display *display, RRCrtc crtc, int size) {
  XRRCrtcGamma *gamma = XRRAllocGamma(size);

  g_assert_nonnull(gamma);

  for (int i = 0; i < size; i++) {
    const double level = size > 1 ? (double)i / (double)(size - 1) : 1.0;

    gamma->red[i] = (guint16)lround(level * 0.60 * 65535.0);
    gamma->green[i] = (guint16)lround(level * 0.80 * 65535.0);
    gamma->blue[i] = (guint16)lround((0.15 + 0.25 * level) * 65535.0);
  }

  XRRSetCrtcGamma(display, crtc, gamma);
  XRRFreeGamma(gamma);
}

static void wait_for_server(Display *display) {
  /* The backend has its own connection and flushes after every write.  Give
   * that request time to arrive before asking this connection to read. */
  g_usleep(100 * 1000);
  XSync(display, False);
}

static void assert_restored(Fixture *fixture, const char *when) {
  for (guint i = 0; i < fixture->saved->len; i++) {
    const SavedGamma *expected = g_ptr_array_index(fixture->saved, i);
    g_autoptr(SavedGamma) actual =
        gamma_capture(fixture->display, expected->crtc, expected->size);

    g_assert_nonnull(actual);

    for (int entry = 0; entry < expected->size; entry++) {
      if (actual->red[entry] != expected->red[entry] ||
          actual->green[entry] != expected->green[entry] ||
          actual->blue[entry] != expected->blue[entry]) {
        g_error("CRTC %lu entry %d not restored %s: got (%u, %u, %u), "
                "expected (%u, %u, %u)",
                (unsigned long)expected->crtc, entry, when,
                (guint)actual->red[entry], (guint)actual->green[entry],
                (guint)actual->blue[entry], (guint)expected->red[entry],
                (guint)expected->green[entry], (guint)expected->blue[entry]);
      }
    }
  }
}

static void assert_warm(Fixture *fixture, const double warm[3]) {
  for (guint i = 0; i < fixture->saved->len; i++) {
    const SavedGamma *original = g_ptr_array_index(fixture->saved, i);
    g_autoptr(SavedGamma) actual =
        gamma_capture(fixture->display, original->crtc, original->size);

    g_assert_nonnull(actual);

    g_autofree guint16 *red = g_new(guint16, original->size);
    g_autofree guint16 *green = g_new(guint16, original->size);
    g_autofree guint16 *blue = g_new(guint16, original->size);

    whelk_night_light_fill_ramp((guint)original->size, warm, red, green, blue);

    for (int entry = 0; entry < original->size; entry++) {
      if (actual->red[entry] != red[entry] ||
          actual->green[entry] != green[entry] ||
          actual->blue[entry] != blue[entry]) {
        g_error("CRTC %lu entry %d is not the warm ramp: got (%u, %u, %u), "
                "expected (%u, %u, %u)",
                (unsigned long)original->crtc, entry,
                (guint)actual->red[entry], (guint)actual->green[entry],
                (guint)actual->blue[entry], (guint)red[entry],
                (guint)green[entry], (guint)blue[entry]);
      }
    }

    const int mid = original->size / 2;

    g_assert_cmpuint(actual->red[mid], >, actual->green[mid]);
    g_assert_cmpuint(actual->green[mid], >, actual->blue[mid]);
    g_assert_cmpuint(actual->red[original->size - 1], ==, 65535);
    g_assert_cmpuint(actual->blue[original->size - 1], ==,
                     (guint16)lround(warm[2] * 65535.0));
    g_assert_true(actual->red[mid] != original->red[mid] ||
                  actual->green[mid] != original->green[mid] ||
                  actual->blue[mid] != original->blue[mid]);

    g_test_message(
        "CRTC %lu (%d entries): warm midpoint RGB %u/%u/%u, "
        "blue maximum %u, calibration midpoint RGB %u/%u/%u",
        (unsigned long)original->crtc, original->size,
        (guint)actual->red[mid], (guint)actual->green[mid],
        (guint)actual->blue[mid], (guint)actual->blue[original->size - 1],
        (guint)original->red[mid], (guint)original->green[mid],
        (guint)original->blue[mid]);
  }
}

static void free_fixture(Fixture *fixture) {
  if (fixture->display && fixture->ambient) {
    for (guint i = 0; i < fixture->ambient->len; i++) {
      const SavedGamma *original = g_ptr_array_index(fixture->ambient, i);
      XRRCrtcGamma *gamma = XRRAllocGamma(original->size);

      if (!gamma)
        continue;

      memcpy(gamma->red, original->red,
             (gsize)original->size * sizeof(guint16));
      memcpy(gamma->green, original->green,
             (gsize)original->size * sizeof(guint16));
      memcpy(gamma->blue, original->blue,
             (gsize)original->size * sizeof(guint16));
      XRRSetCrtcGamma(fixture->display, original->crtc, gamma);
      XRRFreeGamma(gamma);
    }

    XSync(fixture->display, False);
  }

  if (fixture->ambient) {
    g_ptr_array_unref(fixture->ambient);
    fixture->ambient = NULL;
  }

  if (fixture->saved) {
    g_ptr_array_unref(fixture->saved);
    fixture->saved = NULL;
  }

  if (fixture->display) {
    XCloseDisplay(fixture->display);
    fixture->display = NULL;
  }
}

/* Returns NULL on success, or a reason to skip. */
static const char *setup_fixture(Fixture *fixture) {
  const char *name = g_getenv("DISPLAY");

  if (!name || *name == '\0')
    return "DISPLAY is not set";

  fixture->display = XOpenDisplay(NULL);

  if (!fixture->display)
    return "the X server named by DISPLAY could not be opened";

  int event_base = 0;
  int error_base = 0;

  if (!XRRQueryExtension(fixture->display, &event_base, &error_base))
    return "the X server has no RandR extension";

  if (!XRRQueryVersion(fixture->display, &fixture->randr_major,
                       &fixture->randr_minor) ||
      fixture->randr_major < 1 ||
      (fixture->randr_major == 1 && fixture->randr_minor < 2))
    return "the X server has no RandR 1.2";

  fixture->root = DefaultRootWindow(fixture->display);

  XRRScreenResources *resources =
      fixture->randr_minor >= 3
          ? XRRGetScreenResourcesCurrent(fixture->display, fixture->root)
          : XRRGetScreenResources(fixture->display, fixture->root);

  if (!resources)
    return "the X server returned no screen resources";

  GArray *crtcs = g_array_new(FALSE, FALSE, sizeof(RRCrtc));

  for (int i = 0; i < resources->ncrtc; i++) {
    if (XRRGetCrtcGammaSize(fixture->display, resources->crtcs[i]) > 0)
      g_array_append_val(crtcs, resources->crtcs[i]);
  }

  XRRFreeScreenResources(resources);

  if (crtcs->len == 0) {
    g_array_unref(crtcs);
    return "the X server exposes no CRTC with a gamma table";
  }

  fixture->ambient = g_ptr_array_new_with_free_func(saved_gamma_free);
  fixture->saved = g_ptr_array_new_with_free_func(saved_gamma_free);

  for (guint i = 0; i < crtcs->len; i++) {
    const RRCrtc crtc = g_array_index(crtcs, RRCrtc, i);
    const int size = XRRGetCrtcGammaSize(fixture->display, crtc);
    SavedGamma *ambient = gamma_capture(fixture->display, crtc, size);

    if (!ambient) {
      g_array_unref(crtcs);
      return "the X server would not return a CRTC gamma table";
    }

    g_ptr_array_add(fixture->ambient, ambient);

    install_calibration(fixture->display, crtc, size);
    XSync(fixture->display, False);

    SavedGamma *saved = gamma_capture(fixture->display, crtc, size);

    if (!saved) {
      g_array_unref(crtcs);
      return "the X server would not return the installed gamma table";
    }

    g_ptr_array_add(fixture->saved, saved);
  }

  g_array_unref(crtcs);
  return NULL;
}

static void test_warm_and_restore(gconstpointer data) {
  Fixture *fixture = (Fixture *)data;
  double warm[3];
  const double neutral[3] = {1.0, 1.0, 1.0};

  whelk_night_light_color(3400, warm);
  g_assert_cmpfloat(warm[0], >=, warm[1]);
  g_assert_cmpfloat(warm[1], >, warm[2]);

  g_test_message("server %s: RandR %d.%d, %u CRTC(s) with gamma tables",
                 DisplayString(fixture->display), fixture->randr_major,
                 fixture->randr_minor, fixture->saved->len);

  WhelkNightLightBackend *backend = whelk_night_light_x11_backend_new();

  g_assert_nonnull(backend);
  g_assert_cmpstr(backend->name, ==, "randr");

  g_assert_true(backend->set_color(backend->data, warm));
  wait_for_server(fixture->display);
  assert_warm(fixture, warm);

  /* Neutral is the controller's "off": it must restore, not write identity. */
  g_assert_true(backend->set_color(backend->data, neutral));
  wait_for_server(fixture->display);
  assert_restored(fixture, "after going neutral");

  /* A shutdown can free the backend without a neutral write first.  The
   * backend's own destroy must still leave the calibration in place. */
  g_assert_true(backend->set_color(backend->data, warm));
  wait_for_server(fixture->display);
  assert_warm(fixture, warm);

  whelk_night_light_backend_free(backend);
  wait_for_server(fixture->display);
  assert_restored(fixture, "after backend destroy");

  g_print("X11 night light: %u CRTC(s), warm 3400 K applied, original "
          "tables restored after neutral and after destroy\n",
          fixture->saved->len);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  Fixture fixture = {0};
  const char *skip = setup_fixture(&fixture);

  if (skip) {
    g_printerr("SKIP: %s\n", skip);
    free_fixture(&fixture);
    return 77;
  }

  g_test_add_data_func("/nightlight/x11/warm-and-restore", &fixture,
                       test_warm_and_restore);

  const int result = g_test_run();

  free_fixture(&fixture);
  return result;
}

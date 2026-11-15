// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "nightlight_backend.h"

#include <errno.h>
#include <glib-unix.h>
#include <glib/gstdio.h>
#include <math.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>

#include "logger.h"
#include "util/nightlight.h"
#include "wlr-gamma-control-client-protocol.h"

/* =========================================================================
 * WAYLAND: wlr-gamma-control-unstable-v1
 * =========================================================================
 * The protocol is the native Wayland answer to a night light: the compositor
 * owns the CRTC, so it is the only process that can write the gamma table.  A
 * client binds the manager, creates one control object per wl_output, and
 * sends a file descriptor holding three 16-bit ramps.  The compositor restores
 * the original table when the control object is destroyed.
 *
 * whelk uses its own wl_display connection rather than GDK's.  The protocol
 * does not depend on the shell's surfaces, and a separate connection keeps a
 * gamma failure from touching the bar's event queue.  The connection's fd is
 * folded into the GLib main loop, so hotplug and gamma_size events arrive the
 * same way every other event in the shell does.
 *
 * Second connection, same client: the compositor permits it, and the control
 * object is per output, not per seat or per surface.
 */

#define GAMMA_RETRY_US (30 * G_TIME_SPAN_SECOND)

typedef struct _WhelkWaylandNightLight WhelkWaylandNightLight;
typedef struct _WhelkWaylandOutput WhelkWaylandOutput;

struct _WhelkWaylandOutput {
  WhelkWaylandNightLight *backend;
  struct wl_output *output;
  struct zwlr_gamma_control_v1 *control;
  guint32 global_name;
  guint32 ramp_size;
  gboolean failed;
  gboolean warned;
  gboolean reported;
  gint64 retry_at;
};

struct _WhelkWaylandNightLight {
  struct wl_display *display;
  struct wl_registry *registry;
  struct zwlr_gamma_control_manager_v1 *manager;
  guint32 manager_name;
  GList *outputs;
  guint fd_source;
  gboolean alive;
  /* The controller asks for a colour only when the filter is actually on.
   * Until then this backend must not create a gamma control: the control is
   * exclusive, and taking it just to write an identity table would steal the
   * output from a calibrator or another night light. */
  gboolean has_color;
  double rgb[3];
};

static void apply_output(WhelkWaylandOutput *output);
static const struct zwlr_gamma_control_v1_listener gamma_listener;

/* =========================================================================
 * THE GAMMA CONTROL OBJECT
 * ========================================================================= */

static void ensure_control(WhelkWaylandOutput *output) {
  WhelkWaylandNightLight *backend = output->backend;

  if (output->control || !backend->manager || !backend->alive)
    return;

  /* A failed control is usually another client holding the output.  Retrying
   * on every fade step would be a request storm; thirty seconds is enough for
   * the other client to go away, and the next output event retries sooner. */
  if (output->failed && g_get_monotonic_time() < output->retry_at)
    return;

  output->failed = FALSE;
  output->control = zwlr_gamma_control_manager_v1_get_gamma_control(
      backend->manager, output->output);
  output->ramp_size = 0;
  zwlr_gamma_control_v1_add_listener(output->control, &gamma_listener, output);
}

static int create_table_fd(gsize bytes) {
  int fd = memfd_create("whelk-gamma", MFD_CLOEXEC);

  if (fd < 0) {
    /* memfd_create is Linux, and whelk is Linux; a hardened or very old
     * runtime can still refuse it.  An unlinked temporary file is the same
     * thing to the compositor and leaves no name behind. */
    g_autofree char *path = NULL;
    g_autoptr(GError) error = NULL;

    fd = g_file_open_tmp("whelk-gamma-XXXXXX", &path, &error);

    if (fd < 0) {
      LOG_C_WARN("Night light", "could not create a gamma table: %s",
                 error ? error->message : "no file descriptor");
      return -1;
    }

    g_unlink(path);
  }

  int sized;

  do {
    errno = 0;
    sized = ftruncate(fd, (off_t)bytes);
  } while (sized != 0 && errno == EINTR);

  if (sized != 0) {
    LOG_C_WARN("Night light", "could not size a gamma table: %s",
               g_strerror(errno));
    close(fd);
    return -1;
  }

  return fd;
}

static void apply_output(WhelkWaylandOutput *output) {
  WhelkWaylandNightLight *backend = output->backend;

  if (!backend->alive || !backend->has_color)
    return;

  ensure_control(output);

  if (!output->control || output->ramp_size == 0)
    return;

  const guint size = output->ramp_size;

  if (!output->reported) {
    LOG_C_INFO("Night light", "gamma table: %u entries per channel", size);
    output->reported = TRUE;
  }

  const gsize bytes = (gsize)size * 3 * sizeof(guint16);
  const int fd = create_table_fd(bytes);

  if (fd < 0)
    return;

  void *mapped = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);

  if (mapped == MAP_FAILED) {
    LOG_C_WARN("Night light", "could not map a gamma table: %s",
               g_strerror(errno));
    close(fd);
    return;
  }

  guint16 *table = mapped;

  whelk_night_light_fill_ramp(size, backend->rgb, table, table + size,
                              table + 2 * size);
  munmap(mapped, bytes);

  zwlr_gamma_control_v1_set_gamma(output->control, fd);
  close(fd);
}

static gboolean neutral(const double rgb[3]) {
  return fabs(rgb[0] - 1.0) < 0.000001 && fabs(rgb[1] - 1.0) < 0.000001 &&
         fabs(rgb[2] - 1.0) < 0.000001;
}

static void release_controls(WhelkWaylandNightLight *backend) {
  for (GList *item = backend->outputs; item; item = item->next) {
    WhelkWaylandOutput *output = item->data;

    g_clear_pointer(&output->control, zwlr_gamma_control_v1_destroy);
    output->ramp_size = 0;
    output->failed = FALSE;
    output->retry_at = 0;
    output->warned = FALSE;
    output->reported = FALSE;
  }
}

static void on_gamma_size(void *data, struct zwlr_gamma_control_v1 *control,
                          uint32_t size) {
  WhelkWaylandOutput *output = data;

  /* An event can already be queued when the control is released. */
  if (output->control != control)
    return;

  output->ramp_size = size;
  output->warned = FALSE;
  apply_output(output);
}

static void on_gamma_failed(void *data, struct zwlr_gamma_control_v1 *control) {
  WhelkWaylandOutput *output = data;

  if (output->control != control)
    return;

  /* Once per output per attempt: a refused output must not repeat this every
   * five seconds for the length of a fade. */
  if (!output->warned) {
    LOG_C_WARN("Night light",
               "the compositor refused gamma control for one output; another "
               "client may hold it, or the output has no gamma table");
    output->warned = TRUE;
  }

  output->failed = TRUE;
  output->ramp_size = 0;
  output->retry_at = g_get_monotonic_time() + GAMMA_RETRY_US;
  zwlr_gamma_control_v1_destroy(control);
  output->control = NULL;
}

static const struct zwlr_gamma_control_v1_listener gamma_listener = {
    .gamma_size = on_gamma_size,
    .failed = on_gamma_failed,
};

/* =========================================================================
 * OUTPUTS
 * ========================================================================= */

static void reapply_all(WhelkWaylandNightLight *backend) {
  for (GList *item = backend->outputs; item; item = item->next)
    apply_output(item->data);
}

static void on_output_geometry(void *data, struct wl_output *output, int32_t x,
                               int32_t y, int32_t physical_width,
                               int32_t physical_height, int32_t subpixel,
                               const char *make, const char *model,
                               int32_t transform) {
  (void)data;
  (void)output;
  (void)x;
  (void)y;
  (void)physical_width;
  (void)physical_height;
  (void)subpixel;
  (void)make;
  (void)model;
  (void)transform;
}

static void on_output_mode(void *data, struct wl_output *output,
                           uint32_t flags, int32_t width, int32_t height,
                           int32_t refresh) {
  WhelkWaylandOutput *wayland_output = data;

  (void)output;
  (void)flags;
  (void)width;
  (void)height;
  (void)refresh;

  /* A mode change can reset the CRTC's gamma table; this is the event that
   * says so, and the compositor may not announce anything else. */
  apply_output(wayland_output);
}

static void on_output_done(void *data, struct wl_output *output) {
  (void)data;
  (void)output;
}

static void on_output_scale(void *data, struct wl_output *output,
                            int32_t factor) {
  (void)data;
  (void)output;
  (void)factor;

  /* Logical scale does not touch the CRTC's gamma table; only a mode change
   * does, and on_output_mode is the event for that. */
}

static const struct wl_output_listener output_listener = {
    .geometry = on_output_geometry,
    .mode = on_output_mode,
    .done = on_output_done,
    .scale = on_output_scale,
};

static void output_free(WhelkWaylandOutput *output) {
  if (!output)
    return;

  if (output->control)
    zwlr_gamma_control_v1_destroy(output->control);

  if (output->output)
    wl_output_destroy(output->output);

  g_free(output);
}

/* =========================================================================
 * THE REGISTRY
 * ========================================================================= */

static void on_registry_global(void *data, struct wl_registry *registry,
                               uint32_t name, const char *interface,
                               uint32_t version) {
  WhelkWaylandNightLight *backend = data;

  if (strcmp(interface, zwlr_gamma_control_manager_v1_interface.name) == 0) {
    if (!backend->manager) {
      backend->manager = wl_registry_bind(
          registry, name, &zwlr_gamma_control_manager_v1_interface,
          MIN(version, 1u));
      backend->manager_name = name;
    }

    /* Outputs can be announced before the manager.  If the screen is already
     * warm, the outputs that were waiting get their controls now. */
    if (backend->manager)
      reapply_all(backend);

    return;
  }

  if (strcmp(interface, wl_output_interface.name) != 0)
    return;

  WhelkWaylandOutput *output = g_new0(WhelkWaylandOutput, 1);

  output->backend = backend;
  output->global_name = name;
  output->output =
      wl_registry_bind(registry, name, &wl_output_interface, MIN(version, 2u));

  if (!output->output) {
    g_free(output);
    return;
  }

  wl_output_add_listener(output->output, &output_listener, output);
  backend->outputs = g_list_append(backend->outputs, output);
  apply_output(output);
}

static void on_registry_global_remove(void *data, struct wl_registry *registry,
                                      uint32_t name) {
  WhelkWaylandNightLight *backend = data;

  (void)registry;

  for (GList *item = backend->outputs; item; item = item->next) {
    WhelkWaylandOutput *output = item->data;

    if (output->global_name != name)
      continue;

    backend->outputs = g_list_delete_link(backend->outputs, item);
    output_free(output);
    return;
  }

  /* The manager itself can disappear (a compositor restart in place).  Its
   * controls stay valid by protocol; a later output will simply have none. */
  if (backend->manager && name == backend->manager_name) {
    backend->manager = NULL;
    backend->manager_name = 0;
  }
}

static const struct wl_registry_listener registry_listener = {
    .global = on_registry_global,
    .global_remove = on_registry_global_remove,
};

/* =========================================================================
 * THE GLIB SIDE
 * ========================================================================= */

static gboolean on_display_fd(gint fd, GIOCondition condition,
                              gpointer user_data) {
  WhelkWaylandNightLight *backend = user_data;

  (void)fd;
  (void)condition;

  if (wl_display_dispatch(backend->display) < 0) {
    LOG_C_WARN("Night light", "the Wayland connection carrying gamma control "
                              "closed; the screen keeps its last table");
    backend->alive = FALSE;
    backend->fd_source = 0;
    return G_SOURCE_REMOVE;
  }

  return G_SOURCE_CONTINUE;
}

static gboolean set_color(gpointer data, const double rgb[3]) {
  WhelkWaylandNightLight *backend = data;

  memcpy(backend->rgb, rgb, sizeof(backend->rgb));

  /* Neutral is the controller's "off".  Destroying the controls lets the
   * compositor restore each output's original table and releases the exclusive
   * control, so another calibrator or night light can take over. */
  backend->has_color = !neutral(rgb);

  if (backend->has_color)
    reapply_all(backend);
  else
    release_controls(backend);

  if (!backend->alive)
    return FALSE;

  if (wl_display_flush(backend->display) < 0 && errno != EAGAIN)
    LOG_C_DEBUG("Night light", "could not flush a gamma update: %s",
                g_strerror(errno));

  return TRUE;
}

static void destroy(gpointer data) {
  WhelkWaylandNightLight *backend = data;

  if (!backend)
    return;

  if (backend->fd_source != 0)
    g_source_remove(backend->fd_source);

  for (GList *item = backend->outputs; item; item = item->next)
    output_free(item->data);

  g_list_free(backend->outputs);

  if (backend->manager)
    zwlr_gamma_control_manager_v1_destroy(backend->manager);

  if (backend->registry)
    wl_registry_destroy(backend->registry);

  if (backend->display) {
    wl_display_flush(backend->display);
    wl_display_disconnect(backend->display);
  }

  g_free(backend);
}

WhelkNightLightBackend *whelk_night_light_wayland_backend_new(void) {
  WhelkWaylandNightLight *backend = g_new0(WhelkWaylandNightLight, 1);

  backend->display = wl_display_connect(NULL);

  if (!backend->display) {
    g_free(backend);
    return NULL;
  }

  backend->alive = TRUE;
  backend->rgb[0] = 1.0;
  backend->rgb[1] = 1.0;
  backend->rgb[2] = 1.0;
  backend->registry = wl_display_get_registry(backend->display);

  if (!backend->registry) {
    destroy(backend);
    return NULL;
  }

  wl_registry_add_listener(backend->registry, &registry_listener, backend);

  if (wl_display_roundtrip(backend->display) < 0 || !backend->manager) {
    destroy(backend);
    return NULL;
  }

  backend->fd_source = g_unix_fd_add(
      wl_display_get_fd(backend->display),
      G_IO_IN | G_IO_ERR | G_IO_HUP, on_display_fd, backend);

  WhelkNightLightBackend *result = g_new0(WhelkNightLightBackend, 1);

  result->name = "wlr-gamma-control";
  result->set_color = set_color;
  result->destroy = destroy;
  result->data = backend;

  return result;
}

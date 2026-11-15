// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/riverstatus.h"

#include <errno.h>
#include <glib-unix.h>
#include <string.h>
#include <wayland-client.h>

#include "logger.h"
#include "river-status-client-protocol.h"

struct _WhelkRiverStatus {
  struct wl_display *display;
  struct wl_registry *registry;
  struct zriver_status_manager_v1 *manager;
  GPtrArray *seats; /* RiverSeat*, one per wl_seat global */
  guint source_id;
  WhelkRiverViewFunc callback;
  gpointer callback_data;
  char *title;
};

typedef struct {
  struct wl_seat *seat;
  struct zriver_seat_status_v1 *status;
} RiverSeat;

static void river_seat_free(RiverSeat *river) {
  if (!river)
    return;

  if (river->status)
    zriver_seat_status_v1_destroy(river->status);

  if (river->seat)
    wl_seat_destroy(river->seat);

  g_free(river);
}

static void on_focused_view(void *data, struct zriver_seat_status_v1 *status,
                            const char *title) {
  (void)status;

  WhelkRiverStatus *self = data;

  g_free(self->title);
  self->title = g_strdup(title ? title : "");

  if (self->callback)
    self->callback(self->title, self->callback_data);
}

static void on_focused_output(void *data, struct zriver_seat_status_v1 *status,
                              struct wl_output *output) {
  (void)data;
  (void)status;
  (void)output;
}

static void on_unfocused_output(void *data, struct zriver_seat_status_v1 *status,
                                struct wl_output *output) {
  (void)data;
  (void)status;
  (void)output;
}

static void on_mode(void *data, struct zriver_seat_status_v1 *status,
                    const char *name) {
  (void)data;
  (void)status;
  (void)name;
}

static const struct zriver_seat_status_v1_listener seat_status_listener = {
    .focused_output = on_focused_output,
    .unfocused_output = on_unfocused_output,
    .focused_view = on_focused_view,
    .mode = on_mode,
};

static void bind_seat_status(WhelkRiverStatus *self, RiverSeat *river) {
  if (!self->manager || river->status)
    return;

  river->status =
      zriver_status_manager_v1_get_river_seat_status(self->manager, river->seat);

  if (river->status)
    zriver_seat_status_v1_add_listener(river->status, &seat_status_listener,
                                       self);
}

static void on_registry_global(void *data, struct wl_registry *registry,
                               uint32_t name, const char *interface,
                               uint32_t version) {
  WhelkRiverStatus *self = data;

  if (g_strcmp0(interface, zriver_status_manager_v1_interface.name) == 0) {
    if (!self->manager) {
      /* Version 3 carries focused_view and mode; the manager's later version
       * only adds output layout events this client does not use. */
      self->manager = wl_registry_bind(
          registry, name, &zriver_status_manager_v1_interface, MIN(version, 3));

      for (guint i = 0; i < self->seats->len; i++)
        bind_seat_status(self, g_ptr_array_index(self->seats, i));
    }
  } else if (g_strcmp0(interface, wl_seat_interface.name) == 0) {
    RiverSeat *river = g_new0(RiverSeat, 1);

    river->seat =
        wl_registry_bind(registry, name, &wl_seat_interface, MIN(version, 1));
    g_ptr_array_add(self->seats, river);
    bind_seat_status(self, river);
  }

  wl_display_flush(self->display);
}

static void on_registry_remove(void *data, struct wl_registry *registry,
                               uint32_t name) {
  (void)data;
  (void)registry;
  (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = on_registry_global,
    .global_remove = on_registry_remove,
};

static gboolean on_display_ready(gint fd, GIOCondition condition, gpointer data) {
  (void)fd;

  WhelkRiverStatus *self = data;

  if (condition & G_IO_IN) {
    if (wl_display_dispatch(self->display) < 0) {
      LOG_C_WARN("River", "river-status connection failed: %s",
                 strerror(wl_display_get_error(self->display)));
      self->source_id = 0;
      return G_SOURCE_REMOVE;
    }

    /* Requests made from the registry callbacks are flushed here. */
    if (wl_display_flush(self->display) < 0) {
      LOG_C_WARN("River", "could not flush the river-status connection: %s",
                 strerror(errno));
      self->source_id = 0;
      return G_SOURCE_REMOVE;
    }
  }

  if (condition & (G_IO_ERR | G_IO_HUP)) {
    LOG_C_WARN("River", "river-status connection closed");
    self->source_id = 0;
    return G_SOURCE_REMOVE;
  }

  return G_SOURCE_CONTINUE;
}

WhelkRiverStatus *whelk_river_status_new(WhelkRiverViewFunc callback,
                                         gpointer data) {
  WhelkRiverStatus *self = g_new0(WhelkRiverStatus, 1);

  self->callback = callback;
  self->callback_data = data;
  self->seats = g_ptr_array_new_with_free_func((GDestroyNotify)river_seat_free);
  self->display = wl_display_connect(NULL);

  if (!self->display) {
    LOG_C_DEBUG("River", "no Wayland connection for river-status: %s",
                strerror(errno));
    whelk_river_status_free(self);
    return NULL;
  }

  self->registry = wl_display_get_registry(self->display);

  if (!self->registry) {
    LOG_C_WARN("River", "could not create the river-status registry");
    whelk_river_status_free(self);
    return NULL;
  }

  wl_registry_add_listener(self->registry, &registry_listener, self);
  wl_display_flush(self->display);

  self->source_id = g_unix_fd_add(wl_display_get_fd(self->display),
                                  G_IO_IN | G_IO_ERR | G_IO_HUP,
                                  on_display_ready, self);

  if (self->source_id == 0) {
    LOG_C_WARN("River", "could not watch the river-status connection");
    whelk_river_status_free(self);
    return NULL;
  }

  return self;
}

void whelk_river_status_free(WhelkRiverStatus *self) {
  if (!self)
    return;

  if (self->source_id != 0)
    g_source_remove(self->source_id);

  g_ptr_array_unref(self->seats);

  if (self->manager)
    zriver_status_manager_v1_destroy(self->manager);

  if (self->registry)
    wl_registry_destroy(self->registry);

  if (self->display)
    wl_display_disconnect(self->display);

  g_free(self->title);
  g_free(self);
}

const char *whelk_river_status_title(WhelkRiverStatus *self) {
  return self ? self->title : NULL;
}

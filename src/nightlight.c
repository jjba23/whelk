// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "nightlight.h"

#include <math.h>
#include <string.h>

#include "logger.h"
#include "nightlight_backend.h"

/* =========================================================================
 * THE CLOCK AND THE COLOUR
 * =========================================================================
 * The controller wakes on a timer, asks the pure schedule what the strength is
 * now, and sends a ramp only when the resulting colour differs from the one
 * already on screen.  Outside a fade that means no gamma traffic at all, which
 * matters: niri's own release note points out that setting gamma is not free.
 */

#define COLOR_EPSILON 0.001

struct _WhelkNightLight {
  WhelkNightLightConfig config;
  WhelkNightLightBackend *backend;
  double applied[3];
  gboolean applied_once;
  guint timer_id;
};

static double now_minutes(void) {
  g_autoptr(GDateTime) now = g_date_time_new_now_local();

  return (double)(g_date_time_get_hour(now) * 60 +
                  g_date_time_get_minute(now)) +
         (double)g_date_time_get_second(now) / 60.0;
}

static void apply_now(WhelkNightLight *night) {
  const double strength =
      night->config.enabled
          ? whelk_night_light_strength(&night->config, now_minutes())
          : 0.0;
  const gboolean want_neutral = strength <= 0.0;
  double rgb[3];

  /* Nothing to do before whelk ever touched the screen.  This covers the
   * middle of the day as well as a disabled filter: the gamma may belong to a
   * distribution's night light, a calibrator or another tool, and a scheduled
   * filter that is currently at zero must not take exclusive control of the
   * outputs just to write an identity table. */
  if (want_neutral && !night->applied_once)
    return;

  if (want_neutral) {
    rgb[0] = 1.0;
    rgb[1] = 1.0;
    rgb[2] = 1.0;
  } else {
    whelk_night_light_mix(night->config.temperature, strength, rgb);
  }

  if (night->applied_once && fabs(rgb[0] - night->applied[0]) < COLOR_EPSILON &&
      fabs(rgb[1] - night->applied[1]) < COLOR_EPSILON &&
      fabs(rgb[2] - night->applied[2]) < COLOR_EPSILON)
    return;

  if (!night->backend ||
      !night->backend->set_color(night->backend->data, rgb))
    return;

  memcpy(night->applied, rgb, sizeof(night->applied));
  night->applied_once = !want_neutral;
}

static void schedule_next(WhelkNightLight *night);

static gboolean on_tick(gpointer user_data) {
  WhelkNightLight *night = user_data;

  night->timer_id = 0;
  apply_now(night);
  schedule_next(night);

  return G_SOURCE_REMOVE;
}

static void schedule_next(WhelkNightLight *night) {
  if (night->timer_id != 0) {
    g_source_remove(night->timer_id);
    night->timer_id = 0;
  }

  /* A disabled filter, an always-on one and an all-day window cannot change
   * colour with the clock, so none needs a wakeup.  A later set_config starts
   * the timer again if the policy becomes scheduled. */
  if (!night->config.enabled ||
      night->config.mode == WHELK_NIGHT_LIGHT_ALWAYS ||
      night->config.start_minute == night->config.end_minute)
    return;

  const guint seconds =
      whelk_night_light_next_tick_seconds(&night->config, now_minutes());

  night->timer_id = g_timeout_add_seconds(seconds, on_tick, night);
}

WhelkNightLight *whelk_night_light_new(void) {
  WhelkNightLight *night = g_new0(WhelkNightLight, 1);

  night->config.configured = FALSE;
  night->config.enabled = FALSE;
  night->config.mode = WHELK_NIGHT_LIGHT_SCHEDULED;
  night->config.temperature = 3400;
  night->config.start_minute = 19 * 60;
  night->config.end_minute = 7 * 60;
  night->config.transition_minutes = 30;

  const char *wayland_display = g_getenv("WAYLAND_DISPLAY");

  if (wayland_display && *wayland_display) {
    night->backend = whelk_night_light_wayland_backend_new();

    if (!night->backend)
      LOG_C_WARN("Night light",
                 "this Wayland compositor does not offer "
                 "wlr-gamma-control-unstable-v1, so whelk cannot change the "
                 "screen's gamma; the setting is kept for a session that can");
  } else {
    night->backend = whelk_night_light_x11_backend_new();

    if (!night->backend)
      LOG_C_WARN("Night light",
                 "this X display has no RandR gamma table whelk can use; the "
                 "setting is kept for a session that has one");
  }

  if (night->backend)
    LOG_C_INFO("Night light", "using %s", night->backend->name);

  return night;
}

void whelk_night_light_set_config(WhelkNightLight *night,
                                  const WhelkNightLightConfig *config) {
  if (!night || !config)
    return;

  night->config = *config;
  whelk_night_light_config_normalize(&night->config);

  if (night->config.enabled) {
    LOG_C_INFO("Night light", "%d K, %02d:%02d to %02d:%02d, %d min fade (%s)",
               night->config.temperature, night->config.start_minute / 60,
               night->config.start_minute % 60, night->config.end_minute / 60,
               night->config.end_minute % 60,
               night->config.transition_minutes,
               night->config.mode == WHELK_NIGHT_LIGHT_ALWAYS ? "always"
                                                              : "scheduled");
  } else {
    LOG_C_INFO("Night light", "off");
  }

  apply_now(night);
  schedule_next(night);
}

void whelk_night_light_free(WhelkNightLight *night) {
  if (!night)
    return;

  if (night->timer_id != 0) {
    g_source_remove(night->timer_id);
    night->timer_id = 0;
  }

  g_clear_pointer(&night->backend, whelk_night_light_backend_free);
  g_free(night);
}

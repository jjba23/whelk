// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_NIGHTLIGHT_H
#define WHELK_NIGHTLIGHT_H

#include <glib.h>

G_BEGIN_DECLS

/* =========================================================================
 * NIGHT LIGHT, THE ARITHMETIC
 * =========================================================================
 * A night light is a white point moved towards the red end of the blackbody
 * curve and the display's gamma tables set to match.  Everything here is pure:
 * a temperature becomes three channel multipliers, a configuration and a time
 * of day become a strength from 0 to 1, and a ramp size turns that into the
 * 16-bit table a display server wants.
 *
 * The backends (src/nightlight_wayland.c and src/nightlight_x11.c) only carry
 * the table to the server.  Keeping the arithmetic apart is what lets the
 * curve, the schedule and the ramp be tested without a monitor.
 */

#define WHELK_NIGHT_LIGHT_MIN_TEMPERATURE 1000
#define WHELK_NIGHT_LIGHT_MAX_TEMPERATURE 6500
#define WHELK_NIGHT_LIGHT_NEUTRAL 6500

/**
 * WhelkNightLightMode:
 * @WHELK_NIGHT_LIGHT_SCHEDULED: warm between @start_minute and @end_minute.
 * @WHELK_NIGHT_LIGHT_ALWAYS: warm for as long as it is enabled.
 */
typedef enum {
  WHELK_NIGHT_LIGHT_SCHEDULED,
  WHELK_NIGHT_LIGHT_ALWAYS,
} WhelkNightLightMode;

/**
 * WhelkNightLightConfig:
 * @configured: whether the settings file described one at all.
 * @enabled: whether the filter is on.
 * @mode: scheduled or always.
 * @temperature: the white point in kelvin, clamped to
 *   [WHELK_NIGHT_LIGHT_MIN_TEMPERATURE, WHELK_NIGHT_LIGHT_MAX_TEMPERATURE].
 * @start_minute: when the warm window opens, minutes from midnight.
 * @end_minute: when it closes; a value smaller than @start_minute means the
 *   window crosses midnight.
 * @transition_minutes: how long the fade in and the fade out take.  Clamped to
 *   half the window when it would not otherwise fit.
 *
 * One configuration, resolved from the settings file or from a GUI override.
 */
typedef struct {
  gboolean configured;
  gboolean enabled;
  WhelkNightLightMode mode;
  int temperature;
  int start_minute;
  int end_minute;
  int transition_minutes;
} WhelkNightLightConfig;

/* A disabled, scheduled configuration: 3400 K from 19:00 to 07:00, fading for
 * half an hour at each end.  The caller frees it with
 * whelk_night_light_config_free(). */
WhelkNightLightConfig *whelk_night_light_config_new(void);

/* Clamps every field to what the arithmetic can use. */
void whelk_night_light_config_normalize(WhelkNightLightConfig *config);

void whelk_night_light_config_free(WhelkNightLightConfig *config);

G_DEFINE_AUTOPTR_CLEANUP_FUNC(WhelkNightLightConfig,
                              whelk_night_light_config_free)

/**
 * whelk_night_light_color:
 * @temperature: kelvin; values outside the supported range are clamped.
 * @rgb: (out) (array fixed-size=3): the red, green and blue multipliers,
 *   each between 0 and 1, with 6500 K exactly (1, 1, 1).
 *
 * The blackbody colour of @temperature, normalised to the neutral white point
 * so a night light never tints a screen that is not scheduled to be warm.
 */
void whelk_night_light_color(int temperature, double rgb[3]);

/**
 * whelk_night_light_strength:
 * @config: the night light.
 * @minutes_of_day: local time as minutes from midnight (0.0 to 1440.0).
 *
 * Returns: 0 when the screen should be neutral, 1 at the target white point,
 *   and the fraction of a fade in between.
 */
double whelk_night_light_strength(const WhelkNightLightConfig *config,
                                  double minutes_of_day);

/**
 * whelk_night_light_mix:
 * @temperature: the target white point.
 * @strength: 0 to 1.
 * @rgb: (out) (array fixed-size=3): the channel multipliers to apply now.
 *
 * Blends the neutral white point with @temperature's colour.  A fade moves the
 * ramp, not just the clock, so a transition is a slow slide rather than a step.
 */
void whelk_night_light_mix(int temperature, double strength, double rgb[3]);

/**
 * whelk_night_light_fill_ramp:
 * @size: entries per channel; a size below 1 is treated as 1.
 * @rgb: (array fixed-size=3): the channel multipliers.
 * @red: (out) (array length=size): the red ramp.
 * @green: (out) (array length=size): the green ramp.
 * @blue: (out) (array length=size): the blue ramp.
 *
 * Fills three 16-bit gamma ramps with the identity curve scaled by each
 * multiplier: entry i is i/(size-1) of full scale, which is what both RandR
 * and wlr-gamma-control expect.
 */
void whelk_night_light_fill_ramp(guint size, const double rgb[3],
                                 guint16 *red, guint16 *green, guint16 *blue);

/**
 * whelk_night_light_parse_clock:
 * @text: "HH:MM", with a 0-23 hour and a two-digit minute.
 * @minutes: (out): minutes from midnight.
 *
 * Returns: TRUE for a valid clock, FALSE otherwise.  The settings window and
 * the Scheme constructor accept the same spelling, so the same text cannot
 * mean two different times on the two sides of the bridge.
 */
gboolean whelk_night_light_parse_clock(const char *text, int *minutes);

/**
 * whelk_night_light_next_tick_seconds:
 * @config: the night light.
 * @minutes_of_day: the time the tick is being planned for.
 *
 * Returns: how long until the strength can next change by a visible amount:
 *   a short step during a fade, and at most a minute otherwise.  A controller
 *   that wakes on this costs nothing while nothing is moving.
 */
guint whelk_night_light_next_tick_seconds(const WhelkNightLightConfig *config,
                                          double minutes_of_day);

G_END_DECLS

#endif // WHELK_NIGHTLIGHT_H

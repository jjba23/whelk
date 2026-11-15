// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/nightlight.h"

#include <math.h>
#include <string.h>

/* =========================================================================
 * THE WHITE POINT
 * =========================================================================
 * The curve is the blackbody approximation night-light tools have used for
 * years (Tanner Helland's fit to the Planckian locus), normalised at 6500 K so
 * the neutral setting is exactly white.  Without that normalisation the fit's
 * own 6500 K value is a slightly warm off-white, and simply enabling whelk
 * would tint the screen even when the schedule says it should not.
 *
 * Only the blue and green channels are ever reduced: a ramp that lifts a
 * channel above full scale clips, and clipped video is worse than a cool
 * picture.
 */

#define BLACKBODY_REFERENCE 6500.0

static double clamp_unit(double value) { return CLAMP(value, 0.0, 1.0); }

static int clamp_temperature(int temperature) {
  return CLAMP(temperature, WHELK_NIGHT_LIGHT_MIN_TEMPERATURE,
               WHELK_NIGHT_LIGHT_MAX_TEMPERATURE);
}

static void blackbody_raw(double kelvin, double rgb[3]) {
  const double temperature = kelvin / 100.0;
  double red;
  double green;
  double blue;

  if (temperature <= 66.0) {
    red = 255.0;
    green = 99.4708025861 * log(temperature) - 161.1195681661;

    if (temperature <= 19.0)
      blue = 0.0;
    else
      blue = 138.5177312231 * log(temperature - 10.0) - 305.0447927307;
  } else {
    red = 329.698727446 * pow(temperature - 60.0, -0.1332047592);
    green = 288.1221695283 * pow(temperature - 60.0, -0.0755148492);
    blue = 255.0;
  }

  rgb[0] = clamp_unit(red / 255.0);
  rgb[1] = clamp_unit(green / 255.0);
  rgb[2] = clamp_unit(blue / 255.0);
}

void whelk_night_light_color(int temperature, double rgb[3]) {
  double reference[3];
  double colour[3];

  blackbody_raw(BLACKBODY_REFERENCE, reference);
  blackbody_raw(clamp_temperature(temperature), colour);

  for (int channel = 0; channel < 3; channel++) {
    rgb[channel] =
        reference[channel] > 0.0 ? clamp_unit(colour[channel] / reference[channel])
                                 : 1.0;
  }
}

void whelk_night_light_mix(int temperature, double strength, double rgb[3]) {
  double colour[3];
  const double amount = clamp_unit(strength);

  whelk_night_light_color(temperature, colour);

  for (int channel = 0; channel < 3; channel++)
    rgb[channel] = 1.0 + amount * (colour[channel] - 1.0);
}

/* =========================================================================
 * THE SCHEDULE
 * =========================================================================
 * The window may cross midnight, and its fades may be longer than the window
 * itself: an hour-long window cannot spend two hours fading.  Both are decided
 * here so the two backends only ever see a strength.
 */

static double normalize_minutes(double minutes) {
  double value = fmod(minutes, 1440.0);

  return value < 0.0 ? value + 1440.0 : value;
}

double whelk_night_light_strength(const WhelkNightLightConfig *config,
                                  double minutes_of_day) {
  if (!config || !config->enabled)
    return 0.0;

  if (config->mode == WHELK_NIGHT_LIGHT_ALWAYS)
    return 1.0;

  const double start = normalize_minutes((double)config->start_minute);
  const double end = normalize_minutes((double)config->end_minute);

  /* Equal ends describe a window with no dark half.  Reading it as "all day"
   * is more useful than a schedule that can never be on. */
  if (start == end)
    return 1.0;

  const double window = normalize_minutes(end - start + 1440.0);
  const double elapsed = normalize_minutes(minutes_of_day - start);

  if (elapsed >= window)
    return 0.0;

  const double transition = MIN((double)MAX(config->transition_minutes, 0),
                                window / 2.0);

  if (transition <= 0.0)
    return 1.0;

  /* Both ends fade: the start is dark and climbs, the end is bright and
   * falls.  When the transition fills the window the two meet in the middle
   * at exactly half strength. */
  return clamp_unit(MIN(elapsed / transition, (window - elapsed) / transition));
}

guint whelk_night_light_next_tick_seconds(const WhelkNightLightConfig *config,
                                          double minutes_of_day) {
  if (!config || !config->enabled || config->mode == WHELK_NIGHT_LIGHT_ALWAYS)
    return 300;

  const double strength = whelk_night_light_strength(config, minutes_of_day);

  /* A fade is the only time a small step is visible; five seconds is finer
   * than the eye can follow on a thirty-minute transition. */
  if (strength > 0.0 && strength < 1.0)
    return 5;

  const double start = normalize_minutes((double)config->start_minute);
  const double end = normalize_minutes((double)config->end_minute);

  if (start == end)
    return 300;

  const double window = normalize_minutes(end - start + 1440.0);
  const double elapsed = normalize_minutes(minutes_of_day - start);
  double until_next;

  if (elapsed < window) {
    const double transition =
        MIN((double)MAX(config->transition_minutes, 0), window / 2.0);

    /* Exactly at the opening edge the strength is still zero, but the fade is
     * about to move: a one-minute sleep there would make every schedule start
     * a minute late. */
    if (transition > 0.0 &&
        (elapsed < transition || window - elapsed < transition))
      return 5;

    const double fade_out = normalize_minutes(start + window - transition);

    until_next = normalize_minutes(fade_out - minutes_of_day);
  } else {
    until_next = normalize_minutes(start - minutes_of_day);
  }

  /* A minute outside the window is a cheap wake-up and keeps the schedule
   * honest across a suspend or a timezone change; a boundary that is already
   * behind us waits for the next tick rather than spinning. */
  return (guint)CLAMP((int)ceil(until_next * 60.0), 5, 60);
}

/* =========================================================================
 * THE RAMP
 * =========================================================================
 * Entry i is i/(size-1) of full scale, scaled by the channel's multiplier.
 * This is the identity curve, not a gamma exponent: a night light moves the
 * white point, and darkening the midtones on the way is a second, unasked-for
 * change.
 */

void whelk_night_light_fill_ramp(guint size, const double rgb[3],
                                 guint16 *red, guint16 *green, guint16 *blue) {
  const guint entries = MAX(size, 1u);
  const double divisor = entries > 1 ? (double)(entries - 1) : 1.0;

  for (guint i = 0; i < entries; i++) {
    /* A one-entry ramp has no other level to be, so it is full scale rather
     * than the blank table a division by zero would leave behind. */
    const double level = entries > 1 ? (double)i / divisor : 1.0;

    red[i] = (guint16)lround(level * clamp_unit(rgb[0]) * 65535.0);
    green[i] = (guint16)lround(level * clamp_unit(rgb[1]) * 65535.0);
    blue[i] = (guint16)lround(level * clamp_unit(rgb[2]) * 65535.0);
  }
}

/* =========================================================================
 * CONFIGURATION
 * ========================================================================= */

WhelkNightLightConfig *whelk_night_light_config_new(void) {
  WhelkNightLightConfig *config = g_new0(WhelkNightLightConfig, 1);

  config->configured = FALSE;
  config->enabled = FALSE;
  config->mode = WHELK_NIGHT_LIGHT_SCHEDULED;
  config->temperature = 3400;
  config->start_minute = 19 * 60;
  config->end_minute = 7 * 60;
  config->transition_minutes = 30;

  return config;
}

void whelk_night_light_config_normalize(WhelkNightLightConfig *config) {
  if (!config)
    return;

  if (config->mode != WHELK_NIGHT_LIGHT_ALWAYS)
    config->mode = WHELK_NIGHT_LIGHT_SCHEDULED;

  config->temperature = clamp_temperature(config->temperature);
  config->start_minute = CLAMP(config->start_minute, 0, 1439);
  config->end_minute = CLAMP(config->end_minute, 0, 1439);
  config->transition_minutes = CLAMP(config->transition_minutes, 0, 1440);
}

void whelk_night_light_config_free(WhelkNightLightConfig *config) {
  g_free(config);
}

gboolean whelk_night_light_parse_clock(const char *text, int *minutes) {
  if (!text || strlen(text) != 5 || text[2] != ':' || !g_ascii_isdigit(text[0]) ||
      !g_ascii_isdigit(text[1]) || !g_ascii_isdigit(text[3]) ||
      !g_ascii_isdigit(text[4]))
    return FALSE;

  const int hour = (text[0] - '0') * 10 + (text[1] - '0');
  const int minute = (text[3] - '0') * 10 + (text[4] - '0');

  if (hour > 23 || minute > 59)
    return FALSE;

  if (minutes)
    *minutes = hour * 60 + minute;

  return TRUE;
}

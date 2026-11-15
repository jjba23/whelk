// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The night light's arithmetic, without a screen: the colour of a temperature,
// the shape of a scheduled window and the gamma ramp a display server receives.
// The backends (wlr-gamma-control and RandR) are exercised by the shell itself;
// what can be wrong in a way a screenshot would not show is tested here.

#include "util/nightlight.h"

#include <glib.h>
#include <math.h>

static void test_blackbody_neutral(void) {
  double rgb[3];

  whelk_night_light_color(WHELK_NIGHT_LIGHT_NEUTRAL, rgb);

  g_assert_cmpfloat_with_epsilon(rgb[0], 1.0, 0.0001);
  g_assert_cmpfloat_with_epsilon(rgb[1], 1.0, 0.0001);
  g_assert_cmpfloat_with_epsilon(rgb[2], 1.0, 0.0001);
}

static void test_blackbody_warm(void) {
  double warm[3];
  double warmest[3];

  whelk_night_light_color(3400, warm);
  whelk_night_light_color(1000, warmest);

  /* Warm means less blue and a little less green, never more red: a ramp that
   * lifts a channel above full scale clips and loses detail instead. */
  g_assert_cmpfloat(warm[0], >=, warm[1]);
  g_assert_cmpfloat(warm[1], >, warm[2]);
  g_assert_cmpfloat(warm[2], <, 0.75);
  g_assert_cmpfloat(warm[0], <=, 1.0);

  g_assert_cmpfloat(warmest[2], <, 0.01);
  g_assert_cmpfloat(warmest[1], >, 0.0);
}

static void test_blackbody_clamps(void) {
  double too_cold[3];
  double coldest[3];
  double too_hot[3];
  double neutral[3];

  whelk_night_light_color(-4000, too_cold);
  whelk_night_light_color(WHELK_NIGHT_LIGHT_MIN_TEMPERATURE, coldest);
  whelk_night_light_color(12000, too_hot);
  whelk_night_light_color(WHELK_NIGHT_LIGHT_NEUTRAL, neutral);

  for (int channel = 0; channel < 3; channel++) {
    g_assert_cmpfloat_with_epsilon(too_cold[channel], coldest[channel], 0.0001);
    g_assert_cmpfloat_with_epsilon(too_hot[channel], neutral[channel], 0.0001);
  }
}

static WhelkNightLightConfig *scheduled(int start, int end, int transition) {
  WhelkNightLightConfig *config = whelk_night_light_config_new();

  config->enabled = TRUE;
  config->mode = WHELK_NIGHT_LIGHT_SCHEDULED;
  config->start_minute = start;
  config->end_minute = end;
  config->transition_minutes = transition;

  return config;
}

static void test_schedule_overnight(void) {
  g_autoptr(WhelkNightLightConfig) config = scheduled(19 * 60, 7 * 60, 30);

  g_assert_cmpfloat(whelk_night_light_strength(config, 12 * 60), ==, 0.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 18 * 60 + 59), ==, 0.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 19 * 60), ==, 0.0);
  g_assert_cmpfloat_with_epsilon(whelk_night_light_strength(config, 19 * 60 + 15),
                                 0.5, 0.0001);
  g_assert_cmpfloat(whelk_night_light_strength(config, 23 * 60), ==, 1.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 0), ==, 1.0);
  g_assert_cmpfloat_with_epsilon(whelk_night_light_strength(config, 6 * 60 + 45),
                                 0.5, 0.0001);
  g_assert_cmpfloat(whelk_night_light_strength(config, 7 * 60), ==, 0.0);
}

static void test_schedule_daytime(void) {
  g_autoptr(WhelkNightLightConfig) config = scheduled(8 * 60, 17 * 60, 0);

  g_assert_cmpfloat(whelk_night_light_strength(config, 7 * 60), ==, 0.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 8 * 60), ==, 1.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 12 * 60), ==, 1.0);
  g_assert_cmpfloat(whelk_night_light_strength(config, 17 * 60), ==, 0.0);
}

static void test_schedule_modes(void) {
  g_autoptr(WhelkNightLightConfig) disabled = scheduled(19 * 60, 7 * 60, 30);
  g_autoptr(WhelkNightLightConfig) always = scheduled(19 * 60, 7 * 60, 30);
  g_autoptr(WhelkNightLightConfig) all_day = scheduled(12 * 60, 12 * 60, 0);

  disabled->enabled = FALSE;
  g_assert_cmpfloat(whelk_night_light_strength(disabled, 23 * 60), ==, 0.0);

  always->mode = WHELK_NIGHT_LIGHT_ALWAYS;
  g_assert_cmpfloat(whelk_night_light_strength(always, 12 * 60), ==, 1.0);

  /* A window that starts and ends at the same minute has no dark half to
   * describe; reading it as "all day" is the useful answer rather than a
   * schedule that never fires. */
  g_assert_cmpfloat(whelk_night_light_strength(all_day, 3 * 60), ==, 1.0);
}

static void test_schedule_short_window(void) {
  /* An hour-long window cannot fade for two hours.  The transition is clamped
   * to half the window, so the two fades meet exactly at the middle. */
  g_autoptr(WhelkNightLightConfig) config = scheduled(12 * 60, 13 * 60, 120);

  g_assert_cmpfloat(whelk_night_light_strength(config, 12 * 60), ==, 0.0);
  g_assert_cmpfloat_with_epsilon(whelk_night_light_strength(config, 12 * 60 + 15),
                                 0.5, 0.0001);
  g_assert_cmpfloat(whelk_night_light_strength(config, 12 * 60 + 30), ==, 1.0);
  g_assert_cmpfloat_with_epsilon(whelk_night_light_strength(config, 12 * 60 + 45),
                                 0.5, 0.0001);
  g_assert_cmpfloat(whelk_night_light_strength(config, 13 * 60), ==, 0.0);
}

static void test_mix(void) {
  double off[3];
  double half[3];
  double full[3];
  double target[3];

  whelk_night_light_mix(3400, 0.0, off);
  whelk_night_light_mix(3400, 0.5, half);
  whelk_night_light_mix(3400, 1.0, full);
  whelk_night_light_color(3400, target);

  for (int channel = 0; channel < 3; channel++) {
    g_assert_cmpfloat_with_epsilon(off[channel], 1.0, 0.0001);
    g_assert_cmpfloat_with_epsilon(full[channel], target[channel], 0.0001);
    g_assert_cmpfloat_with_epsilon(
        half[channel], (1.0 + target[channel]) / 2.0, 0.0001);
  }

  /* Out-of-range strengths are clamped rather than extrapolated into a
   * negative or over-bright ramp. */
  whelk_night_light_mix(3400, -2.0, off);
  whelk_night_light_mix(3400, 4.0, full);
  g_assert_cmpfloat_with_epsilon(off[2], 1.0, 0.0001);
  g_assert_cmpfloat_with_epsilon(full[2], target[2], 0.0001);
}

static void test_next_tick(void) {
  g_autoptr(WhelkNightLightConfig) config = scheduled(19 * 60, 7 * 60, 30);
  g_autoptr(WhelkNightLightConfig) always = scheduled(19 * 60, 7 * 60, 30);

  /* A fade is the only time a small step is visible, including the exact
   * opening edge where the strength is still zero but is about to climb. */
  g_assert_cmpuint(whelk_night_light_next_tick_seconds(config, 19 * 60), ==,
                   5);
  g_assert_cmpuint(whelk_night_light_next_tick_seconds(config, 19 * 60 + 15),
                   ==, 5);

  /* Outside the window the next event is minutes away; the cap keeps a
   * suspend, a timezone change or a clock set by hand from stranding the
   * schedule for hours. */
  g_assert_cmpuint(whelk_night_light_next_tick_seconds(config, 12 * 60), ==,
                   60);
  g_assert_cmpuint(whelk_night_light_next_tick_seconds(config, 18 * 60 + 59),
                   <=, 60);

  always->mode = WHELK_NIGHT_LIGHT_ALWAYS;
  g_assert_cmpuint(whelk_night_light_next_tick_seconds(always, 12 * 60), ==,
                   300);
}

static void test_clock_parsing(void) {
  int minutes = -1;

  g_assert_true(whelk_night_light_parse_clock("07:05", &minutes));
  g_assert_cmpint(minutes, ==, 7 * 60 + 5);

  g_assert_true(whelk_night_light_parse_clock("23:59", &minutes));
  g_assert_cmpint(minutes, ==, 23 * 60 + 59);

  g_assert_false(whelk_night_light_parse_clock("24:00", &minutes));
  g_assert_false(whelk_night_light_parse_clock("7", &minutes));
  g_assert_false(whelk_night_light_parse_clock("seven", &minutes));
  g_assert_false(whelk_night_light_parse_clock("07:5", &minutes));
}

static void test_ramp(void) {
  const double rgb[3] = {1.0, 0.5, 0.25};
  guint16 red[4];
  guint16 green[4];
  guint16 blue[4];

  whelk_night_light_fill_ramp(4, rgb, red, green, blue);

  g_assert_cmpuint(red[0], ==, 0);
  g_assert_cmpuint(red[1], ==, 21845);
  g_assert_cmpuint(red[2], ==, 43690);
  g_assert_cmpuint(red[3], ==, 65535);

  g_assert_cmpuint(green[0], ==, 0);
  g_assert_cmpuint(green[1], ==, 10923);
  g_assert_cmpuint(green[2], ==, 21845);
  g_assert_cmpuint(green[3], ==, 32768);

  g_assert_cmpuint(blue[0], ==, 0);
  g_assert_cmpuint(blue[1], ==, 5461);
  g_assert_cmpuint(blue[2], ==, 10923);
  g_assert_cmpuint(blue[3], ==, 16384);
}

static void test_ramp_neutral_and_single(void) {
  const double neutral[3] = {1.0, 1.0, 1.0};
  const double dim[3] = {1.0, 0.5, 0.0};
  guint16 red[1];
  guint16 green[1];
  guint16 blue[1];
  guint16 red2[2];
  guint16 green2[2];
  guint16 blue2[2];

  whelk_night_light_fill_ramp(2, neutral, red2, green2, blue2);
  g_assert_cmpuint(red2[0], ==, 0);
  g_assert_cmpuint(red2[1], ==, 65535);
  g_assert_cmpuint(green2[1], ==, 65535);
  g_assert_cmpuint(blue2[1], ==, 65535);

  /* A ramp of one entry is full scale, not a division by zero. */
  whelk_night_light_fill_ramp(1, dim, red, green, blue);
  g_assert_cmpuint(red[0], ==, 65535);
  g_assert_cmpuint(green[0], ==, 32768);
  g_assert_cmpuint(blue[0], ==, 0);
}

static void test_defaults(void) {
  g_autoptr(WhelkNightLightConfig) config = whelk_night_light_config_new();

  g_assert_false(config->configured);
  g_assert_false(config->enabled);
  g_assert_cmpint(config->mode, ==, WHELK_NIGHT_LIGHT_SCHEDULED);
  g_assert_cmpint(config->temperature, ==, 3400);
  g_assert_cmpint(config->start_minute, ==, 19 * 60);
  g_assert_cmpint(config->end_minute, ==, 7 * 60);
  g_assert_cmpint(config->transition_minutes, ==, 30);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/nightlight/blackbody/neutral", test_blackbody_neutral);
  g_test_add_func("/nightlight/blackbody/warm", test_blackbody_warm);
  g_test_add_func("/nightlight/blackbody/clamps", test_blackbody_clamps);
  g_test_add_func("/nightlight/schedule/overnight", test_schedule_overnight);
  g_test_add_func("/nightlight/schedule/daytime", test_schedule_daytime);
  g_test_add_func("/nightlight/schedule/modes", test_schedule_modes);
  g_test_add_func("/nightlight/schedule/short-window",
                  test_schedule_short_window);
  g_test_add_func("/nightlight/mix", test_mix);
  g_test_add_func("/nightlight/next-tick", test_next_tick);
  g_test_add_func("/nightlight/clock/parse", test_clock_parsing);
  g_test_add_func("/nightlight/ramp", test_ramp);
  g_test_add_func("/nightlight/ramp/neutral-single",
                  test_ramp_neutral_and_single);
  g_test_add_func("/nightlight/defaults", test_defaults);

  return g_test_run();
}

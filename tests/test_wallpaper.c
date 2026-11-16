// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

/* The wallpaper engine's pure half: which files it accepts, what a transition
 * does at a given moment, how a picture fits an output, and where the rotation
 * goes next.  The view and the layer surfaces need a display; these rules do
 * not. */

#include "util/wallpaper.h"

#include <math.h>

static int failures;

static void check(const char *name, gboolean condition) {
  if (!condition) {
    failures++;
    g_printerr("not ok %s\n", name);
  }
}

static gboolean near(double value, double expected) {
  return fabs(value - expected) < 0.0001;
}

static void test_kinds(void) {
  check("png is an image",
        whelk_wallpaper_kind_for_path("/w/a.png") == WHELK_WALLPAPER_IMAGE);
  check("upper-case JPEG is an image",
        whelk_wallpaper_kind_for_path("/w/a.JPEG") == WHELK_WALLPAPER_IMAGE);
  check("webp is an image",
        whelk_wallpaper_kind_for_path("a.webp") == WHELK_WALLPAPER_IMAGE);
  check("avif is an image",
        whelk_wallpaper_kind_for_path("a.avif") == WHELK_WALLPAPER_IMAGE);
  check("gif is an image",
        whelk_wallpaper_kind_for_path("a.gif") == WHELK_WALLPAPER_IMAGE);
  check("mp4 is a video",
        whelk_wallpaper_kind_for_path("a.mp4") == WHELK_WALLPAPER_VIDEO);
  check("webm is a video",
        whelk_wallpaper_kind_for_path("a.WEBM") == WHELK_WALLPAPER_VIDEO);
  check("a text file is not supported",
        !whelk_wallpaper_is_supported("notes.txt"));
  check("a file with no extension is not supported",
        !whelk_wallpaper_is_supported("README"));
  check("a dot in a directory name is not an extension",
        !whelk_wallpaper_is_supported("/home/me/.walls/README"));
}

static void test_names(void) {
  WhelkWallpaperAnimation animation = WHELK_WALLPAPER_ANIMATION_NONE;
  WhelkWallpaperFit fit = WHELK_WALLPAPER_FIT_COVER;

  check("crossfade parses",
        whelk_wallpaper_animation_parse("crossfade", &animation) &&
            animation == WHELK_WALLPAPER_ANIMATION_CROSSFADE);
  check("slide-left parses",
        whelk_wallpaper_animation_parse("slide-left", &animation) &&
            animation == WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT);
  check("an unknown animation is refused",
        !whelk_wallpaper_animation_parse("explode", &animation));

  for (WhelkWallpaperAnimation a = WHELK_WALLPAPER_ANIMATION_NONE;
       a <= WHELK_WALLPAPER_ANIMATION_ZOOM; a++) {
    WhelkWallpaperAnimation back = WHELK_WALLPAPER_ANIMATION_NONE;

    check("every animation name round-trips",
          whelk_wallpaper_animation_parse(whelk_wallpaper_animation_name(a),
                                          &back) &&
              back == a);
  }

  check("contain parses",
        whelk_wallpaper_fit_parse("contain", &fit) &&
            fit == WHELK_WALLPAPER_FIT_CONTAIN);
  check("fill parses",
        whelk_wallpaper_fit_parse("fill", &fit) &&
            fit == WHELK_WALLPAPER_FIT_FILL);
  check("an unknown fit is refused",
        !whelk_wallpaper_fit_parse("stretch", &fit));
}

static void test_transitions(void) {
  check("smoothstep has fixed ends", near(whelk_wallpaper_ease(0.0), 0.0) &&
                                        near(whelk_wallpaper_ease(1.0), 1.0));
  check("a crossfade starts transparent and ends opaque",
        near(whelk_wallpaper_alpha_in(WHELK_WALLPAPER_ANIMATION_CROSSFADE,
                                      0.0),
             0.0) &&
            near(whelk_wallpaper_alpha_in(WHELK_WALLPAPER_ANIMATION_CROSSFADE,
                                          1.0),
                 1.0));
  check("a crossfade keeps the outgoing picture visible",
        near(whelk_wallpaper_alpha_out(WHELK_WALLPAPER_ANIMATION_CROSSFADE,
                                       0.5),
             1.0));

  /* A fade spends its first half taking the old picture to black and its
   * second half bringing the new one up. */
  check("a fade is black in the middle",
        near(whelk_wallpaper_alpha_in(WHELK_WALLPAPER_ANIMATION_FADE, 0.5),
             0.0) &&
            near(whelk_wallpaper_alpha_out(WHELK_WALLPAPER_ANIMATION_FADE,
                                           0.5),
                 0.0));
  check("a fade ends on the new picture",
        near(whelk_wallpaper_alpha_in(WHELK_WALLPAPER_ANIMATION_FADE, 1.0),
             1.0));
  check("a fade starts on the old picture",
        near(whelk_wallpaper_alpha_out(WHELK_WALLPAPER_ANIMATION_FADE, 0.0),
             1.0));

  double out_x = 0.0;
  double out_y = 0.0;
  double in_x = 0.0;
  double in_y = 0.0;

  whelk_wallpaper_offsets(WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT, 0.5, 1000.0,
                          500.0, &out_x, &out_y, &in_x, &in_y);
  check("a left slide moves the old picture out and the new one in",
        near(out_x, -500.0) && near(in_x, 500.0) && near(out_y, 0.0) &&
            near(in_y, 0.0));

  whelk_wallpaper_offsets(WHELK_WALLPAPER_ANIMATION_SLIDE_DOWN, 0.25, 1000.0,
                          500.0, &out_x, &out_y, &in_x, &in_y);
  check("a down slide uses the height",
        near(out_y, 125.0) && near(in_y, -375.0) && near(out_x, 0.0));

  check("zoom starts slightly large and settles",
        whelk_wallpaper_zoom_in(WHELK_WALLPAPER_ANIMATION_ZOOM, 0.0) > 1.0 &&
            near(whelk_wallpaper_zoom_in(WHELK_WALLPAPER_ANIMATION_ZOOM, 1.0),
                 1.0));
  check("a crossfade does not zoom",
        near(whelk_wallpaper_zoom_in(WHELK_WALLPAPER_ANIMATION_CROSSFADE,
                                     0.0),
             1.0));
}

static void test_fit(void) {
  double sx = 0.0;
  double sy = 0.0;
  double x = 0.0;
  double y = 0.0;

  /* A square picture on a 1920x1080 screen is scaled until its height fills
   * the screen; the sides are cropped equally. */
  whelk_wallpaper_fit(WHELK_WALLPAPER_FIT_COVER, 1920.0, 1080.0, 1000.0,
                      1000.0, 1.0, &sx, &sy, &x, &y);
  check("cover fills both axes", near(sx, 1.92) && near(sy, 1.92));
  check("cover centres the crop", near(x, (1920.0 - 1920.0) / 2.0) &&
                                     near(y, (1080.0 - 1920.0) / 2.0));

  whelk_wallpaper_fit(WHELK_WALLPAPER_FIT_CONTAIN, 1920.0, 1080.0, 1000.0,
                      1000.0, 1.0, &sx, &sy, &x, &y);
  check("contain fits the smaller axis", near(sx, 1.08) && near(sy, 1.08));
  check("contain letterboxes", near(x, (1920.0 - 1080.0) / 2.0) &&
                                   near(y, 0.0));

  whelk_wallpaper_fit(WHELK_WALLPAPER_FIT_FILL, 1920.0, 1080.0, 1000.0,
                      1000.0, 1.0, &sx, &sy, &x, &y);
  check("fill stretches each axis", near(sx, 1.92) && near(sy, 1.08));
  check("fill has no letterbox", near(x, 0.0) && near(y, 0.0));

  whelk_wallpaper_fit(WHELK_WALLPAPER_FIT_COVER, 100.0, 100.0, 100.0, 100.0,
                      1.1, &sx, &sy, &x, &y);
  check("zoom magnifies around the centre", near(sx, 1.1) && near(x, -5.0));

  whelk_wallpaper_fit(WHELK_WALLPAPER_FIT_COVER, 0.0, 100.0, 100.0, 100.0,
                      1.0, &sx, &sy, &x, &y);
  check("a zero-sized view is not a division by zero",
        near(sx, 1.0) && near(x, 0.0));
}

static void test_rotation(void) {
  check("no collection has no next", whelk_wallpaper_next_index(0, 0, FALSE,
                                                               NULL) == 0);
  check("one picture cycles to itself",
        whelk_wallpaper_next_index(1, 0, FALSE, NULL) == 0);
  check("sequential order wraps",
        whelk_wallpaper_next_index(3, 2, FALSE, NULL) == 0);

  guint64 state = 12345;
  guint previous = 0;

  for (int i = 0; i < 40; i++) {
    const guint next =
        whelk_wallpaper_next_index(5, previous, TRUE, &state);

    check("a shuffled next is in range", next < 5);
    check("a shuffled next is not the current picture", next != previous);
    previous = next;
  }

  guint64 a = 99;
  guint64 b = 99;

  check("the shuffle state is reproducible",
        whelk_wallpaper_next_index(7, 0, TRUE, &a) ==
            whelk_wallpaper_next_index(7, 0, TRUE, &b));
}

int main(void) {
  test_kinds();
  test_names();
  test_transitions();
  test_fit();
  test_rotation();

  if (failures > 0) {
    g_printerr("%d wallpaper test(s) failed\n", failures);
    return 1;
  }

  g_print("wallpaper: all checks passed\n");
  return 0;
}

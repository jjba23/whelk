// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The wallpaper policy crosses the bridge like the clipboard's and the night
// light's: Scheme holds the alist, C receives a struct it can outlive the call
// with.  This test pins the names and the conversion, and the default that
// leaves a compositor's own background alone.

#include "guile_bridge.h"
#include "wallpaper.h"

#include <glib.h>
#include <glib/gstdio.h>

static void test_default_is_off(void) {
  WhelkWallpaperConfig *config = whelk_bridge_wallpaper(NULL);

  g_assert_nonnull(config);
  g_assert_false(config->configured);
  g_assert_false(config->enabled);
  g_assert_cmpuint(config->paths->len, ==, 0);
  g_assert_cmpint(config->interval_seconds, ==, 900);
  g_assert_true(config->shuffle);
  g_assert_cmpint(config->animation, ==,
                  WHELK_WALLPAPER_ANIMATION_CROSSFADE);
  g_assert_cmpint(config->fit, ==, WHELK_WALLPAPER_FIT_COVER);

  whelk_wallpaper_config_free(config);
}

static void test_bridge(void) {
  g_autofree char *directory =
      g_dir_make_tmp("whelk-wallpaper-XXXXXX", NULL);
  g_assert_nonnull(directory);

  g_autofree char *path = g_build_filename(directory, "settings.scm", NULL);
  const char *settings =
      "(configure-shell!\n"
      " #:wallpaper\n"
      " (wallpaper-settings #:enabled? #t\n"
      "                     #:paths (list \"/wallpapers\" \"/more/walls\")\n"
      "                     #:interval 300\n"
      "                     #:shuffle? #f\n"
      "                     #:animation 'slide-left\n"
      "                     #:animation-duration 800\n"
      "                     #:fit 'contain\n"
      "                     #:ken-burns? #t\n"
      "                     #:video? #f\n"
      "                     #:mute-video? #f\n"
      "                     #:current \"/wallpapers/start.png\"\n"
      "                     #:picker-width 1200\n"
      "                     #:picker-height 700\n"
      "                     #:picker-columns 5))\n";

  g_assert_true(g_file_set_contents(path, settings, -1, NULL));
  g_assert_true(whelk_bridge_load(path));

  WhelkWallpaperConfig *config = whelk_bridge_wallpaper(path);

  g_assert_nonnull(config);
  g_assert_true(config->configured);
  g_assert_true(config->enabled);
  g_assert_cmpuint(config->paths->len, ==, 2);
  g_assert_cmpstr(g_ptr_array_index(config->paths, 0), ==, "/wallpapers");
  g_assert_cmpstr(g_ptr_array_index(config->paths, 1), ==, "/more/walls");
  g_assert_cmpint(config->interval_seconds, ==, 300);
  g_assert_false(config->shuffle);
  g_assert_cmpint(config->animation, ==,
                  WHELK_WALLPAPER_ANIMATION_SLIDE_LEFT);
  g_assert_cmpint(config->animation_ms, ==, 800);
  g_assert_cmpint(config->fit, ==, WHELK_WALLPAPER_FIT_CONTAIN);
  g_assert_true(config->ken_burns);
  g_assert_false(config->video);
  g_assert_false(config->mute_video);
  g_assert_cmpstr(config->current, ==, "/wallpapers/start.png");
  g_assert_cmpint(config->picker_width, ==, 1200);
  g_assert_cmpint(config->picker_height, ==, 700);
  g_assert_cmpint(config->picker_columns, ==, 5);

  WhelkWallpaperConfig *copy = whelk_wallpaper_config_copy(config);

  whelk_wallpaper_config_free(config);
  g_assert_cmpuint(copy->paths->len, ==, 2);
  g_assert_cmpstr(g_ptr_array_index(copy->paths, 0), ==, "/wallpapers");
  g_assert_cmpstr(copy->current, ==, "/wallpapers/start.png");
  whelk_wallpaper_config_free(copy);

  g_unlink(path);
  g_rmdir(directory);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  whelk_bridge_start_guile();
  whelk_bridge_setup_load_path();
  g_assert_true(whelk_bridge_use_core());

  g_test_add_func("/wallpaper/default", test_default_is_off);
  g_test_add_func("/wallpaper/bridge", test_bridge);

  return g_test_run();
}

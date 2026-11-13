// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/trayicon.h"

#include <string.h>

char *whelk_tray_icon_file(const char *theme_path, const char *icon_name) {
  if (!theme_path || !*theme_path || !g_path_is_absolute(theme_path))
    return NULL;

  if (!icon_name || !*icon_name || strchr(icon_name, '/') ||
      strchr(icon_name, '\\'))
    return NULL;

  static const char *const suffixes[] = {"", ".png", ".svg", ".xpm"};

  for (guint i = 0; i < G_N_ELEMENTS(suffixes); i++) {
    g_autofree char *candidate = g_strconcat(icon_name, suffixes[i], NULL);
    g_autofree char *path = g_build_filename(theme_path, candidate, NULL);

    if (g_file_test(path, G_FILE_TEST_IS_REGULAR))
      return g_strdup(path);
  }

  return NULL;
}

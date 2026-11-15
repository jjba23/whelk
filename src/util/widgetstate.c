// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/widgetstate.h"

#include <glib/gstdio.h>
#include <string.h>

#define POSITION_GROUP "positions"
#define STATE_GROUP "state"

static const char *state_home(void) {
  const char *xdg = g_getenv("XDG_STATE_HOME");

  if (xdg && *xdg)
    return xdg;

  return NULL;
}

char *whelk_widget_state_path(const char *config_path) {
  if (!config_path || *config_path == '\0')
    return NULL;

  const char *base = strrchr(config_path, '/');

  base = base ? base + 1 : config_path;

  g_autofree char *stem = g_strdup(base);

  if (g_str_has_suffix(stem, ".scm"))
    stem[strlen(stem) - 4] = '\0';

  const char *xdg = state_home();
  const char *home = g_get_home_dir();

  if (!xdg && (!home || *home == '\0'))
    return NULL;

  g_autofree char *directory =
      xdg ? g_build_filename(xdg, "whelk", NULL)
          : g_build_filename(home, ".local", "state", "whelk", NULL);

  g_autofree char *name = g_strdup_printf("widgets-%s.ini", stem);

  return g_build_filename(directory, name, NULL);
}

int whelk_widget_snap(int value, int grid) {
  if (value <= 0)
    return 0;

  if (grid <= 0)
    return value;

  return ((value + grid / 2) / grid) * grid;
}

void whelk_widget_clamp(int *x, int *y, int width, int height,
                        int area_width, int area_height) {
  if (!x || !y)
    return;

  const int max_x = MAX(0, area_width - width);
  const int max_y = MAX(0, area_height - height);

  *x = CLAMP(*x, 0, max_x);
  *y = CLAMP(*y, 0, max_y);
}

static GKeyFile *load_keyfile(const char *path, gboolean create) {
  GKeyFile *file = g_key_file_new();
  GError *error = NULL;

  if (!path || *path == '\0') {
    g_key_file_free(file);
    return NULL;
  }

  if (!g_key_file_load_from_file(file, path, G_KEY_FILE_NONE, &error)) {
    const gboolean missing = g_error_matches(error, G_FILE_ERROR,
                                             G_FILE_ERROR_NOENT);

    g_clear_error(&error);

    if (!create && !missing) {
      g_key_file_free(file);
      return NULL;
    }

    if (!missing) {
      g_key_file_free(file);
      return NULL;
    }
  }

  return file;
}

static gboolean save_keyfile(const char *path, GKeyFile *file) {
  gsize length = 0;
  g_autofree char *data = g_key_file_to_data(file, &length, NULL);

  if (!data)
    return FALSE;

  g_autofree char *directory = g_path_get_dirname(path);

  if (g_mkdir_with_parents(directory, 0700) != 0)
    return FALSE;

  return g_file_set_contents(path, data, (gssize)length, NULL);
}

/* A position is "x,y".  Reading it out of one key is the whole parser, so the
 * monitor-specific and fallback keys share it. */
static gboolean read_position(GKeyFile *file, const char *key, int *x, int *y) {
  g_autofree char *value =
      g_key_file_get_string(file, POSITION_GROUP, key, NULL);

  if (!value)
    return FALSE;

  int parsed_x = 0;
  int parsed_y = 0;

  if (sscanf(value, "%d,%d", &parsed_x, &parsed_y) != 2 || parsed_x < 0 ||
      parsed_y < 0)
    return FALSE;

  *x = parsed_x;
  *y = parsed_y;
  return TRUE;
}

/* The key a placement is stored under.  "default" is how a settings file says
 * "no monitor in particular", so it is the monitor-less key too. */
static char *position_key(const char *id, const char *monitor) {
  if (!monitor || *monitor == '\0' || g_strcmp0(monitor, "default") == 0)
    return g_strdup(id);

  return g_strdup_printf("%s@%s", id, monitor);
}

gboolean whelk_widget_state_load_position_for(const char *path, const char *id,
                                              const char *monitor, int *x,
                                              int *y) {
  if (!path || !id || !x || !y)
    return FALSE;

  GKeyFile *file = load_keyfile(path, FALSE);

  if (!file)
    return FALSE;

  g_autofree char *key = position_key(id, monitor);
  gboolean found = read_position(file, key, x, y);

  /* The monitor is gone, or this widget was placed before positions were kept
   * per monitor: the monitor-less key is the last place it was seen. */
  if (!found && g_strcmp0(key, id) != 0)
    found = read_position(file, id, x, y);

  g_key_file_free(file);
  return found;
}

gboolean whelk_widget_state_load_position(const char *path, const char *id,
                                          int *x, int *y) {
  return whelk_widget_state_load_position_for(path, id, NULL, x, y);
}

gboolean whelk_widget_state_save_position_for(const char *path, const char *id,
                                              const char *monitor, int x,
                                              int y) {
  if (!path || !id || x < 0 || y < 0)
    return FALSE;

  GKeyFile *file = load_keyfile(path, TRUE);

  if (!file)
    return FALSE;

  g_autofree char *value = g_strdup_printf("%d,%d", x, y);

  if (monitor && *monitor && g_strcmp0(monitor, "default") != 0) {
    g_autofree char *key = position_key(id, monitor);

    g_key_file_set_string(file, POSITION_GROUP, key, value);
  }

  /* Written even when a monitor was named: this is what an unplugged monitor
   * falls back to, and what a widget with no monitor of its own reads. */
  g_key_file_set_string(file, POSITION_GROUP, id, value);

  const gboolean saved = save_keyfile(path, file);

  g_key_file_free(file);
  return saved;
}

gboolean whelk_widget_state_save_position(const char *path, const char *id,
                                          int x, int y) {
  return whelk_widget_state_save_position_for(path, id, NULL, x, y);
}

static char *state_key(const char *id, const char *key) {
  return g_strdup_printf("%s.%s", id, key);
}

char *whelk_widget_state_get(const char *path, const char *id,
                             const char *key) {
  if (!path || !id || !key)
    return NULL;

  GKeyFile *file = load_keyfile(path, FALSE);

  if (!file)
    return NULL;

  g_autofree char *name = state_key(id, key);
  char *value = g_key_file_get_string(file, STATE_GROUP, name, NULL);

  g_key_file_free(file);
  return value;
}

gboolean whelk_widget_state_set(const char *path, const char *id,
                                const char *key, const char *value) {
  if (!path || !id || !key)
    return FALSE;

  GKeyFile *file = load_keyfile(path, TRUE);

  if (!file)
    return FALSE;

  g_autofree char *name = state_key(id, key);

  if (value)
    g_key_file_set_string(file, STATE_GROUP, name, value);
  else
    g_key_file_remove_key(file, STATE_GROUP, name, NULL);

  const gboolean saved = save_keyfile(path, file);

  g_key_file_free(file);
  return saved;
}

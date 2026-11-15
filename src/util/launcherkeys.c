// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/launcherkeys.h"

#include <string.h>

typedef struct {
  const char *mode;
  const char *prefix;
} KeyRow;

/* The built-in map, in one place: the launcher's table, the settings window's
 * defaults and the tests all read it here rather than spelling the characters
 * out a second time. */
static const KeyRow key_rows[] = {
    {"app", ""},
    {"window", "@"},
    {"run", ">"},
    {"calc", "="},
    {"file", "/"},
    {"web", "?"},
    {"ssh", "%"},
    {"quit", "!"},
    {"clipboard", ";"},
    {"menuitem", ":"},
};

#define KEY_ROW_COUNT G_N_ELEMENTS(key_rows)

struct _WhelkLauncherKeys {
  char *default_mode;
  char *prefixes[G_N_ELEMENTS(key_rows)];
};

static int row_for_mode(const char *mode) {
  if (!mode)
    return -1;

  for (gsize i = 0; i < KEY_ROW_COUNT; i++) {
    if (g_strcmp0(key_rows[i].mode, mode) == 0)
      return (int)i;
  }

  return -1;
}

gboolean whelk_launcher_keys_is_mode(const char *mode) {
  return row_for_mode(mode) >= 0;
}

guint whelk_launcher_keys_mode_count(void) { return KEY_ROW_COUNT; }

const char *whelk_launcher_keys_mode_at(guint index) {
  return index < KEY_ROW_COUNT ? key_rows[index].mode : NULL;
}

WhelkLauncherKeys *whelk_launcher_keys_new(void) {
  WhelkLauncherKeys *keys = g_new0(WhelkLauncherKeys, 1);

  keys->default_mode = g_strdup("app");

  for (gsize i = 0; i < KEY_ROW_COUNT; i++)
    keys->prefixes[i] = g_strdup(key_rows[i].prefix);

  return keys;
}

void whelk_launcher_keys_free(WhelkLauncherKeys *keys) {
  if (!keys)
    return;

  g_free(keys->default_mode);

  for (gsize i = 0; i < KEY_ROW_COUNT; i++)
    g_free(keys->prefixes[i]);

  g_free(keys);
}

void whelk_launcher_keys_set_default_mode(WhelkLauncherKeys *keys,
                                         const char *mode) {
  if (!keys || !whelk_launcher_keys_is_mode(mode))
    return;

  g_free(keys->default_mode);
  keys->default_mode = g_strdup(mode);
}

const char *whelk_launcher_keys_default_mode(const WhelkLauncherKeys *keys) {
  return (keys && keys->default_mode && *keys->default_mode)
             ? keys->default_mode
             : "app";
}

void whelk_launcher_keys_set_prefix(WhelkLauncherKeys *keys, const char *mode,
                                    const char *prefix) {
  const int row = row_for_mode(mode);

  if (!keys || row < 0)
    return;

  g_free(keys->prefixes[row]);
  keys->prefixes[row] = g_strdup(prefix ? prefix : "");
}

const char *whelk_launcher_keys_prefix(const WhelkLauncherKeys *keys,
                                       const char *mode) {
  const int row = row_for_mode(mode);

  if (row < 0)
    return "";

  return keys && keys->prefixes[row] ? keys->prefixes[row]
                                     : key_rows[row].prefix;
}

const char *whelk_launcher_keys_mode_for_text(const WhelkLauncherKeys *keys,
                                              const char *text,
                                              const char **query) {
  const char *best_mode = NULL;
  gsize best_length = 0;

  if (query)
    *query = text;

  if (!text)
    return NULL;

  for (gsize i = 0; i < KEY_ROW_COUNT; i++) {
    const char *prefix = whelk_launcher_keys_prefix(keys, key_rows[i].mode);
    const gsize length = prefix ? strlen(prefix) : 0;

    /* An empty prefix is a disabled one: `app' has no key of its own.  A
     * longer match wins, so `>>' can be a different mode from `>'. */
    if (length == 0 || length <= best_length || !g_str_has_prefix(text, prefix))
      continue;

    best_mode = key_rows[i].mode;
    best_length = length;
  }

  if (best_mode && query)
    *query = text + best_length;

  return best_mode;
}

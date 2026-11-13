// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/desktop.h"

#include <string.h>

/* Is this desktop id plausibly the entry for @app_id?  "org.alacritty.Alacritty"
 * contains "alacritty", and "firefox-esr" contains "firefox"; the check is what
 * keeps a search that matched an application's description from attaching
 * another application's actions to the focused window. */
static gboolean id_names_app(const char *desktop_id, const char *app_id) {
  if (!desktop_id || !app_id || *app_id == '\0')
    return FALSE;

  g_autofree char *lower_id = g_ascii_strdown(desktop_id, -1);
  g_autofree char *lower_app = g_ascii_strdown(app_id, -1);

  return strstr(lower_id, lower_app) != NULL;
}

char *whelk_desktop_short_name(const char *app_id, GDesktopAppInfo *info) {
  if (info) {
    const char *name = g_app_info_get_name(G_APP_INFO(info));

    if (name && *name)
      return g_strdup(name);
  }

  if (!app_id || *app_id == '\0')
    return g_strdup("Application");

  g_autofree char *copy = g_strdup(app_id);

  if (g_str_has_suffix(copy, ".desktop"))
    copy[strlen(copy) - strlen(".desktop")] = '\0';

  const char *last_dot = strrchr(copy, '.');
  const char *base = last_dot ? last_dot + 1 : copy;
  char *name = g_strdup(*base ? base : copy);
  gboolean start = TRUE;

  for (char *p = name; *p; p++) {
    if (*p == '-' || *p == '_') {
      *p = ' ';
      start = TRUE;
    } else if (start) {
      *p = (char)g_ascii_toupper(*p);
      start = FALSE;
    }
  }

  return name;
}

GDesktopAppInfo *whelk_desktop_for_app_id(const char *app_id) {
  if (!app_id || *app_id == '\0')
    return NULL;

  g_autofree char *with_suffix = g_strdup_printf("%s.desktop", app_id);
  GDesktopAppInfo *info = g_desktop_app_info_new(with_suffix);

  if (info)
    return info;

  info = g_desktop_app_info_new(app_id);

  if (info)
    return info;

  /* The desktop database's own search: by visible name, generic name and
   * keywords, which is how "Alacritty" reaches org.alacritty.Alacritty. */
  char ***groups = g_desktop_app_info_search(app_id);
  GDesktopAppInfo *found = NULL;

  if (groups) {
    for (guint group = 0; groups[group] && !found; group++)
      for (guint i = 0; groups[group][i] && !found; i++)
        if (id_names_app(groups[group][i], app_id))
          found = g_desktop_app_info_new(groups[group][i]);

    for (guint group = 0; groups[group]; group++)
      g_strfreev(groups[group]);

    g_free(groups);
  }

  return found;
}

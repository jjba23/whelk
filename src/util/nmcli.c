// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/nmcli.h"

#include <string.h>

GPtrArray *whelk_nmcli_split_terse(const char *line) {
  GPtrArray *fields = g_ptr_array_new_with_free_func(g_free);
  GString *current = g_string_new(NULL);

  if (!line)
    return fields;

  for (const char *p = line; *p; p++) {
    if (*p == '\\' && p[1] != '\0') {
      g_string_append_c(current, p[1]);
      p++;
      continue;
    }

    if (*p == ':') {
      g_ptr_array_add(fields, g_strdup(current->str));
      g_string_truncate(current, 0);
      continue;
    }

    g_string_append_c(current, *p);
  }

  g_ptr_array_add(fields, g_strdup(current->str));
  g_string_free(current, TRUE);

  return fields;
}

GPtrArray *whelk_nmcli_wifi_profile_names(const char *terse_output) {
  GPtrArray *names = g_ptr_array_new_with_free_func(g_free);

  if (!terse_output)
    return names;

  g_auto(GStrv) lines = g_strsplit(terse_output, "\n", -1);

  for (guint i = 0; lines[i]; i++) {
    g_autoptr(GPtrArray) fields = whelk_nmcli_split_terse(lines[i]);

    if (fields->len < 2)
      continue;

    const char *name = g_ptr_array_index(fields, 0);
    const char *type = g_ptr_array_index(fields, 1);

    if (*name != '\0' && g_strcmp0(type, "802-11-wireless") == 0)
      g_ptr_array_add(names, g_strdup(name));
  }

  return names;
}

const char *whelk_nmcli_wifi_format(gboolean show_ssid) {
  return show_ssid ? "{icon} {ssid}" : "{icon}";
}

const char *whelk_nmcli_wifi_tooltip(gboolean show_ssid) {
  return show_ssid ? "{ssid} · {signal}%\nClick: networks"
                   : "Wi-Fi · {signal}%\nClick: networks";
}

char *whelk_nmcli_key_update_script(const char *psk) {
  if (!psk || *psk == '\0')
    return NULL;

  /* The editor trims the value's edges, so a key that begins or ends in
   * whitespace cannot be written without changing it. */
  if (g_ascii_isspace(psk[0]) || g_ascii_isspace(psk[strlen(psk) - 1]))
    return NULL;

  /* A newline would end the `set` command and start another; a control
   * character cannot appear in a keyfile at all.  Refuse rather than truncate:
   * a saved key that is not the typed one is worse than a refusal. */
  for (const char *p = psk; *p; p++) {
    if ((guchar)*p < 0x20 || (guchar)*p == 0x7f)
      return NULL;
  }

  return g_strdup_printf("set 802-11-wireless-security.psk %s\nsave\nquit\n",
                         psk);
}

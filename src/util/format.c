// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/format.h"

#include <math.h>
#include <string.h>

char *whelk_str_subst(const char *text, const char *key, const char *value) {
  if (!text)
    return NULL;
  if (!key || !value || *key == '\0')
    return g_strdup(text);

  g_autofree char *needle = g_strdup_printf("{%s}", key);
  const size_t needle_len = strlen(needle);
  const char *cursor = text;
  GString *out = g_string_new(NULL);

  for (;;) {
    const char *hit = strstr(cursor, needle);

    if (!hit) {
      g_string_append(out, cursor);
      break;
    }

    g_string_append_len(out, cursor, (gssize)(hit - cursor));
    g_string_append(out, value);
    cursor = hit + needle_len;
  }

  return g_string_free(out, FALSE);
}

char *whelk_markup_escape(const char *text) {
  if (!text)
    return g_strdup("");

  return g_markup_escape_text(text, -1);
}

char *whelk_icon_markup(const char *icon, const char *font_family) {
  if (!icon || *icon == '\0')
    return g_strdup("");

  g_autofree char *escaped_icon = whelk_markup_escape(icon);

  if (!font_family || *font_family == '\0')
    return g_strdup(escaped_icon);

  g_autofree char *escaped_font = whelk_markup_escape(font_family);

  return g_strdup_printf("<span font_family=\"%s\">%s</span>", escaped_font,
                         escaped_icon);
}

char *whelk_str_truncate_utf8(const char *text, int max_chars) {
  if (!text)
    return g_strdup("");

  if (max_chars <= 0)
    return g_strdup(text);

  glong length = g_utf8_strlen(text, -1);
  if (length <= max_chars)
    return g_strdup(text);

  const char *cut = g_utf8_offset_to_pointer(text, max_chars);
  g_autofree char *head = g_strndup(text, (gsize)(cut - text));

  return g_strconcat(g_strchomp(head), "…", NULL);
}

char *whelk_str_first_line(const char *text) {
  if (!text)
    return g_strdup("");

  const char *newline = strchr(text, '\n');
  if (!newline)
    return whelk_str_strip_newline(g_strdup(text));

  return whelk_str_strip_newline(g_strndup(text, (gsize)(newline - text)));
}

char *whelk_str_strip_newline(char *text) {
  if (!text)
    return text;

  gsize len = strlen(text);

  while (len > 0 && (text[len - 1] == '\n' || text[len - 1] == '\r'))
    text[--len] = '\0';

  return text;
}

char *whelk_str_gib(guint64 kibibytes) {
  const double gib = (double)kibibytes / (1024.0 * 1024.0);

  if (isnan(gib))
    return g_strdup("0.0 GiB");

  return g_strdup_printf("%.1f GiB", gib);
}

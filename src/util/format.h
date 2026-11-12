// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_FORMAT_H
#define WHELK_FORMAT_H

#include <glib.h>

/* =========================================================================
 * TEXT HELPERS
 * =========================================================================
 * A module renders a configuration string such as "{icon} {usage}%" by
 * substituting named values.  Plain substitution, escaping and truncation live
 * here because every module needs exactly these three and no module needs SCM.
 */

/**
 * whelk_str_subst:
 * @text: the template.
 * @key: the name inside the braces, e.g. "usage".
 * @value: what to put there.
 *
 * Returns: (transfer full) (nullable): @text with every "{@key}" replaced, or
 * a copy of @text when it has none.
 */
char *whelk_str_subst(const char *text, const char *key, const char *value);

/**
 * whelk_markup_escape:
 * Returns: (transfer full): @text escaped for Pango markup ("&" -> "&amp;").
 */
char *whelk_markup_escape(const char *text);

/**
 * whelk_icon_markup:
 * @icon: the glyph, a private-use codepoint from the icon font.
 * @font_family: (nullable): the family that carries the glyph.
 *
 * Returns: (transfer full): @icon as a markup span naming @font_family, so the
 * glyph is drawn from the face the configuration meant instead of whichever
 * face fontconfig happens to answer at the bar's weight.
 */
char *whelk_icon_markup(const char *icon, const char *font_family);

/**
 * whelk_str_truncate_utf8:
 * @text: the text to shorten.
 * @max_chars: the most characters to keep; 0 or less means no limit.
 *
 * Returns: (transfer full): @text, shortened on a character boundary and ended
 * with an ellipsis when it had to be.
 */
char *whelk_str_truncate_utf8(const char *text, int max_chars);

/**
 * whelk_str_first_line:
 * Returns: (transfer full): the first line of @text without its line ending.
 * A tray or a CLI that answers in paragraphs becomes a bar reading.
 */
char *whelk_str_first_line(const char *text);

/**
 * whelk_str_strip_newline:
 * Removes one trailing "\n" or "\r\n" from @text in place.
 *
 * Returns: @text, for convenience.
 */
char *whelk_str_strip_newline(char *text);

/* Formats @bytes as a human-readable binary size, e.g. "9.4 GiB". */
char *whelk_str_gib(guint64 kibibytes);

#endif // WHELK_FORMAT_H

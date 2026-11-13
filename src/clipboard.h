// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CLIPBOARD_H
#define WHELK_CLIPBOARD_H

#include <glib-object.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE CLIPBOARD
 * =========================================================================
 * A Wayland client cannot read the clipboard while another window has focus,
 * so a clipboard manager is a listener, not a reader: `wl-paste --watch` runs a
 * program every time the selection changes and hands it the new contents.  That
 * program appends a NUL after each item, which is how this side tells one copy
 * from the next on a single pipe (text itself never contains a NUL).
 *
 * The history is newest first and de-duplicated: copying something already in
 * the list moves it to the front rather than showing it twice.  It lives in
 * memory only -- a shell is not the place to write every password somebody
 * copies to disk.
 */

#define WHELK_TYPE_CLIPBOARD (whelk_clipboard_get_type())
G_DECLARE_FINAL_TYPE(WhelkClipboard, whelk_clipboard, WHELK, CLIPBOARD,
                     GObject)

/**
 * whelk_clipboard_new:
 * @max_entries: how many copies to keep; 0 means 50.
 *
 * Returns: (transfer full): an empty, stopped clipboard.
 */
WhelkClipboard *whelk_clipboard_new(guint max_entries);

/**
 * whelk_clipboard_start:
 * @self: the clipboard.
 *
 * Starts listening with `wl-paste --watch`.  Missing wl-clipboard is not an
 * error: the history simply stays empty and the picker says so.
 */
void whelk_clipboard_start(WhelkClipboard *self);

/* The history, newest first.  Borrowed strings, owned by @self. */
GPtrArray *whelk_clipboard_entries(WhelkClipboard *self);

/* Puts @text back on the clipboard, and at the front of the history. */
void whelk_clipboard_copy(WhelkClipboard *self, const char *text);

void whelk_clipboard_clear(WhelkClipboard *self);

/**
 * whelk_clipboard_history_add:
 * @entries: a #GPtrArray of strings, newest first.
 * @text: (nullable): the copy to remember.
 * @max_entries: the cap; 0 means 50.
 *
 * The history rule, separated from the listener so it can be tested without a
 * compositor: empty copies are ignored, a repeat moves to the front, and the
 * oldest copy falls off the end.
 */
void whelk_clipboard_history_add(GPtrArray *entries, const char *text,
                                 guint max_entries);

/* Splits NUL-terminated items out of @pending into @out (strings, borrowed
 * pointers into a copy).  Exposed for the test; the listener uses it too. */
void whelk_clipboard_split(GString *pending, GPtrArray *out);

G_END_DECLS

#endif // WHELK_CLIPBOARD_H

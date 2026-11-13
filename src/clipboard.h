// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CLIPBOARD_H
#define WHELK_CLIPBOARD_H

#include <gio/gio.h>
#include <glib-object.h>

G_BEGIN_DECLS

/* =========================================================================
 * THE CLIPBOARD
 * =========================================================================
 * A Wayland client cannot read the clipboard while another window has focus,
 * so a clipboard manager is a listener, not a reader: `wl-paste --watch` runs a
 * program every time the selection changes.  This side runs a small helper
 * (whelk --clipboard-store) on every change; the helper reads the advertised
 * MIME types, refuses anything a password manager marked secret, fetches the
 * richest usable representation -- an image, rich text or plain text -- and
 * writes one length-prefixed record to its standard output.
 *
 * An entry keeps the bytes and the MIME type, not just a line of text, so an
 * image or HTML fragment can be put back as what it was.  The history is newest
 * first; pinned entries are exempt from the cap; a repeat moves to the front.
 *
 * Persistence is opt-in: nothing is written to disk unless a path is set with
 * whelk_clipboard_set_persistence(), which the shell does when
 * WHELK_CLIPBOARD_PERSIST is set.
 */

#define WHELK_TYPE_CLIPBOARD (whelk_clipboard_get_type())
G_DECLARE_FINAL_TYPE(WhelkClipboard, whelk_clipboard, WHELK, CLIPBOARD,
                     GObject)

/* One copy.  text is what the picker shows and searches, mime is the type the
 * bytes are restored as, data holds them. */
typedef struct {
  char *mime;
  char *text;
  GBytes *data;
  gboolean pinned;
} WhelkClipboardEntry;

WhelkClipboardEntry *whelk_clipboard_entry_new(const char *mime,
                                               const char *text, GBytes *data);
void whelk_clipboard_entry_free(WhelkClipboardEntry *entry);

/**
 * whelk_clipboard_new:
 * @max_entries: how many unpinned copies to keep; 0 means 50.
 *
 * Returns: (transfer full): an empty, stopped clipboard.
 */
WhelkClipboard *whelk_clipboard_new(guint max_entries);

/**
 * whelk_clipboard_start:
 * @self: the clipboard.
 *
 * Starts listening with `wl-paste --watch` and whelk's helper.  Missing
 * wl-clipboard is not an error: the history simply stays empty and the picker
 * says so.
 */
void whelk_clipboard_start(WhelkClipboard *self);

/* The entries, newest first; borrowed, owned by @self. */
GPtrArray *whelk_clipboard_entries(WhelkClipboard *self);

guint whelk_clipboard_count(WhelkClipboard *self);

/* Adds a copy at the front.  The bytes are copied. */
void whelk_clipboard_add(WhelkClipboard *self, const char *mime,
                         const char *text, GBytes *data);

/* Puts an entry back on the clipboard as its own MIME type. */
gboolean whelk_clipboard_restore(WhelkClipboard *self, guint index);

/* Text convenience: restores and records a UTF-8 copy. */
void whelk_clipboard_copy(WhelkClipboard *self, const char *text);

/* Pinning keeps an entry when the cap trims the rest; deleting is final. */
gboolean whelk_clipboard_pin(WhelkClipboard *self, guint index, gboolean pinned);
gboolean whelk_clipboard_remove(WhelkClipboard *self, guint index);

void whelk_clipboard_clear(WhelkClipboard *self);

/**
 * whelk_clipboard_set_persistence:
 * @self: the clipboard.
 * @path: (nullable): where the history lives, or NULL for memory only.
 *
 * Reads @path when it exists and writes it after every change.  The file is
 * private (0600) and opt-in; without this call nothing touches the disk.
 *
 * Returns: TRUE when an existing history was read.
 */
gboolean whelk_clipboard_set_persistence(WhelkClipboard *self,
                                         const char *path);

/* =========================================================================
 * THE HISTORY RULE, WITHOUT A COMPOSITOR
 * =========================================================================
 * These are the parts that decide what is kept, what is restored and what is
 * refused; the listener and the helper are thin wrappers over them, and the
 * tests exercise them directly.
 */

GPtrArray *whelk_clipboard_history_new(void);

/* Takes ownership of @entry; duplicates move up and keep their pin. */
void whelk_clipboard_history_add(GPtrArray *entries, WhelkClipboardEntry *entry,
                                 guint max_entries);

/* TRUE when @types carries a password manager's secret hint. */
gboolean whelk_clipboard_types_secret(const char *types);

/* The best advertised type, preferring images, then rich text, then plain.
 * With @text_only, only a text type is returned.  NULL when none is usable. */
char *whelk_clipboard_pick_mime(const char *types, gboolean text_only);

/* Consumes complete WHELK1 records from @pending into @entries.  Returns the
 * number of records read; incomplete bytes stay in @pending for the next read.
 * NUL bytes in image data are ordinary payload, not separators. */
guint whelk_clipboard_parse(GByteArray *pending, GPtrArray *entries,
                            guint max_entries);

/* The helper `wl-paste --watch` runs: reads its stdin, fetches the selection's
 * richest representation and writes one record to stdout.  Compositor I/O is
 * here; the parsing above is not. */
int whelk_clipboard_helper(void);

G_END_DECLS

#endif // WHELK_CLIPBOARD_H

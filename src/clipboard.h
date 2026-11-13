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
 * image, an HTML fragment or a file list can be put back as what it was.  It
 * also keeps a stable content id and the time it was copied: the picker uses
 * the id to keep its selection across a refresh, and the age to enforce an
 * optional retention period.
 *
 * The history is newest first; pinned entries are exempt from the count cap
 * and from the time-to-live; a repeat moves to the front.  Persistence is
 * opt-in and can keep the whole history or only the pinned entries; it never
 * touches the disk unless a path is set.
 */

#define WHELK_TYPE_CLIPBOARD (whelk_clipboard_get_type())
G_DECLARE_FINAL_TYPE(WhelkClipboard, whelk_clipboard, WHELK, CLIPBOARD,
                     GObject)

#define WHELK_CLIPBOARD_DEFAULT_MAX 50
#define WHELK_CLIPBOARD_DEFAULT_MAX_ENTRY (32u * 1024u * 1024u)
#define WHELK_CLIPBOARD_DEFAULT_MAX_TOTAL (128u * 1024u * 1024u)
#define WHELK_CLIPBOARD_TEXT_MAX 8192

/* One copy.  id is a content hash, text is what the picker shows and the
 * plain-text fallback rich content offers, mime is the type the bytes are
 * restored as, data holds them.  created_us is wall-clock microseconds. */
typedef struct {
  char *id;
  char *mime;
  char *text;
  GBytes *data;
  gboolean pinned;
  gint64 created_us;
} WhelkClipboardEntry;

WhelkClipboardEntry *whelk_clipboard_entry_new(const char *mime,
                                               const char *text, GBytes *data);
void whelk_clipboard_entry_free(WhelkClipboardEntry *entry);

/* The entry's stable id, borrowed from the entry.  It is never NULL for an
 * entry built by this module. */
const char *whelk_clipboard_entry_id(const WhelkClipboardEntry *entry);

/* TRUE for the plain-text MIME family (text/plain, UTF8_STRING, STRING,
 * TEXT); the picker and the history use it to compare renderings rather than
 * spellings. */
gboolean whelk_clipboard_mime_is_plain_text(const char *mime);

/**
 * whelk_clipboard_new:
 * @max_entries: how many unpinned copies to keep; 0 means 50.
 *
 * Returns: (transfer full): an empty, stopped clipboard with the default
 * memory limits.
 */
WhelkClipboard *whelk_clipboard_new(guint max_entries);

/* Starts or stops listening with `wl-paste --watch`.  Starting is idempotent.
 * A missing wl-clipboard is not an error: the history stays empty and
 * whelk_clipboard_is_available() answers FALSE. */
void whelk_clipboard_start(WhelkClipboard *self);
void whelk_clipboard_stop(WhelkClipboard *self);
void whelk_clipboard_restart(WhelkClipboard *self);

/* Pausing stops the watcher instead of merely ignoring its records, so a
 * paused shell does not keep fetching copies.  The history is preserved. */
void whelk_clipboard_set_paused(WhelkClipboard *self, gboolean paused);
gboolean whelk_clipboard_is_paused(WhelkClipboard *self);

/**
 * whelk_clipboard_set_clear_on_lock:
 * @self: the history.
 * @enabled: whether to watch the session's lock state.
 *
 * When enabled, logind's LockedHint clears unpinned entries.  Pinned copies
 * are the deliberate vault and survive.  A session without logind simply
 * never receives the signal; capture is unaffected.
 */
void whelk_clipboard_set_clear_on_lock(WhelkClipboard *self, gboolean enabled);
gboolean whelk_clipboard_clear_on_lock(WhelkClipboard *self);
gboolean whelk_clipboard_is_available(WhelkClipboard *self);

/* Count and byte budgets.  @max_entries 0 keeps the fifty-entry default;
 * @max_entry_bytes 0 keeps 32 MiB; @max_total_bytes 0 keeps 128 MiB. */
void whelk_clipboard_set_limits(WhelkClipboard *self, guint max_entries,
                                guint max_entry_bytes, guint max_total_bytes);
guint whelk_clipboard_max_entries(WhelkClipboard *self);
guint whelk_clipboard_max_entry_bytes(WhelkClipboard *self);
guint whelk_clipboard_max_total_bytes(WhelkClipboard *self);
gsize whelk_clipboard_bytes(WhelkClipboard *self);

/* Unpinned entries older than @seconds are dropped; 0 disables retention.
 * Pinned entries never expire. */
void whelk_clipboard_set_ttl(WhelkClipboard *self, guint seconds);
guint whelk_clipboard_ttl(WhelkClipboard *self);
guint whelk_clipboard_prune(WhelkClipboard *self);

/* The entries, newest first; borrowed, owned by @self. */
GPtrArray *whelk_clipboard_entries(WhelkClipboard *self);

guint whelk_clipboard_count(WhelkClipboard *self);

/* Adds a copy at the front.  The bytes are copied.  An entry over the
 * per-entry budget is refused and logged rather than silently eating the
 * shell's memory. */
void whelk_clipboard_add(WhelkClipboard *self, const char *mime,
                         const char *text, GBytes *data);

/* Puts an entry back on the clipboard as its own MIME type.  The GTK
 * clipboard is used when a display is available, so rich entries can offer
 * their plain-text fallback as well; wl-copy remains the headless fallback. */
gboolean whelk_clipboard_restore(WhelkClipboard *self, guint index);
gboolean whelk_clipboard_restore_id(WhelkClipboard *self, const char *id);

/* Text convenience: restores and records a UTF-8 copy. */
void whelk_clipboard_copy(WhelkClipboard *self, const char *text);

/* Find an entry by its stable id; borrowed, NULL when it is gone. */
WhelkClipboardEntry *whelk_clipboard_find(WhelkClipboard *self, const char *id);
gint whelk_clipboard_index_of(WhelkClipboard *self, const char *id);

/* Pinning keeps an entry when the cap trims the rest and exempts it from the
 * retention timer; deleting is final. */
gboolean whelk_clipboard_pin(WhelkClipboard *self, guint index, gboolean pinned);
gboolean whelk_clipboard_pin_id(WhelkClipboard *self, const char *id,
                                gboolean pinned);
gboolean whelk_clipboard_remove(WhelkClipboard *self, guint index);
gboolean whelk_clipboard_remove_id(WhelkClipboard *self, const char *id);

/* The most recent deletion can be put back once; a new deletion replaces the
 * tombstone.  Clear-all is not undoable. */
gboolean whelk_clipboard_undo_remove(WhelkClipboard *self);

void whelk_clipboard_clear(WhelkClipboard *self);
gboolean whelk_clipboard_clear_unpinned(WhelkClipboard *self);

/**
 * whelk_clipboard_set_persistence:
 * @self: the clipboard.
 * @path: (nullable): where the history lives, or NULL for memory only.
 *
 * Reads @path when it exists and schedules a write after every change.  The
 * file is private (0600) and opt-in; without this call nothing touches the
 * disk.  Writes happen off the main loop, with one write in flight at a time.
 *
 * Returns: TRUE when an existing history was read.
 */
gboolean whelk_clipboard_set_persistence(WhelkClipboard *self,
                                         const char *path);

/* When TRUE, the persisted file keeps pinned entries only.  The in-memory
 * history still keeps everything; this is the privacy switch that turns a
 * transcript into a deliberate vault. */
void whelk_clipboard_set_persist_pinned_only(WhelkClipboard *self,
                                             gboolean pinned_only);
gboolean whelk_clipboard_persist_pinned_only(WhelkClipboard *self);

/* Writes the current history synchronously.  For shutdown and tests; the
 * normal path is debounced and asynchronous. */
void whelk_clipboard_flush(WhelkClipboard *self);

const char *whelk_clipboard_persist_path(WhelkClipboard *self);

/* A short, user-facing reason the picker can show when there is nothing to
 * list: available, paused, or the listener stopped. */
const char *whelk_clipboard_status(WhelkClipboard *self);

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

/* Enforces the count and total-byte budgets.  The oldest unpinned entries go
 * first; pinned entries are never evicted. */
void whelk_clipboard_history_trim(GPtrArray *entries, guint max_entries,
                                  guint max_total_bytes);

/* Drops unpinned entries older than @ttl_seconds relative to @now_us.
 * Returns how many were removed. */
guint whelk_clipboard_history_prune(GPtrArray *entries, guint ttl_seconds,
                                    gint64 now_us);

/* Total payload bytes in @entries, for the budget and for tests. */
gsize whelk_clipboard_history_bytes(GPtrArray *entries);

/* TRUE when @types carries a password manager's secret hint. */
gboolean whelk_clipboard_types_secret(const char *types);

/* The best advertised type, preferring images, then file lists, then rich
 * text, then plain.  With @text_only, only a text type is returned.  NULL
 * when none is usable. */
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

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <errno.h>
#include <gdk/gdk.h>
#include <gdk-pixbuf/gdk-pixbuf.h>
#include <gio/gio.h>
#include <gio/gunixinputstream.h>
#include <glib/gstdio.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "logger.h"

/* The helper wl-paste runs on every change.  It is this same binary, so a
 * development build and an installed one both use the code they shipped with
 * instead of whatever `whelk` happens to be first on PATH. */
#define WHELK_CLIPBOARD_STORE_FLAG "--clipboard-store"
#define WHELK_CLIPBOARD_READ_SIZE 8192
#define WHELK_CLIPBOARD_DATA_MAX (64u * 1024u * 1024u)
#define WHELK_CLIPBOARD_HEADER_MAX 512
#define WHELK_CLIPBOARD_HELPER_LIMIT (32u * 1024u * 1024u)
#define WHELK_CLIPBOARD_SAVE_DELAY_MS 180
#define WHELK_CLIPBOARD_PRUNE_SECONDS 60
#define WHELK_CLIPBOARD_FILE_MAX (512u * 1024u * 1024u)

/* v2 adds the stable id and the copy time to every entry.  v1 files are still
 * read, so upgrading a machine does not throw its history away. */
#define WHELK_CLIPBOARD_MAGIC_V1 "whelk-clipboard-1"
#define WHELK_CLIPBOARD_MAGIC_V2 "whelk-clipboard-2"
#define WHELK_CLIPBOARD_RECORD "WHELK1"

typedef struct {
  WhelkClipboard *self; /* a strong reference: the read outlives a dispose */
  guint8 *buffer;
  guint generation;
} ReadContext;

typedef struct {
  char *path;
  GBytes *data;
} SaveJob;

struct _WhelkClipboard {
  GObject parent_instance;

  guint max_entries;
  guint max_entry_bytes;
  guint max_total_bytes;
  GPtrArray *entries; /* WhelkClipboardEntry *, newest first */
  GByteArray *pending;

  GSubprocess *watch;
  GInputStream *stream;
  guint read_generation;
  gboolean paused;
  gboolean available;

  guint ttl_seconds;
  guint prune_timeout;

  gboolean clear_on_lock;
  gboolean lock_watch_pending;
  GDBusConnection *logind;
  guint lock_sub;
  char *lock_session_path;

  char *persist_path;
  gboolean persist_loaded;
  gboolean persist_pinned_only;
  guint save_timeout;
  gboolean save_in_flight;
  gboolean save_dirty;

  WhelkClipboardEntry *last_removed;
  guint last_removed_index;
};

G_DEFINE_TYPE(WhelkClipboard, whelk_clipboard, G_TYPE_OBJECT)

enum { SIGNAL_CHANGED, N_SIGNALS };

static guint signals[N_SIGNALS];

/* =========================================================================
 * ENTRIES AND IDS
 * ========================================================================= */

gboolean whelk_clipboard_mime_is_plain_text(const char *mime) {
  return mime && (g_str_has_prefix(mime, "text/plain") ||
                  g_strcmp0(mime, "UTF8_STRING") == 0 ||
                  g_strcmp0(mime, "STRING") == 0 ||
                  g_strcmp0(mime, "TEXT") == 0);
}

/* The id is the identity, not a database key: text in any of the plain-text
 * spellings hashes the same, so a copy arriving as text/plain and then as
 * text/plain;charset=utf-8 does not become two rows. */
static char *entry_id_for(const char *mime, const char *text, GBytes *data) {
  GChecksum *checksum = g_checksum_new(G_CHECKSUM_SHA1);

  if (whelk_clipboard_mime_is_plain_text(mime) && text && *text) {
    g_checksum_update(checksum, (const guchar *)"text:", 5);
    g_checksum_update(checksum, (const guchar *)text, strlen(text));
  } else {
    gsize size = 0;
    const guint8 *bytes = data ? g_bytes_get_data(data, &size) : NULL;

    g_checksum_update(checksum, (const guchar *)mime, strlen(mime));
    g_checksum_update(checksum, (const guchar *)"\0", 1);

    if (bytes && size > 0)
      g_checksum_update(checksum, bytes, (gssize)size);
  }

  char *id = g_strdup(g_checksum_get_string(checksum));

  g_checksum_free(checksum);

  return id;
}

static WhelkClipboardEntry *entry_new_with_id(const char *id,
                                              gint64 created_us,
                                              const char *mime,
                                              const char *text,
                                              GBytes *data) {
  WhelkClipboardEntry *entry = g_new0(WhelkClipboardEntry, 1);

  entry->mime = g_strdup(mime ? mime : "text/plain;charset=utf-8");
  entry->text = g_strdup(text ? text : "");
  entry->data = data;
  entry->created_us = created_us > 0 ? created_us : g_get_real_time();
  entry->id = (id && *id) ? g_strdup(id)
                          : entry_id_for(entry->mime, entry->text, entry->data);

  return entry;
}

WhelkClipboardEntry *whelk_clipboard_entry_new(const char *mime,
                                               const char *text, GBytes *data) {
  return entry_new_with_id(NULL, 0, mime, text, data);
}

void whelk_clipboard_entry_free(WhelkClipboardEntry *entry) {
  if (!entry)
    return;

  g_free(entry->id);
  g_free(entry->mime);
  g_free(entry->text);
  g_clear_pointer(&entry->data, g_bytes_unref);
  g_free(entry);
}

const char *whelk_clipboard_entry_id(const WhelkClipboardEntry *entry) {
  return entry ? entry->id : NULL;
}

static gboolean entries_equal(WhelkClipboardEntry *a, WhelkClipboardEntry *b) {
  return a && b && g_strcmp0(a->id, b->id) == 0;
}

GPtrArray *whelk_clipboard_history_new(void) {
  return g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_clipboard_entry_free);
}

/* =========================================================================
 * THE HISTORY RULE
 * =========================================================================
 * The count cap counts unpinned entries only: pinning one is a promise that
 * the fiftieth copy after it will not push it off the end.  The byte budget
 * makes the same promise for memory, and evicts the oldest unpinned entry
 * first.  A duplicate moves the existing entry to the front, keeps its pin,
 * and takes the new copy's timestamp.
 */

static gsize entry_bytes(WhelkClipboardEntry *entry) {
  return entry ? g_bytes_get_size(entry->data) : 0;
}

gsize whelk_clipboard_history_bytes(GPtrArray *entries) {
  gsize total = 0;

  for (guint i = 0; entries && i < entries->len; i++)
    total += entry_bytes(g_ptr_array_index(entries, i));

  return total;
}

static void trim_count(GPtrArray *entries, guint max_entries) {
  const guint cap =
      max_entries > 0 ? max_entries : WHELK_CLIPBOARD_DEFAULT_MAX;
  guint unpinned = 0;

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i);

    if (!entry->pinned)
      unpinned++;
  }

  for (guint i = entries->len; unpinned > cap && i > 0; i--) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i - 1);

    if (entry->pinned)
      continue;

    g_ptr_array_remove_index(entries, i - 1);
    unpinned--;
  }
}

static void trim_total(GPtrArray *entries, guint max_total_bytes) {
  const guint cap = max_total_bytes > 0 ? max_total_bytes
                                        : WHELK_CLIPBOARD_DEFAULT_MAX_TOTAL;
  gsize total = whelk_clipboard_history_bytes(entries);

  for (guint i = entries->len; total > cap && i > 0; i--) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i - 1);

    if (entry->pinned)
      continue;

    total -= entry_bytes(entry);
    g_ptr_array_remove_index(entries, i - 1);
  }
}

void whelk_clipboard_history_trim(GPtrArray *entries, guint max_entries,
                                  guint max_total_bytes) {
  if (!entries)
    return;

  trim_count(entries, max_entries);
  trim_total(entries, max_total_bytes);
}

void whelk_clipboard_history_add(GPtrArray *entries, WhelkClipboardEntry *entry,
                                 guint max_entries) {
  if (!entries || !entry)
    return;

  if (!entry->id)
    entry->id = entry_id_for(entry->mime, entry->text, entry->data);

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *existing = g_ptr_array_index(entries, i);

    if (!entries_equal(existing, entry))
      continue;

    WhelkClipboardEntry *kept = g_ptr_array_steal_index(entries, i);

    kept->created_us = entry->created_us;
    whelk_clipboard_entry_free(entry);
    g_ptr_array_insert(entries, 0, kept);
    trim_count(entries, max_entries);
    return;
  }

  g_ptr_array_insert(entries, 0, entry);
  trim_count(entries, max_entries);
}

guint whelk_clipboard_history_prune(GPtrArray *entries, guint ttl_seconds,
                                    gint64 now_us) {
  if (!entries || ttl_seconds == 0)
    return 0;

  const gint64 cutoff = now_us - (gint64)ttl_seconds * G_TIME_SPAN_SECOND;
  guint removed = 0;

  for (guint i = entries->len; i > 0; i--) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i - 1);

    if (entry->pinned || entry->created_us <= 0 ||
        entry->created_us > cutoff)
      continue;

    g_ptr_array_remove_index(entries, i - 1);
    removed++;
  }

  return removed;
}

/* =========================================================================
 * TYPES AND THE SECRET HINT
 * =========================================================================
 * A MIME list is one type per line.  A password manager marks its selection
 * with x-kde-passwordManagerHint (the value says "secret"); the type's presence
 * is the signal, and the value is not ours to read.
 */

static gboolean type_line_is(const char *line, const char *wanted) {
  g_autofree char *trimmed = g_strdup(line);

  g_strstrip(trimmed);

  return g_ascii_strcasecmp(trimmed, wanted) == 0;
}

static gboolean types_have(const char *types, const char *wanted) {
  if (!types || !wanted)
    return FALSE;

  g_auto(GStrv) lines = g_strsplit(types, "\n", -1);

  for (guint i = 0; lines[i]; i++)
    if (type_line_is(lines[i], wanted))
      return TRUE;

  return FALSE;
}

gboolean whelk_clipboard_types_secret(const char *types) {
  if (!types)
    return FALSE;

  g_autofree char *lower = g_ascii_strdown(types, -1);

  return strstr(lower, "passwordmanagerhint") != NULL ||
         strstr(lower, "x-kde-password") != NULL;
}

static const char *const rich_types[] = {
    "image/png",      "image/jpeg", "image/webp", "text/uri-list",
    "text/html",      "text/rtf",   "text/plain;charset=utf-8",
    "text/plain",
};

static const char *const text_types[] = {
    "text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "STRING", "TEXT",
};

char *whelk_clipboard_pick_mime(const char *types, gboolean text_only) {
  if (!types)
    return NULL;

  const char *const *candidates = text_only ? text_types : rich_types;
  const guint count = text_only ? G_N_ELEMENTS(text_types)
                                : G_N_ELEMENTS(rich_types);

  for (guint i = 0; i < count; i++)
    if (types_have(types, candidates[i]))
      return g_strdup(candidates[i]);

  return NULL;
}

/* =========================================================================
 * THE RECORD
 * =========================================================================
 * `WHELK1 <mime length> <data length> <text length>\n' then the three payloads
 * back to back.  Lengths rather than separators: an image's bytes contain NUL
 * and every other byte, and a manager that split on them would truncate it.
 */

static gboolean record_header(const guint8 *data, gsize available,
                              gsize *header_length, gsize *mime_length,
                              gsize *data_length, gsize *text_length,
                              gboolean *incomplete) {
  const gsize magic_length = strlen(WHELK_CLIPBOARD_RECORD);
  const guint8 *newline = memchr(data, '\n', available);

  if (incomplete)
    *incomplete = FALSE;

  if (available >= magic_length &&
      memcmp(data, WHELK_CLIPBOARD_RECORD, magic_length) != 0)
    return FALSE;

  if (!newline) {
    if (incomplete && available < WHELK_CLIPBOARD_HEADER_MAX)
      *incomplete = TRUE;

    return FALSE;
  }

  const gsize line_length = (gsize)(newline - data);
  g_autofree char *line = g_strndup((const char *)data, line_length);
  g_auto(GStrv) parts = g_strsplit(line, " ", 4);

  if (g_strv_length(parts) != 4 ||
      g_strcmp0(parts[0], WHELK_CLIPBOARD_RECORD) != 0)
    return FALSE;

  guint64 mime = g_ascii_strtoull(parts[1], NULL, 10);
  guint64 bytes = g_ascii_strtoull(parts[2], NULL, 10);
  guint64 text = g_ascii_strtoull(parts[3], NULL, 10);

  if (mime == 0 || mime > 256 || bytes > WHELK_CLIPBOARD_DATA_MAX ||
      text > WHELK_CLIPBOARD_DATA_MAX)
    return FALSE;

  *header_length = line_length + 1;
  *mime_length = (gsize)mime;
  *data_length = (gsize)bytes;
  *text_length = (gsize)text;

  return TRUE;
}

guint whelk_clipboard_parse(GByteArray *pending, GPtrArray *entries,
                            guint max_entries) {
  if (!pending || !entries)
    return 0;

  guint records = 0;

  for (;;) {
    gsize header_length = 0;
    gsize mime_length = 0;
    gsize data_length = 0;
    gsize text_length = 0;
    gboolean incomplete = FALSE;

    if (pending->len < strlen(WHELK_CLIPBOARD_RECORD) + 1)
      break;

    if (!record_header(pending->data, pending->len, &header_length,
                       &mime_length, &data_length, &text_length,
                       &incomplete)) {
      if (incomplete)
        break;

      LOG_C_WARN("Clipboard", "dropping a malformed clipboard record");
      g_byte_array_set_size(pending, 0);
      break;
    }

    const gsize total = header_length + mime_length + data_length + text_length;

    if (pending->len < total)
      break;

    const guint8 *payload = pending->data + header_length;
    g_autofree char *mime = g_strndup((const char *)payload, mime_length);
    const guint8 *bytes = payload + mime_length;
    const guint8 *text_bytes = bytes + data_length;

    GBytes *data = g_bytes_new(bytes, data_length);
    g_autofree char *text = text_length > 0
                                ? g_strndup((const char *)text_bytes,
                                            MIN(text_length,
                                                WHELK_CLIPBOARD_TEXT_MAX))
                                : NULL;

    if ((!text || *text == '\0') && g_str_has_prefix(mime, "text/"))
      text = g_strndup((const char *)bytes,
                       MIN(data_length, WHELK_CLIPBOARD_TEXT_MAX));

    WhelkClipboardEntry *entry =
        whelk_clipboard_entry_new(mime, text ? text : "", data);

    whelk_clipboard_history_add(entries, entry, max_entries);
    g_byte_array_remove_range(pending, 0, total);
    records++;
  }

  return records;
}

/* =========================================================================
 * PERSISTENCE, OPT-IN AND ASYNCHRONOUS
 * ========================================================================= */

static GVariant *entries_to_variant(GPtrArray *entries, gboolean pinned_only) {
  GVariantBuilder builder;

  g_variant_builder_init(&builder, G_VARIANT_TYPE("a(sxssayb)"));

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i);

    if (pinned_only && !entry->pinned)
      continue;

    gsize size = 0;
    const guint8 *data = g_bytes_get_data(entry->data, &size);

    g_variant_builder_add(
        &builder, "(sxss@ayb)", entry->id, entry->created_us, entry->mime,
        entry->text, g_variant_new_fixed_array(G_VARIANT_TYPE_BYTE, data, size,
                                               1),
        entry->pinned);
  }

  return g_variant_new("(s@a(sxssayb))", WHELK_CLIPBOARD_MAGIC_V2,
                       g_variant_builder_end(&builder));
}

static GPtrArray *variant_to_entries(GVariant *root, guint max_entries,
                                     gboolean pinned_only, gboolean *loaded) {
  GPtrArray *entries = whelk_clipboard_history_new();
  g_autoptr(GVariant) magic = g_variant_get_child_value(root, 0);
  const char *name = g_variant_get_string(magic, NULL);
  const gboolean v2 = g_strcmp0(name, WHELK_CLIPBOARD_MAGIC_V2) == 0;
  const gboolean v1 = g_strcmp0(name, WHELK_CLIPBOARD_MAGIC_V1) == 0;

  *loaded = v1 || v2;

  if (!*loaded)
    return entries;

  g_autoptr(GVariant) array = g_variant_get_child_value(root, 1);
  const gsize count = g_variant_n_children(array);

  /* The file is newest first, and history_add prepends, so walk backwards;
   * otherwise a restart would reverse the history. */
  for (gsize i = count; i > 0; i--) {
    g_autoptr(GVariant) item = g_variant_get_child_value(array, i - 1);
    const char *id = NULL;
    const char *mime = NULL;
    const char *text = NULL;
    gint64 created = 0;
    GVariant *bytes = NULL;
    gboolean pinned = FALSE;

    if (v2)
      g_variant_get(item, "(sxss@ayb)", &id, &created, &mime, &text, &bytes,
                    &pinned);
    else
      g_variant_get(item, "(ss@ayb)", &mime, &text, &bytes, &pinned);

    if (pinned_only && !pinned) {
      g_variant_unref(bytes);
      continue;
    }

    GBytes *data = g_variant_get_data_as_bytes(bytes);

    g_variant_unref(bytes);

    WhelkClipboardEntry *entry =
        entry_new_with_id(id, created, mime, text, data);

    entry->pinned = pinned;
    whelk_clipboard_history_add(entries, entry, max_entries);
  }

  return entries;
}

static gboolean load_variant(WhelkClipboard *self, const char *contents,
                             gsize length, gboolean *loaded) {
  g_autoptr(GBytes) blob = g_bytes_new(contents, length);
  g_autoptr(GVariant) v2 = g_variant_new_from_bytes(
      G_VARIANT_TYPE("(sa(sxssayb))"), blob, FALSE);
  g_autoptr(GVariant) v1 = NULL;

  if (v2 && g_variant_is_of_type(v2, G_VARIANT_TYPE("(sa(sxssayb))")) &&
      g_variant_is_normal_form(v2)) {
    GPtrArray *entries =
        variant_to_entries(v2, self->max_entries,
                           self->persist_pinned_only, loaded);

    if (*loaded) {
      g_ptr_array_unref(self->entries);
      self->entries = entries;
      return TRUE;
    }
  }

  v1 = g_variant_new_from_bytes(G_VARIANT_TYPE("(sa(ssayb))"), blob, FALSE);

  if (v1 && g_variant_is_of_type(v1, G_VARIANT_TYPE("(sa(ssayb))")) &&
      g_variant_is_normal_form(v1)) {
    GPtrArray *entries =
        variant_to_entries(v1, self->max_entries,
                           self->persist_pinned_only, loaded);

    if (*loaded) {
      g_ptr_array_unref(self->entries);
      self->entries = entries;
      return TRUE;
    }
  }

  return FALSE;
}

static void save_job_free(SaveJob *job) {
  if (!job)
    return;

  g_free(job->path);
  g_clear_pointer(&job->data, g_bytes_unref);
  g_free(job);
}

static void save_task(GTask *task, gpointer source, gpointer task_data,
                      GCancellable *cancellable) {
  (void)source;
  (void)cancellable;

  SaveJob *job = task_data;
  gsize length = 0;
  const char *data = g_bytes_get_data(job->data, &length);
  g_autoptr(GError) error = NULL;

  if (!g_file_set_contents_full(job->path, data, (gssize)length,
                                G_FILE_SET_CONTENTS_CONSISTENT, 0600, &error)) {
    g_task_return_error(task, g_steal_pointer(&error));
    return;
  }

  if (g_chmod(job->path, 0600) != 0) {
    g_task_return_new_error(task, G_FILE_ERROR, G_FILE_ERROR_PERM,
                            "could not make the history private: %s",
                            g_strerror(errno));
    return;
  }

  g_task_return_boolean(task, TRUE);
}

static GBytes *serialize_entries(WhelkClipboard *self) {
  g_autoptr(GVariant) root =
      entries_to_variant(self->entries, self->persist_pinned_only);
  GBytes *bytes = g_variant_get_data_as_bytes(root);

  return bytes;
}

static void clipboard_save_sync(WhelkClipboard *self);

static void on_save_done(GObject *source, GAsyncResult *result,
                         gpointer user_data) {
  (void)source;

  WhelkClipboard *self = user_data;
  g_autoptr(GError) error = NULL;

  if (!g_task_propagate_boolean(G_TASK(result), &error) && error)
    LOG_C_WARN("Clipboard", "could not save the history: %s", error->message);

  self->save_in_flight = FALSE;

  if (self->save_dirty) {
    self->save_dirty = FALSE;
    clipboard_save_sync(self);
  }

  g_object_unref(self);
}

static void clipboard_save_async(WhelkClipboard *self) {
  if (!self->persist_path || !self->persist_loaded || self->save_in_flight)
    return;

  GBytes *data = serialize_entries(self);

  if (!data)
    return;

  SaveJob *job = g_new0(SaveJob, 1);

  job->path = g_strdup(self->persist_path);
  job->data = data;

  self->save_in_flight = TRUE;
  self->save_dirty = FALSE;

  GTask *task = g_task_new(NULL, NULL, on_save_done, g_object_ref(self));

  g_task_set_task_data(task, job, (GDestroyNotify)save_job_free);
  g_task_run_in_thread(task, save_task);
  g_object_unref(task);
}

static gboolean on_save_timeout(gpointer user_data) {
  WhelkClipboard *self = user_data;

  self->save_timeout = 0;

  if (self->save_in_flight)
    return G_SOURCE_REMOVE;

  clipboard_save_async(self);

  return G_SOURCE_REMOVE;
}

static void schedule_save(WhelkClipboard *self) {
  if (!self->persist_path || !self->persist_loaded)
    return;

  self->save_dirty = TRUE;

  if (self->save_in_flight || self->save_timeout != 0)
    return;

  self->save_timeout =
      g_timeout_add(WHELK_CLIPBOARD_SAVE_DELAY_MS, on_save_timeout, self);
}

static void clipboard_save_sync(WhelkClipboard *self) {
  if (!self->persist_path || !self->persist_loaded)
    return;

  if (self->save_timeout != 0) {
    g_source_remove(self->save_timeout);
    self->save_timeout = 0;
  }

  g_autofree char *directory = g_path_get_dirname(self->persist_path);

  g_mkdir_with_parents(directory, 0700);

  g_autoptr(GBytes) data = serialize_entries(self);
  gsize length = 0;
  const char *bytes = g_bytes_get_data(data, &length);
  g_autoptr(GError) error = NULL;

  if (!g_file_set_contents_full(self->persist_path, bytes, (gssize)length,
                                G_FILE_SET_CONTENTS_CONSISTENT, 0600, &error)) {
    LOG_C_WARN("Clipboard", "could not save the history: %s", error->message);
    return;
  }

  if (g_chmod(self->persist_path, 0600) != 0)
    LOG_C_WARN("Clipboard", "could not make the history private: %s",
               g_strerror(errno));

  self->save_dirty = FALSE;
}

gboolean whelk_clipboard_set_persistence(WhelkClipboard *self,
                                         const char *path) {
  if (!self)
    return FALSE;

  if (self->save_timeout != 0) {
    g_source_remove(self->save_timeout);
    self->save_timeout = 0;
  }

  g_free(self->persist_path);
  self->persist_path = g_strdup(path);
  self->persist_loaded = FALSE;

  if (!self->persist_path || !*self->persist_path) {
    g_clear_pointer(&self->persist_path, g_free);
    return FALSE;
  }

  /* From here on a save may write: the file may not exist yet, and a first
   * copy has to create it. */
  self->persist_loaded = TRUE;

  /* A history file larger than this is not one this shell wrote in ordinary
   * use.  Refuse it before allocating its bytes; the next copy replaces it. */
  GStatBuf stat_buf;

  if (g_stat(self->persist_path, &stat_buf) == 0 &&
      stat_buf.st_size > (goffset)WHELK_CLIPBOARD_FILE_MAX) {
    LOG_C_WARN("Clipboard", "history file is too large; it was not loaded");
    return FALSE;
  }

  g_autofree char *contents = NULL;
  gsize length = 0;

  if (!g_file_get_contents(self->persist_path, &contents, &length, NULL))
    return FALSE;

  gboolean loaded = FALSE;

  if (!load_variant(self, contents, length, &loaded) || !loaded)
    return FALSE;

  whelk_clipboard_history_trim(self->entries, self->max_entries,
                               self->max_total_bytes);
  whelk_clipboard_history_prune(self->entries, self->ttl_seconds,
                                g_get_real_time());

  return TRUE;
}

void whelk_clipboard_set_persist_pinned_only(WhelkClipboard *self,
                                             gboolean pinned_only) {
  if (!self)
    return;

  self->persist_pinned_only = pinned_only;

  if (self->persist_loaded)
    schedule_save(self);
}

gboolean whelk_clipboard_persist_pinned_only(WhelkClipboard *self) {
  return self ? self->persist_pinned_only : FALSE;
}

void whelk_clipboard_flush(WhelkClipboard *self) {
  if (!self)
    return;

  /* A write already on the worker may be older than the entries in memory.
   * Let it finish so the synchronous write is the last one. */
  while (self->save_in_flight)
    g_main_context_iteration(NULL, TRUE);

  clipboard_save_sync(self);
}

const char *whelk_clipboard_persist_path(WhelkClipboard *self) {
  return self ? self->persist_path : NULL;
}

/* =========================================================================
 * PUTTING ONE BACK
 * ========================================================================= */

static void on_communicate(GObject *source, GAsyncResult *result,
                           gpointer user_data) {
  (void)user_data;

  g_autoptr(GError) error = NULL;
  GBytes *out = NULL;
  GBytes *err = NULL;

  if (!g_subprocess_communicate_finish(G_SUBPROCESS(source), result, &out,
                                       &err, &error) &&
      error)
    LOG_C_WARN("Clipboard", "wl-copy failed: %s", error->message);

  g_clear_pointer(&out, g_bytes_unref);
  g_clear_pointer(&err, g_bytes_unref);
  g_object_unref(source);
}

static void write_clipboard_fallback(WhelkClipboardEntry *entry) {
  GSubprocessLauncher *launcher =
      g_subprocess_launcher_new(G_SUBPROCESS_FLAGS_STDIN_PIPE);
  const char *argv[] = {"wl-copy", "--type", entry->mime, NULL};
  g_autoptr(GError) error = NULL;
  GSubprocess *process = g_subprocess_launcher_spawnv(launcher, argv, &error);

  g_object_unref(launcher);

  if (!process) {
    LOG_C_WARN("Clipboard", "could not run wl-copy: %s", error->message);
    return;
  }

  g_subprocess_communicate_async(process, entry->data, NULL, on_communicate,
                                 NULL);
}

static gboolean add_provider(GPtrArray *providers, GHashTable *seen,
                             const char *mime, GBytes *bytes) {
  if (!mime || !*mime || !bytes || g_hash_table_contains(seen, mime))
    return FALSE;

  g_hash_table_add(seen, g_strdup(mime));
  g_ptr_array_add(providers,
                  gdk_content_provider_new_for_bytes(mime, bytes));
  return TRUE;
}

/* An app that asks for image/png should not get nothing because the copy was
 * a JPEG or a WebP.  Decode once and offer the same picture as PNG. */
static GBytes *image_png_fallback(WhelkClipboardEntry *entry) {
  if (!entry || !g_str_has_prefix(entry->mime, "image/") ||
      g_strcmp0(entry->mime, "image/png") == 0 || !entry->data)
    return NULL;

  g_autoptr(GInputStream) stream =
      g_memory_input_stream_new_from_bytes(entry->data);
  g_autoptr(GError) error = NULL;
  GdkPixbuf *pixbuf = gdk_pixbuf_new_from_stream(stream, NULL, &error);

  if (!pixbuf)
    return NULL;

  gchar *buffer = NULL;
  gsize buffer_size = 0;
  GBytes *png = NULL;

  if (gdk_pixbuf_save_to_buffer(pixbuf, &buffer, &buffer_size, "png", NULL,
                                NULL))
    png = g_bytes_new_take(buffer, buffer_size);

  g_object_unref(pixbuf);

  return png;
}

/* GTK owns the selection, so an HTML copy can offer its plain rendering to an
 * application that asks for text/plain.  wl-copy is a single-MIME process and
 * stays the fallback for a display-less run. */
static gboolean write_clipboard_gdk(WhelkClipboardEntry *entry) {
  GdkDisplay *display = gdk_display_get_default();

  if (!display)
    return FALSE;

  GdkClipboard *clipboard = gdk_display_get_clipboard(display);

  if (!clipboard)
    return FALSE;

  GPtrArray *providers = g_ptr_array_new_with_free_func(g_object_unref);
  GHashTable *seen = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                           NULL);
  GBytes *plain = NULL;
  GBytes *png = NULL;

  add_provider(providers, seen, entry->mime, entry->data);

  if (entry->text && *entry->text &&
      g_str_has_prefix(entry->mime, "text/") &&
      !whelk_clipboard_mime_is_plain_text(entry->mime)) {
    plain = g_bytes_new(entry->text, strlen(entry->text));
    add_provider(providers, seen, "text/plain;charset=utf-8", plain);
    add_provider(providers, seen, "text/plain", plain);
    add_provider(providers, seen, "UTF8_STRING", plain);
  } else if (whelk_clipboard_mime_is_plain_text(entry->mime)) {
    add_provider(providers, seen, "text/plain", entry->data);
    add_provider(providers, seen, "UTF8_STRING", entry->data);
  }

  if (g_str_has_prefix(entry->mime, "image/")) {
    png = image_png_fallback(entry);

    if (png)
      add_provider(providers, seen, "image/png", png);
  }

  gboolean written = FALSE;

  if (providers->len > 0) {
    const guint count = providers->len;
    GdkContentProvider *provider = gdk_content_provider_new_union(
        (GdkContentProvider **)providers->pdata, count);

    /* new_union() takes ownership of the providers it is given.  The array
     * still has g_object_unref as its free function, so detach it before the
     * unref below: otherwise each child is unref'd twice and GDK later walks
     * freed provider state while serving a clipboard request. */
    g_ptr_array_set_free_func(providers, NULL);

    written = gdk_clipboard_set_content(clipboard, provider);
    g_object_unref(provider);
  }

  g_clear_pointer(&plain, g_bytes_unref);
  g_clear_pointer(&png, g_bytes_unref);
  g_hash_table_unref(seen);
  g_ptr_array_unref(providers);

  return written;
}

static void write_clipboard(WhelkClipboardEntry *entry) {
  if (write_clipboard_gdk(entry))
    return;

  write_clipboard_fallback(entry);
}

static void move_to_front(WhelkClipboard *self, guint index) {
  if (index == 0 || index >= self->entries->len)
    return;

  WhelkClipboardEntry *entry = g_ptr_array_steal_index(self->entries, index);

  g_ptr_array_insert(self->entries, 0, entry);
}

/* =========================================================================
 * THE LISTENER
 * ========================================================================= */

static void clipboard_changed(WhelkClipboard *self) {
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
  schedule_save(self);
}

static void read_next(WhelkClipboard *self);

static void stop_watch(WhelkClipboard *self) {
  /* Invalidate reads already queued against the stream being torn down.  A
   * callback for the old wl-paste process must not stop the replacement
   * watcher when it eventually returns. */
  self->read_generation++;

  if (self->watch) {
    g_subprocess_force_exit(self->watch);
    g_clear_object(&self->watch);
  }

  g_clear_object(&self->stream);
  self->available = FALSE;

  if (self->pending)
    g_byte_array_set_size(self->pending, 0);
}

static void on_read(GObject *source, GAsyncResult *result, gpointer user_data) {
  (void)source;

  ReadContext *context = user_data;
  WhelkClipboard *self = context->self;
  g_autoptr(GError) error = NULL;

  const gssize n = g_input_stream_read_finish(G_INPUT_STREAM(source), result,
                                              &error);

  if (context->generation != self->read_generation) {
    g_free(context->buffer);
    g_object_unref(self);
    g_free(context);
    return;
  }

  if (n <= 0) {
    if (error)
      LOG_C_DEBUG("Clipboard", "wl-paste stopped: %s", error->message);

    stop_watch(self);
    g_signal_emit(self, signals[SIGNAL_CHANGED], 0);

    g_free(context->buffer);
    g_object_unref(self);
    g_free(context);
    return;
  }

  g_byte_array_append(self->pending, context->buffer, (guint)n);

  if (whelk_clipboard_parse(self->pending, self->entries,
                            self->max_entries) > 0) {
    whelk_clipboard_history_trim(self->entries, self->max_entries,
                                 self->max_total_bytes);
    clipboard_changed(self);
  }

  g_free(context->buffer);
  g_free(context);

  read_next(self);
  g_object_unref(self);
}

static void read_next(WhelkClipboard *self) {
  if (!self->stream)
    return;

  ReadContext *context = g_new0(ReadContext, 1);

  context->self = g_object_ref(self);
  context->buffer = g_malloc(WHELK_CLIPBOARD_READ_SIZE);
  context->generation = self->read_generation;

  g_input_stream_read_async(self->stream, context->buffer,
                            WHELK_CLIPBOARD_READ_SIZE, G_PRIORITY_DEFAULT,
                            NULL, on_read, context);
}

void whelk_clipboard_start(WhelkClipboard *self) {
  if (!self || self->watch || self->paused)
    return;

  g_autofree char *exe = g_file_read_link("/proc/self/exe", NULL);
  const char *binary = (exe && *exe) ? exe : "whelk";
  const char *argv[] = {"wl-paste", "--watch", binary,
                        WHELK_CLIPBOARD_STORE_FLAG, NULL};
  g_autoptr(GError) error = NULL;

  self->watch = g_subprocess_newv(
      argv,
      G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_SILENCE,
      &error);

  if (!self->watch) {
    self->available = FALSE;
    LOG_C_DEBUG("Clipboard",
                "wl-paste is not installed: the history stays empty (%s)",
                error->message);
    return;
  }

  self->stream = g_subprocess_get_stdout_pipe(self->watch);
  g_object_ref(self->stream);
  self->available = TRUE;

  read_next(self);
}

void whelk_clipboard_stop(WhelkClipboard *self) {
  if (!self)
    return;

  stop_watch(self);
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

void whelk_clipboard_restart(WhelkClipboard *self) {
  if (!self)
    return;

  stop_watch(self);

  if (!self->paused)
    whelk_clipboard_start(self);

  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

void whelk_clipboard_set_paused(WhelkClipboard *self, gboolean paused) {
  if (!self || self->paused == paused)
    return;

  self->paused = paused;

  if (paused)
    stop_watch(self);
  else
    whelk_clipboard_start(self);

  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

gboolean whelk_clipboard_is_paused(WhelkClipboard *self) {
  return self && self->paused;
}

/* logind publishes LockedHint on the session object.  The history keeps
 * listening while locked (a copy made by a lock screen or a remote session is
 * still a copy), but the moment the session locks, unpinned entries go: the
 * pinned ones are the user's deliberate vault. */
static void on_lock_properties(GDBusConnection *connection,
                               const char *sender_name,
                               const char *object_path,
                               const char *interface_name,
                               const char *signal_name, GVariant *parameters,
                               gpointer user_data) {
  (void)connection;
  (void)sender_name;
  (void)object_path;
  (void)interface_name;
  (void)signal_name;

  WhelkClipboard *self = user_data;
  g_autofree char *changed_interface = NULL;
  g_autoptr(GVariant) properties = NULL;
  g_autoptr(GVariant) invalidated = NULL;

  g_variant_get(parameters, "(s@a{sv}@as)", &changed_interface, &properties,
                &invalidated);

  g_autoptr(GVariant) locked = g_variant_lookup_value(
      properties, "LockedHint", G_VARIANT_TYPE_BOOLEAN);

  if (locked && g_variant_get_boolean(locked)) {
    if (whelk_clipboard_clear_unpinned(self))
      LOG_C_INFO("Clipboard", "session locked: unpinned clipboard history "
                               "cleared");
  }
}

static void on_session_path(GObject *source, GAsyncResult *result,
                            gpointer user_data) {
  WhelkClipboard *self = user_data;
  g_autoptr(GError) error = NULL;
  g_autoptr(GVariant) reply =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), result, &error);

  self->lock_watch_pending = FALSE;

  if (!self->clear_on_lock) {
    g_object_unref(self);
    return;
  }

  if (!reply) {
    LOG_C_DEBUG("Clipboard", "could not find the logind session: %s",
                error ? error->message : "no reply");
    g_object_unref(self);
    return;
  }

  const char *path = NULL;

  g_variant_get(reply, "(&o)", &path);

  if (!path || !*path) {
    g_object_unref(self);
    return;
  }

  g_free(self->lock_session_path);
  self->lock_session_path = g_strdup(path);
  self->lock_sub = g_dbus_connection_signal_subscribe(
      self->logind, "org.freedesktop.login1", "org.freedesktop.login1.Session",
      "PropertiesChanged", self->lock_session_path, NULL,
      G_DBUS_SIGNAL_FLAGS_NONE, on_lock_properties, self, NULL);

  g_object_unref(self);
}

static void on_system_bus(GObject *source, GAsyncResult *result,
                          gpointer user_data) {
  (void)source;

  WhelkClipboard *self = user_data;
  g_autoptr(GError) error = NULL;
  GDBusConnection *bus = g_bus_get_finish(result, &error);

  self->lock_watch_pending = FALSE;

  if (!self->clear_on_lock) {
    g_clear_object(&bus);
    g_object_unref(self);
    return;
  }

  if (!bus) {
    LOG_C_DEBUG("Clipboard", "no system bus for lock detection: %s",
                error ? error->message : "unknown error");
    g_object_unref(self);
    return;
  }

  g_clear_object(&self->logind);
  self->logind = g_object_ref(bus);
  g_object_unref(bus);

  g_dbus_connection_call(
      self->logind, "org.freedesktop.login1", "/org/freedesktop/login1",
      "org.freedesktop.login1.Manager", "GetSession",
      g_variant_new("(s)", "auto"), G_VARIANT_TYPE("(o)"),
      G_DBUS_CALL_FLAGS_NONE, 2000, NULL, on_session_path, self);
}

void whelk_clipboard_set_clear_on_lock(WhelkClipboard *self, gboolean enabled) {
  if (!self || self->clear_on_lock == enabled)
    return;

  self->clear_on_lock = enabled;

  if (enabled && !self->lock_watch_pending && self->lock_sub == 0) {
    self->lock_watch_pending = TRUE;
    g_bus_get(G_BUS_TYPE_SYSTEM, NULL, on_system_bus, g_object_ref(self));
  } else if (!enabled) {
    if (self->lock_sub != 0) {
      g_dbus_connection_signal_unsubscribe(self->logind, self->lock_sub);
      self->lock_sub = 0;
    }

    g_clear_object(&self->logind);
    g_clear_pointer(&self->lock_session_path, g_free);
  }
}

gboolean whelk_clipboard_clear_on_lock(WhelkClipboard *self) {
  return self && self->clear_on_lock;
}

gboolean whelk_clipboard_is_available(WhelkClipboard *self) {
  return self && self->available;
}

const char *whelk_clipboard_status(WhelkClipboard *self) {
  if (!self)
    return "Clipboard unavailable";

  if (self->paused)
    return "Clipboard history paused";

  if (!self->available)
    return "Clipboard listener stopped";

  return "Clipboard history";
}

/* =========================================================================
 * PUBLIC API
 * ========================================================================= */

GPtrArray *whelk_clipboard_entries(WhelkClipboard *self) {
  return self ? self->entries : NULL;
}

guint whelk_clipboard_count(WhelkClipboard *self) {
  return self ? self->entries->len : 0;
}

gsize whelk_clipboard_bytes(WhelkClipboard *self) {
  return self ? whelk_clipboard_history_bytes(self->entries) : 0;
}

void whelk_clipboard_set_limits(WhelkClipboard *self, guint max_entries,
                                guint max_entry_bytes, guint max_total_bytes) {
  if (!self)
    return;

  self->max_entries =
      max_entries > 0 ? max_entries : WHELK_CLIPBOARD_DEFAULT_MAX;
  self->max_entry_bytes = max_entry_bytes > 0
                              ? max_entry_bytes
                              : WHELK_CLIPBOARD_DEFAULT_MAX_ENTRY;
  self->max_total_bytes = max_total_bytes > 0
                              ? max_total_bytes
                              : WHELK_CLIPBOARD_DEFAULT_MAX_TOTAL;

  whelk_clipboard_history_trim(self->entries, self->max_entries,
                               self->max_total_bytes);
  clipboard_changed(self);
}

guint whelk_clipboard_max_entries(WhelkClipboard *self) {
  return self ? self->max_entries : 0;
}

guint whelk_clipboard_max_entry_bytes(WhelkClipboard *self) {
  return self ? self->max_entry_bytes : 0;
}

guint whelk_clipboard_max_total_bytes(WhelkClipboard *self) {
  return self ? self->max_total_bytes : 0;
}

static gboolean on_prune_timeout(gpointer user_data) {
  whelk_clipboard_prune(user_data);
  return G_SOURCE_CONTINUE;
}

void whelk_clipboard_set_ttl(WhelkClipboard *self, guint seconds) {
  if (!self)
    return;

  self->ttl_seconds = seconds;

  if (self->prune_timeout != 0) {
    g_source_remove(self->prune_timeout);
    self->prune_timeout = 0;
  }

  if (seconds > 0) {
    self->prune_timeout = g_timeout_add_seconds(
        MIN(MAX(seconds, 1), WHELK_CLIPBOARD_PRUNE_SECONDS), on_prune_timeout,
        self);
    whelk_clipboard_prune(self);
  }
}

guint whelk_clipboard_ttl(WhelkClipboard *self) {
  return self ? self->ttl_seconds : 0;
}

guint whelk_clipboard_prune(WhelkClipboard *self) {
  if (!self)
    return 0;

  guint removed = whelk_clipboard_history_prune(
      self->entries, self->ttl_seconds, g_get_real_time());

  if (removed > 0)
    clipboard_changed(self);

  return removed;
}

WhelkClipboardEntry *whelk_clipboard_find(WhelkClipboard *self,
                                          const char *id) {
  if (!self || !id)
    return NULL;

  for (guint i = 0; i < self->entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(self->entries, i);

    if (g_strcmp0(entry->id, id) == 0)
      return entry;
  }

  return NULL;
}

gint whelk_clipboard_index_of(WhelkClipboard *self, const char *id) {
  if (!self || !id)
    return -1;

  for (guint i = 0; i < self->entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(self->entries, i);

    if (g_strcmp0(entry->id, id) == 0)
      return (gint)i;
  }

  return -1;
}

void whelk_clipboard_add(WhelkClipboard *self, const char *mime,
                         const char *text, GBytes *data) {
  if (!self || !data)
    return;

  const gsize size = g_bytes_get_size(data);

  if (size > self->max_entry_bytes) {
    LOG_C_INFO("Clipboard",
               "not storing a %" G_GSIZE_FORMAT "-byte copy: the entry budget "
               "is %u bytes",
               size, self->max_entry_bytes);
    return;
  }

  WhelkClipboardEntry *entry =
      whelk_clipboard_entry_new(mime, text, g_bytes_ref(data));

  whelk_clipboard_history_add(self->entries, entry, self->max_entries);
  whelk_clipboard_history_trim(self->entries, self->max_entries,
                               self->max_total_bytes);
  clipboard_changed(self);
}

gboolean whelk_clipboard_restore(WhelkClipboard *self, guint index) {
  if (!self || index >= self->entries->len)
    return FALSE;

  WhelkClipboardEntry *entry = g_ptr_array_index(self->entries, index);

  write_clipboard(entry);
  move_to_front(self, index);
  clipboard_changed(self);

  return TRUE;
}

gboolean whelk_clipboard_restore_id(WhelkClipboard *self, const char *id) {
  const gint index = whelk_clipboard_index_of(self, id);

  return index >= 0 ? whelk_clipboard_restore(self, (guint)index) : FALSE;
}

void whelk_clipboard_copy(WhelkClipboard *self, const char *text) {
  if (!self || !text || *text == '\0')
    return;

  GBytes *bytes = g_bytes_new(text, strlen(text));

  whelk_clipboard_add(self, "text/plain;charset=utf-8", text, bytes);
  g_bytes_unref(bytes);
  whelk_clipboard_restore(self, 0);
}

gboolean whelk_clipboard_pin(WhelkClipboard *self, guint index,
                             gboolean pinned) {
  if (!self || index >= self->entries->len)
    return FALSE;

  WhelkClipboardEntry *entry = g_ptr_array_index(self->entries, index);

  entry->pinned = pinned;
  whelk_clipboard_history_trim(self->entries, self->max_entries,
                               self->max_total_bytes);
  clipboard_changed(self);

  return TRUE;
}

gboolean whelk_clipboard_pin_id(WhelkClipboard *self, const char *id,
                                gboolean pinned) {
  const gint index = whelk_clipboard_index_of(self, id);

  return index >= 0 ? whelk_clipboard_pin(self, (guint)index, pinned) : FALSE;
}

gboolean whelk_clipboard_remove(WhelkClipboard *self, guint index) {
  if (!self || index >= self->entries->len)
    return FALSE;

  WhelkClipboardEntry *entry = g_ptr_array_steal_index(self->entries, index);

  g_clear_pointer(&self->last_removed, whelk_clipboard_entry_free);
  self->last_removed = entry;
  self->last_removed_index = index;

  clipboard_changed(self);

  return TRUE;
}

gboolean whelk_clipboard_remove_id(WhelkClipboard *self, const char *id) {
  const gint index = whelk_clipboard_index_of(self, id);

  return index >= 0 ? whelk_clipboard_remove(self, (guint)index) : FALSE;
}

gboolean whelk_clipboard_undo_remove(WhelkClipboard *self) {
  if (!self || !self->last_removed)
    return FALSE;

  WhelkClipboardEntry *entry = self->last_removed;

  self->last_removed = NULL;

  guint index = MIN(self->last_removed_index, self->entries->len);

  g_ptr_array_insert(self->entries, index, entry);
  whelk_clipboard_history_trim(self->entries, self->max_entries,
                               self->max_total_bytes);
  clipboard_changed(self);

  return TRUE;
}

void whelk_clipboard_clear(WhelkClipboard *self) {
  if (!self)
    return;

  g_ptr_array_set_size(self->entries, 0);
  g_clear_pointer(&self->last_removed, whelk_clipboard_entry_free);
  clipboard_changed(self);
}

gboolean whelk_clipboard_clear_unpinned(WhelkClipboard *self) {
  if (!self)
    return FALSE;

  gboolean removed = FALSE;

  for (guint i = self->entries->len; i > 0; i--) {
    WhelkClipboardEntry *entry = g_ptr_array_index(self->entries, i - 1);

    if (entry->pinned)
      continue;

    g_ptr_array_remove_index(self->entries, i - 1);
    removed = TRUE;
  }

  g_clear_pointer(&self->last_removed, whelk_clipboard_entry_free);

  if (removed)
    clipboard_changed(self);

  return removed;
}

/* =========================================================================
 * THE HELPER
 * =========================================================================
 * wl-paste --watch runs this with the selection's default representation on
 * stdin.  That stdin is drained first: a large copy would otherwise fill the
 * pipe while this process asks the compositor for the representation it
 * actually wants.  Then the types are read, a password hint is honoured, and
 * the richest type is fetched and framed for the listener.
 */

static gsize helper_limit(void) {
  const char *value = g_getenv("WHELK_CLIPBOARD_MAX_ENTRY");

  if (value && *value) {
    guint64 parsed = g_ascii_strtoull(value, NULL, 10);

    if (parsed >= 1024)
      return MIN(parsed, (guint64)WHELK_CLIPBOARD_DATA_MAX);
  }

  return WHELK_CLIPBOARD_HELPER_LIMIT;
}

/* Reads at most @limit bytes from the process's stdout.  poll() gives the
 * helper a deadline as well as a byte budget: a source that advertises a huge
 * type and then stalls must not freeze every later copy. */
static GBytes *wl_paste_fetch(const char *const *argv, gsize limit,
                              guint timeout_ms) {
  g_autoptr(GError) error = NULL;
  GSubprocess *process = g_subprocess_newv(
      argv, G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_SILENCE,
      &error);

  if (!process) {
    LOG_C_DEBUG("Clipboard", "wl-paste is not available: %s", error->message);
    return NULL;
  }

  GInputStream *pipe = g_subprocess_get_stdout_pipe(process);

  if (!pipe) {
    g_subprocess_force_exit(process);
    g_subprocess_wait(process, NULL, NULL);
    g_object_unref(process);
    return NULL;
  }

  const int fd = g_unix_input_stream_get_fd(G_UNIX_INPUT_STREAM(pipe));
  GByteArray *out = g_byte_array_new();
  gboolean failed = FALSE;
  const gint64 deadline = g_get_monotonic_time() +
                          (gint64)timeout_ms * G_TIME_SPAN_MILLISECOND;

  for (;;) {
    const gint64 remaining = deadline - g_get_monotonic_time();

    if (remaining <= 0) {
      failed = TRUE;
      break;
    }

    struct pollfd poll_fd = {.fd = fd, .events = POLLIN | POLLHUP};
    const int ready = poll(&poll_fd, 1, (int)(remaining / 1000) + 1);

    if (ready == 0) {
      failed = TRUE;
      break;
    }

    if (ready < 0) {
      if (errno == EINTR)
        continue;

      failed = TRUE;
      break;
    }

    guint8 buffer[WHELK_CLIPBOARD_READ_SIZE];
    const ssize_t n = read(fd, buffer, sizeof buffer);

    if (n < 0) {
      if (errno == EINTR)
        continue;

      failed = TRUE;
      break;
    }

    if (n == 0)
      break;

    if (out->len + (guint)n > limit) {
      failed = TRUE;
      break;
    }

    g_byte_array_append(out, buffer, (guint)n);
  }

  g_subprocess_force_exit(process);
  g_subprocess_wait(process, NULL, NULL);
  g_object_unref(process);

  if (failed) {
    LOG_C_DEBUG("Clipboard", "a clipboard representation was too large or too "
                             "slow; it was not stored");
    g_byte_array_unref(out);
    return NULL;
  }

  const gsize length = out->len;
  guint8 *bytes = g_byte_array_free(out, FALSE);

  return g_bytes_new_take(bytes, length);
}

static char *wl_paste_types(void) {
  const char *argv[] = {"wl-paste", "--list-types", NULL};
  g_autoptr(GBytes) bytes =
      wl_paste_fetch(argv, 64 * 1024, 2000);

  if (!bytes)
    return NULL;

  gsize size = 0;
  const char *data = g_bytes_get_data(bytes, &size);

  return g_strndup(data, size);
}

static GBytes *wl_paste_type(const char *mime) {
  const char *argv[] = {"wl-paste", "--no-newline", "--type", mime, NULL};

  return wl_paste_fetch(argv, helper_limit(), 5000);
}

static gboolean write_all(int fd, const void *data, gsize length) {
  const guint8 *cursor = data;

  while (length > 0) {
    ssize_t written = write(fd, cursor, length);

    if (written < 0 && errno == EINTR)
      continue;

    if (written <= 0)
      return FALSE;

    cursor += written;
    length -= (gsize)written;
  }

  return TRUE;
}

/* The helper's stdout is the framed record stream, not a terminal.  Any
 * diagnostic it prints has to go to stderr, or a debug line becomes part of
 * the next record and the listener drops it as malformed. */
static void print_to_stderr(const gchar *text) {
  fputs(text, stderr);
}

int whelk_clipboard_helper(void) {
  g_set_print_handler(print_to_stderr);

  guint8 sink[8192];

  while (read(STDIN_FILENO, sink, sizeof sink) > 0)
    ;

  g_autofree char *types = wl_paste_types();

  if (!types || *types == '\0')
    return 0;

  if (whelk_clipboard_types_secret(types)) {
    LOG_C_DEBUG("Clipboard", "not storing a password manager's selection");
    return 0;
  }

  g_autofree char *mime = whelk_clipboard_pick_mime(types, FALSE);

  if (!mime)
    return 0;

  g_autoptr(GBytes) data = wl_paste_type(mime);

  if (!data || g_bytes_get_size(data) == 0)
    return 0;

  g_autoptr(GBytes) text = NULL;

  /* A rich or URI-list copy keeps its plain rendering for the picker and as a
   * fallback representation.  Plain text is its own rendering. */
  if (g_str_has_prefix(mime, "text/") &&
      !whelk_clipboard_mime_is_plain_text(mime)) {
    g_autofree char *text_mime = whelk_clipboard_pick_mime(types, TRUE);

    if (text_mime && g_strcmp0(text_mime, mime) != 0)
      text = wl_paste_type(text_mime);
  }

  const gsize data_size = g_bytes_get_size(data);
  const gsize text_size = text ? g_bytes_get_size(text) : 0;
  g_autofree char *header = g_strdup_printf("%s %" G_GSIZE_FORMAT
                                            " %" G_GSIZE_FORMAT
                                            " %" G_GSIZE_FORMAT "\n%s",
                                            WHELK_CLIPBOARD_RECORD,
                                            strlen(mime), data_size, text_size,
                                            mime);

  gboolean ok = write_all(STDOUT_FILENO, header, strlen(header)) &&
                write_all(STDOUT_FILENO, g_bytes_get_data(data, NULL),
                          data_size);

  if (ok && text)
    ok = write_all(STDOUT_FILENO, g_bytes_get_data(text, NULL), text_size);

  return ok ? 0 : 1;
}

/* =========================================================================
 * LIFECYCLE
 * ========================================================================= */

static void whelk_clipboard_dispose(GObject *object) {
  WhelkClipboard *self = WHELK_CLIPBOARD(object);

  if (self->save_timeout != 0) {
    g_source_remove(self->save_timeout);
    self->save_timeout = 0;
  }

  if (self->prune_timeout != 0) {
    g_source_remove(self->prune_timeout);
    self->prune_timeout = 0;
  }

  stop_watch(self);

  if (self->lock_sub != 0 && self->logind) {
    g_dbus_connection_signal_unsubscribe(self->logind, self->lock_sub);
    self->lock_sub = 0;
  }

  g_clear_object(&self->logind);
  g_clear_pointer(&self->lock_session_path, g_free);

  if (self->pending) {
    g_byte_array_unref(self->pending);
    self->pending = NULL;
  }

  g_clear_pointer(&self->last_removed, whelk_clipboard_entry_free);
  g_clear_pointer(&self->persist_path, g_free);
  g_clear_pointer(&self->entries, g_ptr_array_unref);

  G_OBJECT_CLASS(whelk_clipboard_parent_class)->dispose(object);
}

static void whelk_clipboard_class_init(WhelkClipboardClass *klass) {
  GObjectClass *object_class = G_OBJECT_CLASS(klass);

  object_class->dispose = whelk_clipboard_dispose;

  signals[SIGNAL_CHANGED] =
      g_signal_new("changed", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST, 0,
                   NULL, NULL, NULL, G_TYPE_NONE, 0);
}

static void whelk_clipboard_init(WhelkClipboard *self) {
  self->max_entries = WHELK_CLIPBOARD_DEFAULT_MAX;
  self->max_entry_bytes = WHELK_CLIPBOARD_DEFAULT_MAX_ENTRY;
  self->max_total_bytes = WHELK_CLIPBOARD_DEFAULT_MAX_TOTAL;
  self->entries = whelk_clipboard_history_new();
  self->pending = g_byte_array_new();
}

WhelkClipboard *whelk_clipboard_new(guint max_entries) {
  WhelkClipboard *self = g_object_new(WHELK_TYPE_CLIPBOARD, NULL);

  if (max_entries > 0)
    self->max_entries = max_entries;

  return self;
}

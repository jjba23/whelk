// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <errno.h>
#include <gio/gio.h>
#include <glib/gstdio.h>
#include <string.h>
#include <unistd.h>

#include "logger.h"

#define WHELK_CLIPBOARD_DEFAULT_MAX 50
#define WHELK_CLIPBOARD_READ_SIZE 8192
#define WHELK_CLIPBOARD_TEXT_MAX 8192
#define WHELK_CLIPBOARD_DATA_MAX (64 * 1024 * 1024)
#define WHELK_CLIPBOARD_HEADER_MAX 512
#define WHELK_CLIPBOARD_MAGIC "whelk-clipboard-1"
#define WHELK_CLIPBOARD_RECORD "WHELK1"

/* The helper wl-paste runs on every change.  It is this same binary, so a
 * development build and an installed one both use the code they shipped with
 * instead of whatever `whelk` happens to be first on PATH. */
#define WHELK_CLIPBOARD_STORE_FLAG "--clipboard-store"

typedef struct {
  WhelkClipboard *self; /* a strong reference: the read outlives a dispose */
  guint8 *buffer;
} ReadContext;

struct _WhelkClipboard {
  GObject parent_instance;

  guint max_entries;
  GPtrArray *entries; /* WhelkClipboardEntry *, newest first */
  GByteArray *pending;

  GSubprocess *watch;
  GInputStream *stream;

  char *persist_path;
  gboolean persist_loaded;
};

G_DEFINE_TYPE(WhelkClipboard, whelk_clipboard, G_TYPE_OBJECT)

enum { SIGNAL_CHANGED, N_SIGNALS };

static guint signals[N_SIGNALS];

/* =========================================================================
 * ENTRIES
 * ========================================================================= */

WhelkClipboardEntry *whelk_clipboard_entry_new(const char *mime,
                                               const char *text, GBytes *data) {
  WhelkClipboardEntry *entry = g_new0(WhelkClipboardEntry, 1);

  entry->mime = g_strdup(mime ? mime : "text/plain;charset=utf-8");
  entry->text = g_strdup(text ? text : "");
  entry->data = data;

  return entry;
}

void whelk_clipboard_entry_free(WhelkClipboardEntry *entry) {
  if (!entry)
    return;

  g_free(entry->mime);
  g_free(entry->text);
  g_clear_pointer(&entry->data, g_bytes_unref);
  g_free(entry);
}

static gboolean entries_equal(WhelkClipboardEntry *a, WhelkClipboardEntry *b) {
  return a && b && g_strcmp0(a->mime, b->mime) == 0 &&
         g_bytes_equal(a->data, b->data);
}

GPtrArray *whelk_clipboard_history_new(void) {
  return g_ptr_array_new_with_free_func(
      (GDestroyNotify)whelk_clipboard_entry_free);
}

/* =========================================================================
 * THE HISTORY RULE
 * =========================================================================
 * The cap counts unpinned entries only: pinning one is a promise that the
 * fiftieth copy after it will not push it off the end.  A duplicate moves the
 * existing entry to the front, and a pin survives that move.
 */

static void trim_history(GPtrArray *entries, guint max_entries) {
  const guint cap = max_entries > 0 ? max_entries : WHELK_CLIPBOARD_DEFAULT_MAX;
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

void whelk_clipboard_history_add(GPtrArray *entries, WhelkClipboardEntry *entry,
                                 guint max_entries) {
  if (!entries || !entry)
    return;

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *existing = g_ptr_array_index(entries, i);

    if (!entries_equal(existing, entry))
      continue;

    WhelkClipboardEntry *kept = g_ptr_array_steal_index(entries, i);

    g_ptr_array_insert(entries, 0, kept);
    whelk_clipboard_entry_free(entry);
    trim_history(entries, max_entries);
    return;
  }

  g_ptr_array_insert(entries, 0, entry);
  trim_history(entries, max_entries);
}

static WhelkClipboardEntry *entry_at(WhelkClipboard *self, guint index) {
  if (!self || index >= self->entries->len)
    return NULL;

  return g_ptr_array_index(self->entries, index);
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
    "image/png", "image/jpeg", "image/webp", "text/html", "text/rtf",
    "text/plain;charset=utf-8", "text/plain",
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
    /* No newline yet is a short read, not a malformed record -- unless the
     * header has grown past any length the helper could have written. */
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

      /* A malformed record means the helper and this parser have drifted
       * apart; keeping the bytes would make every later record unreadable. */
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
 * PERSISTENCE, OPT-IN
 * ========================================================================= */

static GVariant *entries_to_variant(GPtrArray *entries) {
  GVariantBuilder builder;

  g_variant_builder_init(&builder, G_VARIANT_TYPE("a(ssayb)"));

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i);
    gsize size = 0;
    const guint8 *data = g_bytes_get_data(entry->data, &size);

    g_variant_builder_add(
        &builder, "(ss@ayb)", entry->mime, entry->text,
        g_variant_new_fixed_array(G_VARIANT_TYPE_BYTE, data, size, 1),
        entry->pinned);
  }

  return g_variant_new("(s@a(ssayb))", WHELK_CLIPBOARD_MAGIC,
                       g_variant_builder_end(&builder));
}

static GPtrArray *variant_to_entries(GVariant *root, guint max_entries) {
  GPtrArray *entries = whelk_clipboard_history_new();
  g_autoptr(GVariant) magic = g_variant_get_child_value(root, 0);

  if (g_strcmp0(g_variant_get_string(magic, NULL), WHELK_CLIPBOARD_MAGIC) != 0)
    return entries;

  g_autoptr(GVariant) array = g_variant_get_child_value(root, 1);
  const gsize count = g_variant_n_children(array);

  /* The file is newest first, and history_add prepends, so the file is walked
   * backwards: otherwise a restart would reverse the history. */
  for (gsize i = count; i > 0; i--) {
    g_autoptr(GVariant) item = g_variant_get_child_value(array, i - 1);
    const char *mime = NULL;
    const char *text = NULL;
    GVariant *bytes = NULL;
    gboolean pinned = FALSE;

    g_variant_get(item, "(ss@ayb)", &mime, &text, &bytes, &pinned);

    GBytes *data = g_variant_get_data_as_bytes(bytes);

    g_variant_unref(bytes);

    WhelkClipboardEntry *entry = whelk_clipboard_entry_new(mime, text, data);

    entry->pinned = pinned;
    whelk_clipboard_history_add(entries, entry, max_entries);
  }

  return entries;
}

static void clipboard_save(WhelkClipboard *self) {
  if (!self->persist_path || !self->persist_loaded)
    return;

  g_autofree char *directory = g_path_get_dirname(self->persist_path);

  g_mkdir_with_parents(directory, 0700);

  g_autoptr(GVariant) root = entries_to_variant(self->entries);
  g_autoptr(GError) error = NULL;

  if (!g_file_set_contents_full(self->persist_path,
                                g_variant_get_data(root),
                                (gssize)g_variant_get_size(root),
                                G_FILE_SET_CONTENTS_CONSISTENT, 0600, &error))
    LOG_C_WARN("Clipboard", "could not save the history: %s", error->message);
  else if (g_chmod(self->persist_path, 0600) != 0)
    LOG_C_WARN("Clipboard", "could not make the history private: %s",
               g_strerror(errno));
}

gboolean whelk_clipboard_set_persistence(WhelkClipboard *self,
                                         const char *path) {
  if (!self)
    return FALSE;

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

  g_autofree char *contents = NULL;
  gsize length = 0;

  if (!g_file_get_contents(self->persist_path, &contents, &length, NULL))
    return FALSE;

  g_autoptr(GBytes) blob = g_bytes_new(contents, length);
  g_autoptr(GVariant) root = g_variant_new_from_bytes(
      G_VARIANT_TYPE("(sa(ssayb))"), blob, FALSE);

  if (!root || !g_variant_is_of_type(root, G_VARIANT_TYPE("(sa(ssayb))")) ||
      !g_variant_is_normal_form(root))
    return FALSE;

  GPtrArray *loaded = variant_to_entries(root, self->max_entries);

  g_ptr_array_unref(self->entries);
  self->entries = loaded;

  return TRUE;
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

static void write_clipboard(WhelkClipboardEntry *entry) {
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
  clipboard_save(self);
}

static void read_next(WhelkClipboard *self);

static void on_read(GObject *source, GAsyncResult *result, gpointer user_data) {
  ReadContext *context = user_data;
  WhelkClipboard *self = context->self;
  g_autoptr(GError) error = NULL;

  const gssize n = g_input_stream_read_finish(G_INPUT_STREAM(source), result,
                                              &error);

  if (n <= 0) {
    if (error)
      LOG_C_DEBUG("Clipboard", "wl-paste stopped: %s", error->message);

    g_free(context->buffer);
    g_object_unref(context->self);
    g_free(context);

    return;
  }

  g_byte_array_append(self->pending, context->buffer, (guint)n);

  if (whelk_clipboard_parse(self->pending, self->entries,
                            self->max_entries) > 0)
    clipboard_changed(self);

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

  g_input_stream_read_async(self->stream, context->buffer,
                            WHELK_CLIPBOARD_READ_SIZE, G_PRIORITY_DEFAULT,
                            NULL, on_read, context);
}

void whelk_clipboard_start(WhelkClipboard *self) {
  if (!self || self->watch)
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
    LOG_C_DEBUG("Clipboard",
                "wl-paste is not installed: the history stays empty (%s)",
                error->message);
    return;
  }

  self->stream = g_subprocess_get_stdout_pipe(self->watch);
  g_object_ref(self->stream);

  read_next(self);
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

void whelk_clipboard_add(WhelkClipboard *self, const char *mime,
                         const char *text, GBytes *data) {
  if (!self || !data)
    return;

  WhelkClipboardEntry *entry =
      whelk_clipboard_entry_new(mime, text, g_bytes_ref(data));

  whelk_clipboard_history_add(self->entries, entry, self->max_entries);
  clipboard_changed(self);
}

gboolean whelk_clipboard_restore(WhelkClipboard *self, guint index) {
  WhelkClipboardEntry *entry = entry_at(self, index);

  if (!entry)
    return FALSE;

  write_clipboard(entry);
  move_to_front(self, index);
  clipboard_changed(self);

  return TRUE;
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
  WhelkClipboardEntry *entry = entry_at(self, index);

  if (!entry)
    return FALSE;

  entry->pinned = pinned;
  clipboard_changed(self);

  return TRUE;
}

gboolean whelk_clipboard_remove(WhelkClipboard *self, guint index) {
  if (!entry_at(self, index))
    return FALSE;

  g_ptr_array_remove_index(self->entries, index);
  clipboard_changed(self);

  return TRUE;
}

void whelk_clipboard_clear(WhelkClipboard *self) {
  if (!self)
    return;

  g_ptr_array_set_size(self->entries, 0);
  clipboard_changed(self);
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

static GBytes *wl_paste_fetch(const char *const *argv) {
  g_autoptr(GError) error = NULL;
  GSubprocess *process = g_subprocess_newv(
      argv, G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_SILENCE,
      &error);

  if (!process) {
    LOG_C_DEBUG("Clipboard", "wl-paste is not available: %s", error->message);
    return NULL;
  }

  GBytes *output = NULL;

  if (!g_subprocess_communicate(process, NULL, NULL, &output, NULL, &error))
    LOG_C_DEBUG("Clipboard", "wl-paste failed: %s", error->message);

  g_object_unref(process);

  return output;
}

static char *wl_paste_types(void) {
  const char *argv[] = {"wl-paste", "--list-types", NULL};
  g_autoptr(GBytes) bytes = wl_paste_fetch(argv);

  if (!bytes)
    return NULL;

  gsize size = 0;
  const char *data = g_bytes_get_data(bytes, &size);

  return g_strndup(data, size);
}

static GBytes *wl_paste_type(const char *mime) {
  const char *argv[] = {"wl-paste", "--no-newline", "--type", mime, NULL};

  return wl_paste_fetch(argv);
}

static gboolean write_all(int fd, const void *data, gsize length) {
  const guint8 *cursor = data;

  while (length > 0) {
    ssize_t written = write(fd, cursor, length);

    if (written <= 0)
      return FALSE;

    cursor += written;
    length -= (gsize)written;
  }

  return TRUE;
}

int whelk_clipboard_helper(void) {
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

  if (!g_str_has_prefix(mime, "text/")) {
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

  if (self->watch) {
    g_subprocess_force_exit(self->watch);
    g_clear_object(&self->watch);
  }

  g_clear_object(&self->stream);

  if (self->pending) {
    g_byte_array_unref(self->pending);
    self->pending = NULL;
  }

  g_free(self->persist_path);
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
  self->entries = whelk_clipboard_history_new();
  self->pending = g_byte_array_new();
}

WhelkClipboard *whelk_clipboard_new(guint max_entries) {
  WhelkClipboard *self = g_object_new(WHELK_TYPE_CLIPBOARD, NULL);

  if (max_entries > 0)
    self->max_entries = max_entries;

  return self;
}

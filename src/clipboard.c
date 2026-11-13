// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <gio/gio.h>
#include <string.h>

#include "logger.h"
#include "util/command.h"

#define WHELK_CLIPBOARD_DEFAULT_MAX 50
#define WHELK_CLIPBOARD_READ_SIZE 8192

/* The program wl-paste runs on every change: it copies stdin (the new
 * selection) to our pipe, then writes the NUL that ends the item. */
#define WHELK_CLIPBOARD_WATCH "cat; printf '\\000'"

typedef struct {
  WhelkClipboard *self; /* a strong reference: the read outlives a dispose */
  guint8 *buffer;
} ReadContext;

struct _WhelkClipboard {
  GObject parent_instance;

  guint max_entries;
  GPtrArray *entries; /* char *, newest first */
  GString *pending;   /* bytes read but not yet ended by a NUL */

  GSubprocess *watch;
  GInputStream *stream;
  gulong changed_id;
};

G_DEFINE_TYPE(WhelkClipboard, whelk_clipboard, G_TYPE_OBJECT)

enum { SIGNAL_CHANGED, N_SIGNALS };

static guint signals[N_SIGNALS];

/* =========================================================================
 * THE HISTORY RULE
 * ========================================================================= */

void whelk_clipboard_split(GString *pending, GPtrArray *out) {
  if (!pending || !out)
    return;

  for (;;) {
    const char *data = pending->str;
    const char *end = memchr(data, '\0', pending->len);

    if (!end)
      return;

    const gsize length = (gsize)(end - data);

    g_ptr_array_add(out, g_strndup(data, length));
    g_string_erase(pending, 0, length + 1);
  }
}

void whelk_clipboard_history_add(GPtrArray *entries, const char *text,
                                 guint max_entries) {
  if (!entries || !text || *text == '\0')
    return;

  const guint cap = max_entries > 0 ? max_entries : WHELK_CLIPBOARD_DEFAULT_MAX;

  for (guint i = 0; i < entries->len; i++) {
    if (g_strcmp0(g_ptr_array_index(entries, i), text) == 0) {
      g_ptr_array_remove_index(entries, i);
      break;
    }
  }

  g_ptr_array_insert(entries, 0, g_strdup(text));

  while (entries->len > cap)
    g_ptr_array_remove_index(entries, entries->len - 1);
}

/* =========================================================================
 * THE LISTENER
 * ========================================================================= */

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

  g_string_append_len(self->pending, (const char *)context->buffer, n);

  GPtrArray *items = g_ptr_array_new_with_free_func(g_free);

  whelk_clipboard_split(self->pending, items);

  gboolean added = FALSE;

  for (guint i = 0; i < items->len; i++) {
    const char *text = g_ptr_array_index(items, i);

    if (*text != '\0') {
      whelk_clipboard_history_add(self->entries, text,
                                  self->max_entries);
      added = TRUE;
    }
  }

  g_ptr_array_unref(items);

  if (added)
    g_signal_emit(self, signals[SIGNAL_CHANGED], 0);

  g_free(context->buffer);
  g_object_unref(context->self);
  g_free(context);

  read_next(self);
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

  const char *argv[] = {"wl-paste", "--watch", "sh", "-c",
                        WHELK_CLIPBOARD_WATCH, NULL};
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

void whelk_clipboard_copy(WhelkClipboard *self, const char *text) {
  if (!self || !text || *text == '\0')
    return;

  /* Optimistically, so the picker shows the copy even before the watcher
   * hears its own echo; the echo lands on the same entry and moves it up. */
  whelk_clipboard_history_add(self->entries, text, self->max_entries);
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);

  whelk_command_run_input("wl-copy", text, NULL, NULL);
}

void whelk_clipboard_clear(WhelkClipboard *self) {
  if (!self)
    return;

  g_ptr_array_set_size(self->entries, 0);
  g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
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
    g_string_free(self->pending, TRUE);
    self->pending = NULL;
  }

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
  self->entries = g_ptr_array_new_with_free_func(g_free);
  self->pending = g_string_new(NULL);
}

WhelkClipboard *whelk_clipboard_new(guint max_entries) {
  WhelkClipboard *self = g_object_new(WHELK_TYPE_CLIPBOARD, NULL);

  if (max_entries > 0)
    self->max_entries = max_entries;

  return self;
}

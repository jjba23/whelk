// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <fcntl.h>
#include <glib/gstdio.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/* The clipboard's decisions, without a compositor: what is kept, what a secret
 * hint refuses, which representation is preferred, how a binary record is
 * framed, and what survives a restart when persistence was asked for. */

static WhelkClipboardEntry *text_entry(const char *text) {
  return whelk_clipboard_entry_new("text/plain;charset=utf-8", text,
                                   g_bytes_new(text, strlen(text)));
}

static WhelkClipboardEntry *image_entry(const guint8 *bytes, gsize length) {
  return whelk_clipboard_entry_new("image/png", "", g_bytes_new(bytes, length));
}

/* The helper and the restore path run real programs; these stubs make them
 * real without a compositor.  The helper's nested wl-paste is the seam that a
 * pure test cannot reach. */
static char *make_fake_tools(void) {
  g_autoptr(GError) error = NULL;
  char *dir = g_dir_make_tmp("whelk-clipboard-test-XXXXXX", &error);

  g_assert_no_error(error);
  g_assert_nonnull(dir);

  /* The stubs are this same test binary under another name.  A shell script
   * would be shorter, but the Guix build sandbox has no /bin/sh or /bin/cat
   * for it to depend on; a symlink has no interpreter at all. */
  g_autofree char *self = g_file_read_link("/proc/self/exe", NULL);
  g_autofree char *paste_path = g_build_filename(dir, "wl-paste", NULL);
  g_autofree char *copy_path = g_build_filename(dir, "wl-copy", NULL);

  g_assert_nonnull(self);
  g_assert_cmpint(symlink(self, paste_path), ==, 0);
  g_assert_cmpint(symlink(self, copy_path), ==, 0);

  return dir;
}

static GByteArray *run_helper(const char *dir, const char *types,
                              const char *image, const char *html,
                              const char *text, const char *max_entry) {
  int fds[2];

  g_assert_cmpint(pipe(fds), ==, 0);
  fflush(NULL);

  const pid_t child = fork();

  g_assert_cmpint(child, >=, 0);

  if (child == 0) {
    close(fds[0]);
    dup2(fds[1], STDOUT_FILENO);
    close(fds[1]);

    const int devnull = open("/dev/null", O_RDONLY);

    if (devnull >= 0) {
      dup2(devnull, STDIN_FILENO);
      close(devnull);
    }

    g_setenv("PATH", dir, TRUE);
    g_setenv("FAKE_TYPES", types ? types : "", TRUE);
    g_setenv("FAKE_IMAGE", image ? image : "", TRUE);
    g_setenv("FAKE_HTML", html ? html : "", TRUE);
    g_setenv("FAKE_TEXT", text ? text : "", TRUE);

    if (max_entry)
      g_setenv("WHELK_CLIPBOARD_MAX_ENTRY", max_entry, TRUE);

    _exit(whelk_clipboard_helper());
  }

  close(fds[1]);

  GByteArray *out = g_byte_array_new();
  guint8 buffer[4096];
  ssize_t n = 0;

  while ((n = read(fds[0], buffer, sizeof buffer)) > 0)
    g_byte_array_append(out, buffer, (guint)n);

  close(fds[0]);
  waitpid(child, NULL, 0);

  return out;
}

static void test_helper_selects_image_and_frames_it(void) {
  g_autofree char *dir = make_fake_tools();
  g_autoptr(GByteArray) raw =
      run_helper(dir, "image/png\ntext/html\ntext/plain\n", "PNGDATA",
                 "<b>html</b>", "plain", NULL);
  g_autoptr(GPtrArray) entries = whelk_clipboard_history_new();
  GByteArray *pending = g_byte_array_new();

  g_byte_array_append(pending, raw->data, raw->len);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 1);
  g_assert_cmpuint(entries->len, ==, 1);

  WhelkClipboardEntry *entry = g_ptr_array_index(entries, 0);
  gsize size = 0;
  const char *data = g_bytes_get_data(entry->data, &size);

  g_assert_cmpstr(entry->mime, ==, "image/png");
  g_assert_cmpuint(size, ==, 7);
  g_assert_cmpmem(data, size, "PNGDATA", 7);

  g_byte_array_unref(pending);
}

static void test_helper_keeps_plain_rendering_of_html(void) {
  g_autofree char *dir = make_fake_tools();
  g_autoptr(GByteArray) raw = run_helper(
      dir, "text/html\ntext/plain\n", NULL, "<b>html</b>", "html", NULL);
  g_autoptr(GPtrArray) entries = whelk_clipboard_history_new();
  GByteArray *pending = g_byte_array_new();

  g_byte_array_append(pending, raw->data, raw->len);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 1);

  WhelkClipboardEntry *entry = g_ptr_array_index(entries, 0);

  g_assert_cmpstr(entry->mime, ==, "text/html");
  g_assert_cmpstr(entry->text, ==, "html");

  g_byte_array_unref(pending);
}

static void test_helper_refuses_password_hint(void) {
  g_autofree char *dir = make_fake_tools();
  g_autoptr(GByteArray) raw =
      run_helper(dir, "text/plain\nx-kde-passwordManagerHint\n", NULL, NULL,
                 "secret", NULL);

  g_assert_cmpuint(raw->len, ==, 0);
}

static void test_helper_refuses_oversized_source(void) {
  g_autofree char *dir = make_fake_tools();
  g_autofree char *huge = g_strnfill(4096, 'x');
  g_autoptr(GByteArray) raw = run_helper(
      dir, "image/png\n", huge, NULL, NULL, "1024");

  g_assert_cmpuint(raw->len, ==, 0);
}

static void test_restore_uses_the_selected_mime(void) {
  g_autofree char *dir = make_fake_tools();
  g_autofree char *log = g_build_filename(dir, "restore.log", NULL);
  g_autofree char *old_path = g_strdup(g_getenv("PATH"));
  g_autofree char *test_path =
      g_strdup_printf("%s%s%s", dir, old_path ? ":" : "",
                      old_path ? old_path : "");

  g_setenv("PATH", test_path, TRUE);
  g_setenv("FAKE_LOG", log, TRUE);
  g_unsetenv("DISPLAY");
  g_unsetenv("WAYLAND_DISPLAY");

  WhelkClipboard *clipboard = whelk_clipboard_new(50);

  whelk_clipboard_add(clipboard, "text/html", "plain",
                      g_bytes_new_static("<b>html</b>", 11));
  g_assert_true(whelk_clipboard_restore(clipboard, 0));

  for (guint i = 0; i < 50 && !g_file_test(log, G_FILE_TEST_EXISTS); i++) {
    g_main_context_iteration(NULL, FALSE);
    g_usleep(20 * 1000);
  }

  g_autofree char *contents = NULL;

  g_assert_true(g_file_get_contents(log, &contents, NULL, NULL));
  g_assert_nonnull(strstr(contents, "--type text/html"));

  g_object_unref(clipboard);

  if (old_path)
    g_setenv("PATH", old_path, TRUE);
  else
    g_unsetenv("PATH");
  g_unsetenv("FAKE_LOG");
  g_unlink(log);
  g_autofree char *paste_path = g_build_filename(dir, "wl-paste", NULL);
  g_autofree char *copy_path = g_build_filename(dir, "wl-copy", NULL);

  g_unlink(paste_path);
  g_unlink(copy_path);
  g_rmdir(dir);
}

static void test_newest_first(void) {
  GPtrArray *entries = whelk_clipboard_history_new();

  whelk_clipboard_history_add(entries, text_entry("one"), 50);
  whelk_clipboard_history_add(entries, text_entry("two"), 50);

  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->text, ==, "two");
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(entries, 1))->text, ==, "one");

  g_ptr_array_unref(entries);
}

static void test_repeat_moves_up_and_keeps_pin(void) {
  GPtrArray *entries = whelk_clipboard_history_new();

  whelk_clipboard_history_add(entries, text_entry("one"), 50);
  whelk_clipboard_history_add(entries, text_entry("two"), 50);

  WhelkClipboardEntry *one = g_ptr_array_index(entries, 1);

  one->pinned = TRUE;
  whelk_clipboard_history_add(entries, text_entry("one"), 50);

  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->text, ==, "one");
  g_assert_true(((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->pinned);

  g_ptr_array_unref(entries);
}

static void test_images_dedupe_by_bytes(void) {
  GPtrArray *entries = whelk_clipboard_history_new();
  const guint8 first[] = {0x89, 'P', 'N', 'G', 0x00, 0x01};
  const guint8 same[] = {0x89, 'P', 'N', 'G', 0x00, 0x01};

  whelk_clipboard_history_add(entries, image_entry(first, sizeof first), 50);
  whelk_clipboard_history_add(entries, text_entry("text"), 50);
  whelk_clipboard_history_add(entries, image_entry(same, sizeof same), 50);

  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->mime,
                  ==, "image/png");

  g_ptr_array_unref(entries);
}

static void test_plain_text_spellings_share_an_id(void) {
  GPtrArray *entries = whelk_clipboard_history_new();
  const guint8 bytes[] = {'s', 'a', 'm', 'e'};
  WhelkClipboardEntry *first = text_entry("same");
  WhelkClipboardEntry *second = whelk_clipboard_entry_new(
      "text/plain", "same", g_bytes_new(bytes, sizeof bytes));

  g_assert_cmpstr(first->id, ==, second->id);
  whelk_clipboard_history_add(entries, first, 50);
  whelk_clipboard_history_add(entries, second, 50);
  g_assert_cmpuint(entries->len, ==, 1);

  g_ptr_array_unref(entries);
}

static void test_ttl_and_undo(void) {
  WhelkClipboard *clipboard = whelk_clipboard_new(50);

  whelk_clipboard_add(clipboard, "text/plain", "old",
                      g_bytes_new_static("old", 3));
  ((WhelkClipboardEntry *)g_ptr_array_index(
       whelk_clipboard_entries(clipboard), 0))
      ->created_us = g_get_real_time() - 2 * G_TIME_SPAN_SECOND;

  whelk_clipboard_add(clipboard, "text/plain", "new",
                      g_bytes_new_static("new", 3));
  whelk_clipboard_set_ttl(clipboard, 1);
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 1);
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(
           whelk_clipboard_entries(clipboard), 0))
          ->text,
      ==, "new");

  whelk_clipboard_add(clipboard, "text/plain", "victim",
                      g_bytes_new_static("victim", 6));
  g_assert_true(whelk_clipboard_remove(clipboard, 0));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 1);
  g_assert_true(whelk_clipboard_undo_remove(clipboard));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 2);
  g_assert_false(whelk_clipboard_undo_remove(clipboard));

  g_object_unref(clipboard);
}

static void test_limits(void) {
  WhelkClipboard *clipboard = whelk_clipboard_new(50);
  const guint8 six[] = {'1', '2', '3', '4', '5', '6'};

  whelk_clipboard_set_limits(clipboard, 50, 8, 128);
  whelk_clipboard_add(clipboard, "text/plain", "too big",
                      g_bytes_new_static("0123456789", 10));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 0);

  whelk_clipboard_set_limits(clipboard, 50, 8, 10);
  whelk_clipboard_add(clipboard, "text/plain", "first",
                      g_bytes_new(six, sizeof six));
  whelk_clipboard_add(clipboard, "text/plain", "second",
                      g_bytes_new(six, sizeof six));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 1);
  g_assert_cmpuint(whelk_clipboard_bytes(clipboard), <=, 10);

  g_object_unref(clipboard);
}

static void test_pinned_survives_the_cap(void) {
  GPtrArray *entries = whelk_clipboard_history_new();

  whelk_clipboard_history_add(entries, text_entry("a"), 2);
  whelk_clipboard_history_add(entries, text_entry("b"), 2);
  whelk_clipboard_history_add(entries, text_entry("c"), 2);

  /* Newest first: c, b, a.  Pin b, then let the cap bite. */
  ((WhelkClipboardEntry *)g_ptr_array_index(entries, 1))->pinned = TRUE;
  whelk_clipboard_history_add(entries, text_entry("d"), 2);

  g_assert_cmpuint(entries->len, ==, 3);

  gboolean has_b = FALSE;

  for (guint i = 0; i < entries->len; i++) {
    WhelkClipboardEntry *entry = g_ptr_array_index(entries, i);

    if (g_strcmp0(entry->text, "b") == 0)
      has_b = entry->pinned;
  }

  g_assert_true(has_b);
  g_ptr_array_unref(entries);
}

static void test_zero_means_the_default_cap(void) {
  GPtrArray *entries = whelk_clipboard_history_new();
  g_autofree char *text = g_strdup_printf("%0*d", 4000, 7);

  for (guint i = 0; i < 55; i++) {
    g_autofree char *item = g_strdup_printf("%u %s", i, text);

    whelk_clipboard_history_add(entries, text_entry(item), 0);
  }

  g_assert_cmpuint(entries->len, ==, 50);
  g_ptr_array_unref(entries);
}

static void test_secret_hint(void) {
  g_assert_true(whelk_clipboard_types_secret(
      "text/plain\nx-kde-passwordManagerHint"));
  g_assert_true(whelk_clipboard_types_secret(
      "application/x-kde-passwordManagerHint\ntext/plain"));
  g_assert_false(
      whelk_clipboard_types_secret("text/plain\ntext/html\nimage/png"));
  g_assert_false(whelk_clipboard_types_secret(NULL));
}

static void test_pick_mime(void) {
  g_autofree char *rich = whelk_clipboard_pick_mime(
      "text/plain\ntext/html\nimage/png", FALSE);
  g_autofree char *text_only = whelk_clipboard_pick_mime(
      "image/png\ntext/plain;charset=utf-8", TRUE);
  g_autofree char *none =
      whelk_clipboard_pick_mime("TIMESTAMP\nTARGETS", FALSE);
  g_autofree char *files =
      whelk_clipboard_pick_mime("text/plain\ntext/uri-list", FALSE);

  g_assert_cmpstr(rich, ==, "image/png");
  g_assert_cmpstr(text_only, ==, "text/plain;charset=utf-8");
  g_assert_cmpstr(files, ==, "text/uri-list");
  g_assert_null(none);
}

/* One record, as the helper writes it. */
static GBytes *record(const char *mime, const guint8 *data, gsize data_size,
                      const char *text) {
  g_autofree char *header = g_strdup_printf(
      "WHELK1 %" G_GSIZE_FORMAT " %" G_GSIZE_FORMAT " %" G_GSIZE_FORMAT
      "\n%s",
      strlen(mime), data_size, text ? strlen(text) : 0, mime);
  gsize total = strlen(header) + data_size + (text ? strlen(text) : 0);
  guint8 *bytes = g_malloc(total);
  gsize offset = 0;

  memcpy(bytes + offset, header, strlen(header));
  offset += strlen(header);
  memcpy(bytes + offset, data, data_size);
  offset += data_size;

  if (text) {
    memcpy(bytes + offset, text, strlen(text));
    offset += strlen(text);
  }

  return g_bytes_new_take(bytes, offset);
}

static void test_parse_waits_for_a_whole_record(void) {
  const guint8 image[] = {0x89, 'P', 'N', 'G', 0x00, 0x01, 0x02};
  g_autoptr(GBytes) bytes = record("image/png", image, sizeof image, "shot");
  gsize size = 0;
  const guint8 *data = g_bytes_get_data(bytes, &size);
  GByteArray *pending = g_byte_array_new();
  GPtrArray *entries = whelk_clipboard_history_new();

  g_byte_array_append(pending, data, size / 2);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 0);
  g_assert_cmpuint(entries->len, ==, 0);

  g_byte_array_append(pending, data + size / 2, size - size / 2);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 1);
  g_assert_cmpuint(pending->len, ==, 0);
  g_assert_cmpuint(entries->len, ==, 1);

  WhelkClipboardEntry *entry = g_ptr_array_index(entries, 0);
  gsize stored = 0;
  const guint8 *payload = g_bytes_get_data(entry->data, &stored);

  g_assert_cmpstr(entry->mime, ==, "image/png");
  g_assert_cmpstr(entry->text, ==, "shot");
  g_assert_cmpuint(stored, ==, sizeof image);
  g_assert_cmpint(memcmp(payload, image, sizeof image), ==, 0);

  g_ptr_array_unref(entries);
  g_byte_array_unref(pending);
}

static void test_parse_two_records(void) {
  const guint8 one[] = {'a', 'b'};
  const guint8 two[] = {'c', 'd', 'e'};
  g_autoptr(GBytes) first = record("text/plain", one, sizeof one, NULL);
  g_autoptr(GBytes) second = record("text/html", two, sizeof two, "plain");
  gsize first_size = 0;
  gsize second_size = 0;
  const guint8 *first_data = g_bytes_get_data(first, &first_size);
  const guint8 *second_data = g_bytes_get_data(second, &second_size);
  GByteArray *pending = g_byte_array_new();
  GPtrArray *entries = whelk_clipboard_history_new();

  g_byte_array_append(pending, first_data, first_size);
  g_byte_array_append(pending, second_data, second_size);

  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 2);
  g_assert_cmpuint(entries->len, ==, 2);
  g_assert_cmpstr(((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->mime,
                  ==, "text/html");
  g_assert_cmpstr(((WhelkClipboardEntry *)g_ptr_array_index(entries, 0))->text,
                  ==, "plain");

  g_ptr_array_unref(entries);
  g_byte_array_unref(pending);
}

static void test_parse_waits_for_the_header(void) {
  const guint8 bytes[] = {'o', 'k'};
  g_autoptr(GBytes) blob = record("text/plain", bytes, sizeof bytes, NULL);
  gsize size = 0;
  const guint8 *data = g_bytes_get_data(blob, &size);
  GByteArray *pending = g_byte_array_new();
  GPtrArray *entries = whelk_clipboard_history_new();

  /* Five bytes are enough to know the record started, not enough to know its
   * lengths: the parser must wait instead of calling a short read malformed. */
  g_byte_array_append(pending, data, 8);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 0);
  g_assert_cmpuint(pending->len, ==, 8);
  g_assert_cmpuint(entries->len, ==, 0);

  g_byte_array_append(pending, data + 8, size - 8);
  g_assert_cmpuint(whelk_clipboard_parse(pending, entries, 50), ==, 1);
  g_assert_cmpuint(entries->len, ==, 1);

  g_ptr_array_unref(entries);
  g_byte_array_unref(pending);
}

static void test_pin_and_delete(void) {
  WhelkClipboard *clipboard = whelk_clipboard_new(50);

  whelk_clipboard_add(clipboard, "text/plain", "one",
                      g_bytes_new_static("one", 3));
  whelk_clipboard_add(clipboard, "text/plain", "two",
                      g_bytes_new_static("two", 3));

  g_assert_true(whelk_clipboard_pin(clipboard, 1, TRUE));
  g_assert_true(
      ((WhelkClipboardEntry *)g_ptr_array_index(
           whelk_clipboard_entries(clipboard), 1))
          ->pinned);

  g_assert_true(whelk_clipboard_pin(clipboard, 1, FALSE));
  g_assert_false(
      ((WhelkClipboardEntry *)g_ptr_array_index(
           whelk_clipboard_entries(clipboard), 1))
          ->pinned);

  g_assert_true(whelk_clipboard_remove(clipboard, 0));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 1);
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(
           whelk_clipboard_entries(clipboard), 0))
          ->text,
      ==, "one");
  g_assert_false(whelk_clipboard_remove(clipboard, 7));

  g_object_unref(clipboard);
}

static void test_persistence_roundtrip(void) {
  g_autofree char *path =
      g_build_filename(g_get_tmp_dir(), "whelk-clipboard-test.variant", NULL);
  const guint8 image[] = {0x89, 'P', 'N', 'G', 0x00, 0xff};

  g_unlink(path);

  WhelkClipboard *first = whelk_clipboard_new(50);

  g_assert_false(whelk_clipboard_set_persistence(first, path));

  whelk_clipboard_add(first, "image/png", "shot",
                      g_bytes_new(image, sizeof image));
  whelk_clipboard_pin(first, 0, TRUE);
  whelk_clipboard_add(first, "text/html", "hello",
                      g_bytes_new_static("<b>hello</b>", 12));

  whelk_clipboard_flush(first);
  g_object_unref(first);

  g_assert_true(g_file_test(path, G_FILE_TEST_EXISTS));

  GStatBuf stat_buf;

  g_assert_cmpint(g_stat(path, &stat_buf), ==, 0);
  g_assert_cmpuint(stat_buf.st_mode & 0777, ==, 0600);

  WhelkClipboard *second = whelk_clipboard_new(50);

  g_assert_true(whelk_clipboard_set_persistence(second, path));
  g_assert_cmpuint(whelk_clipboard_count(second), ==, 2);

  WhelkClipboardEntry *pinned =
      g_ptr_array_index(whelk_clipboard_entries(second), 1);

  g_assert_cmpstr(pinned->mime, ==, "image/png");
  g_assert_true(pinned->pinned);
  g_assert_cmpstr(pinned->text, ==, "shot");

  gsize size = 0;
  const guint8 *bytes = g_bytes_get_data(pinned->data, &size);

  g_assert_cmpuint(size, ==, sizeof image);
  g_assert_cmpint(memcmp(bytes, image, sizeof image), ==, 0);

  g_object_unref(second);
  g_unlink(path);
}

static void test_persistence_recovers_from_garbage(void) {
  g_autofree char *path = g_build_filename(
      g_get_tmp_dir(), "whelk-clipboard-garbage.variant", NULL);

  g_assert_true(g_file_set_contents(path, "not a variant", -1, NULL));

  WhelkClipboard *clipboard = whelk_clipboard_new(50);

  g_assert_false(whelk_clipboard_set_persistence(clipboard, path));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 0);

  whelk_clipboard_add(clipboard, "text/plain", "one",
                      g_bytes_new_static("one", 3));
  whelk_clipboard_flush(clipboard);

  GStatBuf stat_buf;

  g_assert_cmpint(g_stat(path, &stat_buf), ==, 0);
  g_assert_cmpuint(stat_buf.st_mode & 0777, ==, 0600);

  /* A file that was already there must be made private again on the next
   * write, not just the first one. */
  g_assert_cmpint(g_chmod(path, 0644), ==, 0);
  whelk_clipboard_add(clipboard, "text/plain", "two",
                      g_bytes_new_static("two", 3));
  whelk_clipboard_flush(clipboard);
  g_assert_cmpint(g_stat(path, &stat_buf), ==, 0);
  g_assert_cmpuint(stat_buf.st_mode & 0777, ==, 0600);

  g_object_unref(clipboard);
  g_unlink(path);
}

static void test_pinned_only_persistence(void) {
  g_autofree char *path = g_build_filename(
      g_get_tmp_dir(), "whelk-clipboard-pinned.variant", NULL);

  g_unlink(path);

  WhelkClipboard *first = whelk_clipboard_new(50);

  g_assert_false(whelk_clipboard_set_persistence(first, path));
  whelk_clipboard_set_persist_pinned_only(first, TRUE);
  whelk_clipboard_add(first, "text/plain", "ordinary",
                      g_bytes_new_static("ordinary", 8));
  whelk_clipboard_add(first, "text/plain", "keep me",
                      g_bytes_new_static("keep me", 7));
  whelk_clipboard_pin(first, 0, TRUE);
  whelk_clipboard_flush(first);
  g_object_unref(first);

  WhelkClipboard *second = whelk_clipboard_new(50);

  g_assert_true(whelk_clipboard_set_persistence(second, path));
  g_assert_cmpuint(whelk_clipboard_count(second), ==, 1);
  g_assert_cmpstr(
      ((WhelkClipboardEntry *)g_ptr_array_index(
           whelk_clipboard_entries(second), 0))
          ->text,
      ==, "keep me");

  g_object_unref(second);
  g_unlink(path);
}

/* --- the fake tools -------------------------------------------------------
 * wl-paste and wl-copy are symlinks to this test binary.  Their whole job is
 * to answer from FAKE_* environment variables and, for wl-copy, record the
 * arguments it was handed. */

static int fake_paste_main(int argc, char **argv) {
  gboolean list = FALSE;
  const char *mime = NULL;

  for (int i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--list-types") == 0)
      list = TRUE;
    else if (strcmp(argv[i], "--type") == 0 && i + 1 < argc)
      mime = argv[++i];
  }

  const char *value = NULL;

  if (list)
    value = g_getenv("FAKE_TYPES");
  else if (mime && strcmp(mime, "image/png") == 0)
    value = g_getenv("FAKE_IMAGE");
  else if (mime && strcmp(mime, "text/html") == 0)
    value = g_getenv("FAKE_HTML");
  else if (mime && g_str_has_prefix(mime, "text/plain"))
    value = g_getenv("FAKE_TEXT");

  if (value) {
    fputs(value, stdout);
    fflush(stdout);
  }

  return 0;
}

static int fake_copy_main(int argc, char **argv) {
  const char *log = g_getenv("FAKE_LOG");

  if (log && *log) {
    FILE *file = fopen(log, "a");

    if (file) {
      for (int i = 1; i < argc; i++)
        fprintf(file, "%s%s", i > 1 ? " " : "", argv[i]);
      fputc('\n', file);
      fclose(file);
    }
  }

  char sink[4096];

  while (read(STDIN_FILENO, sink, sizeof sink) > 0)
    ;

  return 0;
}

static void test_legacy_persistence_loads(void) {
  g_autofree char *path = g_build_filename(
      g_get_tmp_dir(), "whelk-clipboard-legacy.variant", NULL);
  const guint8 bytes[] = {'o', 'l', 'd'};
  GVariantBuilder entries;

  g_variant_builder_init(&entries, G_VARIANT_TYPE("a(ssayb)"));
  g_variant_builder_add(
      &entries, "(ss@ayb)", "text/plain", "legacy",
      g_variant_new_fixed_array(G_VARIANT_TYPE_BYTE, bytes, sizeof bytes, 1),
      TRUE);

  g_autoptr(GVariant) root =
      g_variant_ref_sink(g_variant_new("(s@a(ssayb))", "whelk-clipboard-1",
                                       g_variant_builder_end(&entries)));
  gsize size = 0;
  const char *data = g_variant_get_data(root);

  size = g_variant_get_size(root);
  g_assert_true(g_file_set_contents(path, data, (gssize)size, NULL));

  WhelkClipboard *clipboard = whelk_clipboard_new(50);

  g_assert_true(whelk_clipboard_set_persistence(clipboard, path));
  g_assert_cmpuint(whelk_clipboard_count(clipboard), ==, 1);

  WhelkClipboardEntry *entry =
      g_ptr_array_index(whelk_clipboard_entries(clipboard), 0);

  g_assert_cmpstr(entry->text, ==, "legacy");
  g_assert_true(entry->pinned);
  g_assert_nonnull(entry->id);

  g_object_unref(clipboard);
  g_unlink(path);
}

int main(int argc, char **argv) {
  const char *name = strrchr(argv[0], '/');

  name = name ? name + 1 : argv[0];

  if (strcmp(name, "wl-paste") == 0)
    return fake_paste_main(argc, argv);
  if (strcmp(name, "wl-copy") == 0)
    return fake_copy_main(argc, argv);

  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/clipboard/newest-first", test_newest_first);
  g_test_add_func("/clipboard/repeat-moves-up",
                  test_repeat_moves_up_and_keeps_pin);
  g_test_add_func("/clipboard/images-dedupe", test_images_dedupe_by_bytes);
  g_test_add_func("/clipboard/text-spellings", test_plain_text_spellings_share_an_id);
  g_test_add_func("/clipboard/ttl-and-undo", test_ttl_and_undo);
  g_test_add_func("/clipboard/limits", test_limits);
  g_test_add_func("/clipboard/pinned-survives-cap",
                  test_pinned_survives_the_cap);
  g_test_add_func("/clipboard/default-cap", test_zero_means_the_default_cap);
  g_test_add_func("/clipboard/secret-hint", test_secret_hint);
  g_test_add_func("/clipboard/pick-mime", test_pick_mime);
  g_test_add_func("/clipboard/parse-partial", test_parse_waits_for_a_whole_record);
  g_test_add_func("/clipboard/parse-partial-header",
                  test_parse_waits_for_the_header);
  g_test_add_func("/clipboard/parse-two", test_parse_two_records);
  g_test_add_func("/clipboard/pin-and-delete", test_pin_and_delete);
  g_test_add_func("/clipboard/persistence", test_persistence_roundtrip);
  g_test_add_func("/clipboard/persistence-pinned",
                  test_pinned_only_persistence);
  g_test_add_func("/clipboard/persistence-garbage",
                  test_persistence_recovers_from_garbage);
  g_test_add_func("/clipboard/persistence-legacy",
                  test_legacy_persistence_loads);
  g_test_add_func("/clipboard/helper/image",
                  test_helper_selects_image_and_frames_it);
  g_test_add_func("/clipboard/helper/html-plain",
                  test_helper_keeps_plain_rendering_of_html);
  g_test_add_func("/clipboard/helper/secret", test_helper_refuses_password_hint);
  g_test_add_func("/clipboard/helper/oversize",
                  test_helper_refuses_oversized_source);
  g_test_add_func("/clipboard/restore/mime", test_restore_uses_the_selected_mime);

  return g_test_run();
}

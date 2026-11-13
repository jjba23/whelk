// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "clipboard.h"

#include <glib/gstdio.h>
#include <string.h>

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

  g_assert_cmpstr(rich, ==, "image/png");
  g_assert_cmpstr(text_only, ==, "text/plain;charset=utf-8");
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

  GStatBuf stat_buf;

  g_assert_cmpint(g_stat(path, &stat_buf), ==, 0);
  g_assert_cmpuint(stat_buf.st_mode & 0777, ==, 0600);

  /* A file that was already there must be made private again on the next
   * write, not just the first one. */
  g_assert_cmpint(g_chmod(path, 0644), ==, 0);
  whelk_clipboard_add(clipboard, "text/plain", "two",
                      g_bytes_new_static("two", 3));
  g_assert_cmpint(g_stat(path, &stat_buf), ==, 0);
  g_assert_cmpuint(stat_buf.st_mode & 0777, ==, 0600);

  g_object_unref(clipboard);
  g_unlink(path);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/clipboard/newest-first", test_newest_first);
  g_test_add_func("/clipboard/repeat-moves-up",
                  test_repeat_moves_up_and_keeps_pin);
  g_test_add_func("/clipboard/images-dedupe", test_images_dedupe_by_bytes);
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
  g_test_add_func("/clipboard/persistence-garbage",
                  test_persistence_recovers_from_garbage);

  return g_test_run();
}

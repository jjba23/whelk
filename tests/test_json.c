// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/json.h"

#include <glib.h>

/* The shapes below are the ones niri actually answers with:
 *   niri msg -j focused-window  -> {"id":3,"title":"...","app_id":"..."}
 *   niri msg -j workspaces      -> [{"id":1,"idx":1,...,"active_window_id":null}]
 */

static void test_object(void) {
  const char *document =
      "{\"id\": 7, \"title\": \"a \\\"quoted\\\" caf\\u00e9\", \"ok\": true,"
      " \"none\": null, \"list\": [1, 2, {}]}";
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(document, &error);

  g_assert_no_error(error);
  g_assert_nonnull(root);
  g_assert_cmpint(whelk_json_type(root), ==, WHELK_JSON_OBJECT);

  g_assert_cmpfloat(whelk_json_object_number(root, "id", -1), ==, 7.0);
  g_assert_cmpstr(whelk_json_object_string(root, "title"), ==,
                  "a \"quoted\" café");
  g_assert_true(whelk_json_object_bool(root, "ok", FALSE));
  g_assert_cmpint(whelk_json_type(whelk_json_object_get(root, "none")), ==,
                  WHELK_JSON_NULL);

  WhelkJson *list = whelk_json_object_get(root, "list");
  g_assert_cmpuint(whelk_json_array_length(list), ==, 3);
  g_assert_cmpfloat(whelk_json_number(whelk_json_array_get(list, 1)), ==, 2.0);
  g_assert_cmpint(whelk_json_type(whelk_json_array_get(list, 2)), ==,
                  WHELK_JSON_OBJECT);

  /* A missing key is a fallback, not a crash. */
  g_assert_cmpstr(whelk_json_object_string(root, "absent"), ==, NULL);
  g_assert_false(whelk_json_object_bool(root, "absent", FALSE));
  g_assert_cmpfloat(whelk_json_object_number(root, "absent", 4.5), ==, 4.5);

  whelk_json_free(root);
}

static void test_workspaces(void) {
  const char *document =
      "[{\"id\":1,\"idx\":1,\"name\":\"one\",\"is_active\":true,"
      "\"is_focused\":true,\"active_window_id\":12},"
      "{\"id\":2,\"idx\":2,\"name\":null,\"is_active\":false,"
      "\"is_focused\":false,\"active_window_id\":null}]";
  g_autoptr(GError) error = NULL;
  WhelkJson *root = whelk_json_parse(document, &error);

  g_assert_no_error(error);
  g_assert_cmpuint(whelk_json_array_length(root), ==, 2);

  WhelkJson *first = whelk_json_array_get(root, 0);
  g_assert_cmpfloat(whelk_json_object_number(first, "idx", -1), ==, 1.0);
  g_assert_true(whelk_json_object_bool(first, "is_active", FALSE));
  g_assert_cmpint(whelk_json_type(whelk_json_object_get(first,
                                                        "active_window_id")),
                  ==, WHELK_JSON_NUMBER);

  WhelkJson *second = whelk_json_array_get(root, 1);
  g_assert_cmpint(whelk_json_type(whelk_json_object_get(second,
                                                        "active_window_id")),
                  ==, WHELK_JSON_NULL);

  whelk_json_free(root);
}

static void test_invalid_documents(void) {
  g_autoptr(GError) error = NULL;

  g_assert_null(whelk_json_parse("{\"a\": 1", &error));
  g_assert_nonnull(error);

  g_clear_error(&error);
  g_assert_null(whelk_json_parse("", &error));
  g_assert_nonnull(error);

  g_clear_error(&error);
  g_assert_null(whelk_json_parse("{\"a\": 1} trailing", &error));
  g_assert_nonnull(error);
}

int main(int argc, char **argv) {
  g_test_init(&argc, &argv, NULL);

  g_test_add_func("/whelk/json/object", test_object);
  g_test_add_func("/whelk/json/workspaces", test_workspaces);
  g_test_add_func("/whelk/json/invalid", test_invalid_documents);

  return g_test_run();
}

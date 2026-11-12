// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/json.h"

#include <string.h>

struct _WhelkJson {
  WhelkJsonType type;
  gboolean boolean;
  double number;
  char *string;
  GPtrArray *items; /* array values, or object values in key order */
  GPtrArray *keys;  /* object keys, NULL for arrays */
};

/* =========================================================================
 * NODE CONSTRUCTION
 * ========================================================================= */

static WhelkJson *node_new(WhelkJsonType type) {
  WhelkJson *node = g_new0(WhelkJson, 1);

  node->type = type;
  return node;
}

void whelk_json_free(WhelkJson *node) {
  if (!node)
    return;

  g_free(node->string);
  g_clear_pointer(&node->items, g_ptr_array_unref);
  g_clear_pointer(&node->keys, g_ptr_array_unref);
  g_free(node);
}

/* =========================================================================
 * PARSER
 * ========================================================================= */

typedef struct {
  const char *start;
  const char *cursor;
  GError **error;
  gboolean failed;
} Parser;

static void fail(Parser *parser, const char *message) {
  if (parser->failed)
    return;

  parser->failed = TRUE;
  g_set_error(parser->error, G_MARKUP_ERROR, G_MARKUP_ERROR_INVALID_CONTENT,
              "%s at offset %ld", message, (long)(parser->cursor - parser->start));
}

static void skip_whitespace(Parser *parser) {
  while (*parser->cursor == ' ' || *parser->cursor == '\t' ||
         *parser->cursor == '\n' || *parser->cursor == '\r')
    parser->cursor++;
}

static WhelkJson *parse_value(Parser *parser);

/* Appends one code point as UTF-8. */
static void append_codepoint(GString *out, guint32 codepoint) {
  if (codepoint > 0x10FFFF || (codepoint >= 0xD800 && codepoint <= 0xDFFF))
    codepoint = 0xFFFD;

  if (codepoint < 0x80) {
    g_string_append_c(out, (gchar)codepoint);
  } else if (codepoint < 0x800) {
    g_string_append_c(out, (gchar)(0xC0 | (codepoint >> 6)));
    g_string_append_c(out, (gchar)(0x80 | (codepoint & 0x3F)));
  } else if (codepoint < 0x10000) {
    g_string_append_c(out, (gchar)(0xE0 | (codepoint >> 12)));
    g_string_append_c(out, (gchar)(0x80 | ((codepoint >> 6) & 0x3F)));
    g_string_append_c(out, (gchar)(0x80 | (codepoint & 0x3F)));
  } else {
    g_string_append_c(out, (gchar)(0xF0 | (codepoint >> 18)));
    g_string_append_c(out, (gchar)(0x80 | ((codepoint >> 12) & 0x3F)));
    g_string_append_c(out, (gchar)(0x80 | ((codepoint >> 6) & 0x3F)));
    g_string_append_c(out, (gchar)(0x80 | (codepoint & 0x3F)));
  }
}

static gboolean parse_hex4(Parser *parser, guint32 *out) {
  guint32 value = 0;

  for (int i = 0; i < 4; i++) {
    const gchar c = parser->cursor[i];
    int digit;

    if (c >= '0' && c <= '9')
      digit = c - '0';
    else if (c >= 'a' && c <= 'f')
      digit = 10 + (c - 'a');
    else if (c >= 'A' && c <= 'F')
      digit = 10 + (c - 'A');
    else
      return FALSE;

    value = (value << 4) | (guint32)digit;
  }

  parser->cursor += 4;
  *out = value;
  return TRUE;
}

static char *parse_string_literal(Parser *parser) {
  if (*parser->cursor != '"') {
    fail(parser, "Expected a string");
    return NULL;
  }

  parser->cursor++;

  GString *out = g_string_new(NULL);

  while (*parser->cursor != '"') {
    if (*parser->cursor == '\0') {
      g_string_free(out, TRUE);
      fail(parser, "Unterminated string");
      return NULL;
    }

    if (*parser->cursor != '\\') {
      g_string_append_c(out, *parser->cursor++);
      continue;
    }

    parser->cursor++;
    switch (*parser->cursor) {
    case '"':
      g_string_append_c(out, '"');
      break;
    case '\\':
      g_string_append_c(out, '\\');
      break;
    case '/':
      g_string_append_c(out, '/');
      break;
    case 'b':
      g_string_append_c(out, '\b');
      break;
    case 'f':
      g_string_append_c(out, '\f');
      break;
    case 'n':
      g_string_append_c(out, '\n');
      break;
    case 'r':
      g_string_append_c(out, '\r');
      break;
    case 't':
      g_string_append_c(out, '\t');
      break;
    case 'u': {
      parser->cursor++;
      guint32 codepoint = 0;

      if (!parse_hex4(parser, &codepoint)) {
        g_string_free(out, TRUE);
        fail(parser, "Invalid \\u escape");
        return NULL;
      }

      /* A surrogate pair: the low half follows as its own escape. */
      if (codepoint >= 0xD800 && codepoint <= 0xDBFF &&
          parser->cursor[0] == '\\' && parser->cursor[1] == 'u') {
        const char *save = parser->cursor;
        guint32 low = 0;

        parser->cursor += 2;
        if (parse_hex4(parser, &low) && low >= 0xDC00 && low <= 0xDFFF)
          codepoint =
              0x10000 + ((codepoint - 0xD800) << 10) + (low - 0xDC00);
        else
          parser->cursor = save;
      }

      append_codepoint(out, codepoint);
      continue; /* the switch already consumed the escape */
    }
    default:
      g_string_free(out, TRUE);
      fail(parser, "Invalid escape");
      return NULL;
    }

    parser->cursor++;
  }

  parser->cursor++; /* closing quote */
  return g_string_free(out, FALSE);
}

static WhelkJson *parse_object(Parser *parser) {
  WhelkJson *node = node_new(WHELK_JSON_OBJECT);

  node->items = g_ptr_array_new_with_free_func((GDestroyNotify)whelk_json_free);
  node->keys = g_ptr_array_new_with_free_func(g_free);

  parser->cursor++; /* '{' */
  skip_whitespace(parser);

  if (*parser->cursor == '}') {
    parser->cursor++;
    return node;
  }

  for (;;) {
    skip_whitespace(parser);

    char *key = parse_string_literal(parser);
    if (!key) {
      whelk_json_free(node);
      return NULL;
    }
    g_ptr_array_add(node->keys, key);

    skip_whitespace(parser);
    if (*parser->cursor != ':') {
      fail(parser, "Expected ':'");
      whelk_json_free(node);
      return NULL;
    }
    parser->cursor++;

    WhelkJson *value = parse_value(parser);
    if (!value) {
      whelk_json_free(node);
      return NULL;
    }
    g_ptr_array_add(node->items, value);

    skip_whitespace(parser);
    if (*parser->cursor == ',') {
      parser->cursor++;
      continue;
    }
    if (*parser->cursor == '}') {
      parser->cursor++;
      return node;
    }

    fail(parser, "Expected ',' or '}'");
    whelk_json_free(node);
    return NULL;
  }
}

static WhelkJson *parse_array(Parser *parser) {
  WhelkJson *node = node_new(WHELK_JSON_ARRAY);

  node->items = g_ptr_array_new_with_free_func((GDestroyNotify)whelk_json_free);

  parser->cursor++; /* '[' */
  skip_whitespace(parser);

  if (*parser->cursor == ']') {
    parser->cursor++;
    return node;
  }

  for (;;) {
    WhelkJson *value = parse_value(parser);
    if (!value) {
      whelk_json_free(node);
      return NULL;
    }
    g_ptr_array_add(node->items, value);

    skip_whitespace(parser);
    if (*parser->cursor == ',') {
      parser->cursor++;
      continue;
    }
    if (*parser->cursor == ']') {
      parser->cursor++;
      return node;
    }

    fail(parser, "Expected ',' or ']'");
    whelk_json_free(node);
    return NULL;
  }
}

static WhelkJson *parse_number(Parser *parser) {
  const char *begin = parser->cursor;
  char *end = NULL;
  const double value = g_ascii_strtod(begin, &end);

  if (end == begin) {
    fail(parser, "Invalid number");
    return NULL;
  }

  parser->cursor = end;

  WhelkJson *node = node_new(WHELK_JSON_NUMBER);
  node->number = value;
  return node;
}

static WhelkJson *parse_value(Parser *parser) {
  skip_whitespace(parser);

  switch (*parser->cursor) {
  case '{':
    return parse_object(parser);
  case '[':
    return parse_array(parser);
  case '"': {
    char *text = parse_string_literal(parser);
    if (!text)
      return NULL;

    WhelkJson *node = node_new(WHELK_JSON_STRING);
    node->string = text;
    return node;
  }
  case 't':
    if (g_str_has_prefix(parser->cursor, "true")) {
      parser->cursor += 4;
      WhelkJson *node = node_new(WHELK_JSON_BOOL);
      node->boolean = TRUE;
      return node;
    }
    break;
  case 'f':
    if (g_str_has_prefix(parser->cursor, "false")) {
      parser->cursor += 5;
      WhelkJson *node = node_new(WHELK_JSON_BOOL);
      node->boolean = FALSE;
      return node;
    }
    break;
  case 'n':
    if (g_str_has_prefix(parser->cursor, "null")) {
      parser->cursor += 4;
      return node_new(WHELK_JSON_NULL);
    }
    break;
  default:
    if (*parser->cursor == '-' || g_ascii_isdigit(*parser->cursor))
      return parse_number(parser);
    break;
  }

  fail(parser, "Unexpected character");
  return NULL;
}

WhelkJson *whelk_json_parse(const char *text, GError **error) {
  if (!text) {
    g_set_error_literal(error, G_MARKUP_ERROR, G_MARKUP_ERROR_INVALID_CONTENT,
                        "No document to parse");
    return NULL;
  }

  Parser parser = {text, text, error, FALSE};
  WhelkJson *root = parse_value(&parser);

  if (!root)
    return NULL;

  skip_whitespace(&parser);
  if (*parser.cursor != '\0') {
    fail(&parser, "Trailing content");
    whelk_json_free(root);
    return NULL;
  }

  return root;
}

/* =========================================================================
 * ACCESSORS
 * ========================================================================= */

WhelkJsonType whelk_json_type(const WhelkJson *node) {
  return node ? node->type : WHELK_JSON_NULL;
}

const char *whelk_json_string(const WhelkJson *node) {
  if (!node || node->type != WHELK_JSON_STRING)
    return NULL;

  return node->string;
}

double whelk_json_number(const WhelkJson *node) {
  if (!node || node->type != WHELK_JSON_NUMBER)
    return 0.0;

  return node->number;
}

gboolean whelk_json_bool(const WhelkJson *node) {
  if (!node || node->type != WHELK_JSON_BOOL)
    return FALSE;

  return node->boolean;
}

guint whelk_json_array_length(const WhelkJson *node) {
  if (!node || !node->items)
    return 0;

  return node->items->len;
}

WhelkJson *whelk_json_array_get(const WhelkJson *node, guint index) {
  if (!node || node->type != WHELK_JSON_ARRAY || !node->items ||
      index >= node->items->len)
    return NULL;

  return g_ptr_array_index(node->items, index);
}

WhelkJson *whelk_json_object_get(const WhelkJson *node, const char *key) {
  if (!node || node->type != WHELK_JSON_OBJECT || !node->keys || !key)
    return NULL;

  for (guint i = 0; i < node->keys->len && i < node->items->len; i++) {
    const char *candidate = g_ptr_array_index(node->keys, i);

    if (g_strcmp0(candidate, key) == 0)
      return g_ptr_array_index(node->items, i);
  }

  return NULL;
}

const char *whelk_json_object_string(const WhelkJson *node, const char *key) {
  return whelk_json_string(whelk_json_object_get(node, key));
}

gboolean whelk_json_object_bool(const WhelkJson *node, const char *key,
                                gboolean fallback) {
  WhelkJson *value = whelk_json_object_get(node, key);

  if (!value || whelk_json_type(value) != WHELK_JSON_BOOL)
    return fallback;

  return whelk_json_bool(value);
}

double whelk_json_object_number(const WhelkJson *node, const char *key,
                                double fallback) {
  WhelkJson *value = whelk_json_object_get(node, key);

  if (!value || whelk_json_type(value) != WHELK_JSON_NUMBER)
    return fallback;

  return whelk_json_number(value);
}

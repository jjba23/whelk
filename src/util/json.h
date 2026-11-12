// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_JSON_H
#define WHELK_JSON_H

#include <glib.h>

/* =========================================================================
 * A SMALL JSON READER
 * =========================================================================
 * niri's IPC answers ("niri msg -j workspaces") are JSON, and the shell has no
 * JSON library to lean on: adding one for three fields would be a dependency
 * the build does not otherwise need.  This reader is deliberately small -- the
 * subset an IPC answer uses -- and deliberately borrowed-only: a node owns its
 * children and dies with its root, and lookups hand back children without a
 * reference to drop.
 */

typedef enum {
  WHELK_JSON_NULL = 0,
  WHELK_JSON_BOOL,
  WHELK_JSON_NUMBER,
  WHELK_JSON_STRING,
  WHELK_JSON_ARRAY,
  WHELK_JSON_OBJECT,
} WhelkJsonType;

typedef struct _WhelkJson WhelkJson;

/**
 * whelk_json_parse:
 * @text: the document.
 * @error: (out) (optional): set when @text is not valid JSON.
 *
 * Returns: (transfer full) (nullable): the document's root node.
 */
WhelkJson *whelk_json_parse(const char *text, GError **error);

/* Frees a root node and everything under it.  Safe on NULL. */
void whelk_json_free(WhelkJson *node);

WhelkJsonType whelk_json_type(const WhelkJson *node);

/* Scalars.  NULL and type mismatches answer with the zero value / NULL. */
const char *whelk_json_string(const WhelkJson *node);
double whelk_json_number(const WhelkJson *node);
gboolean whelk_json_bool(const WhelkJson *node);

/* Containers.  Children are borrowed and live as long as the root. */
guint whelk_json_array_length(const WhelkJson *node);
WhelkJson *whelk_json_array_get(const WhelkJson *node, guint index);
WhelkJson *whelk_json_object_get(const WhelkJson *node, const char *key);

/* Convenience: the string at @key of an object, or NULL. */
const char *whelk_json_object_string(const WhelkJson *node, const char *key);
/* Convenience: the bool at @key of an object, or @fallback. */
gboolean whelk_json_object_bool(const WhelkJson *node, const char *key,
                                gboolean fallback);
/* Convenience: the number at @key of an object, or @fallback. */
double whelk_json_object_number(const WhelkJson *node, const char *key,
                                double fallback);

#endif // WHELK_JSON_H

// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#include "util/calc.h"

#include <math.h>
#include <string.h>

/* Recursive descent over the grammar:
 *
 *   expression := term (('+' | '-') term)*
 *   term       := power (('*' | '/' | '%') power)*
 *   power      := unary ('^' power)?            (right associative)
 *   unary      := ('-' | '+') unary | primary
 *   primary    := number | '(' expression ')'
 */

typedef struct {
  const char *cursor;
  gboolean failed;
} CalcParser;

static double parse_expression(CalcParser *parser);

static void skip_spaces(CalcParser *parser) {
  while (g_ascii_isspace(*parser->cursor))
    parser->cursor++;
}

static double parse_primary(CalcParser *parser) {
  skip_spaces(parser);

  if (*parser->cursor == '(') {
    parser->cursor++;
    const double value = parse_expression(parser);

    skip_spaces(parser);
    if (*parser->cursor != ')') {
      parser->failed = TRUE;
      return 0.0;
    }
    parser->cursor++;

    return value;
  }

  char *end = NULL;
  const double value = g_ascii_strtod(parser->cursor, &end);

  if (end == parser->cursor) {
    parser->failed = TRUE;
    return 0.0;
  }

  parser->cursor = end;
  return value;
}

static double parse_unary(CalcParser *parser) {
  skip_spaces(parser);

  if (*parser->cursor == '-') {
    parser->cursor++;
    return -parse_unary(parser);
  }

  if (*parser->cursor == '+') {
    parser->cursor++;
    return parse_unary(parser);
  }

  return parse_primary(parser);
}

static double parse_power(CalcParser *parser) {
  const double base = parse_unary(parser);

  skip_spaces(parser);

  if (*parser->cursor == '^') {
    parser->cursor++;
    return pow(base, parse_power(parser));
  }

  return base;
}

static double parse_term(CalcParser *parser) {
  double value = parse_power(parser);

  for (;;) {
    skip_spaces(parser);

    const char operator= *parser->cursor;
    if (operator!= '*' && operator!= '/' && operator!= '%')
      break;

    parser->cursor++;
    const double right = parse_power(parser);

    if ((operator== '/' || operator== '%') && right == 0.0) {
      parser->failed = TRUE;
      return 0.0;
    }

    if (operator== '*')
      value *= right;
    else if (operator== '/')
      value /= right;
    else
      value = fmod(value, right);
  }

  return value;
}

static double parse_expression(CalcParser *parser) {
  double value = parse_term(parser);

  for (;;) {
    skip_spaces(parser);

    if (*parser->cursor == '+') {
      parser->cursor++;
      value += parse_term(parser);
    } else if (*parser->cursor == '-') {
      parser->cursor++;
      value -= parse_term(parser);
    } else if (*parser->cursor == '=') {
      /* A typed trailing '=' is a full stop, not an error. */
      parser->cursor++;
    } else {
      break;
    }
  }

  return value;
}

gboolean whelk_calc_eval(const char *expression, double *out) {
  if (!out)
    return FALSE;

  *out = 0.0;

  if (!expression || g_strstrip(g_strdup(expression)) == NULL)
    return FALSE;

  /* A bare word is not arithmetic: refuse it here rather than let strtod
   * accept the leading number of something like "2x". */
  for (const char *p = expression; *p; p++) {
    if (!g_ascii_isdigit(*p) && !g_ascii_isspace(*p) &&
        !strchr("+-*/%^().,=", *p))
      return FALSE;
  }

  CalcParser parser = {expression, FALSE};
  const double value = parse_expression(&parser);

  skip_spaces(&parser);

  if (parser.failed || *parser.cursor != '\0' || isnan(value) || isinf(value))
    return FALSE;

  *out = value;
  return TRUE;
}

char *whelk_calc_format(double value) {
  const double rounded = round(value);

  if (fabs(value - rounded) < 1e-9 && fabs(value) < 1e15)
    return g_strdup_printf("%.0f", rounded);

  g_autofree char *text = g_strdup_printf("%.10g", value);

  return g_strdup(text);
}

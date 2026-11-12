// whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

#ifndef WHELK_CALC_H
#define WHELK_CALC_H

#include <glib.h>

/* =========================================================================
 * THE CALCULATOR
 * =========================================================================
 * The launcher's `=' mode: an arithmetic expression evaluated as it is typed.
 * A launcher that can add is the difference between reaching for a terminal and
 * not, and the grammar is deliberately the one people type:
 *
 *   + - * / % ^   parentheses   unary minus   decimal numbers
 *
 * No variables, no functions, no units: an expression, a number.
 */

/**
 * whelk_calc_eval:
 * @expression: the text to evaluate.
 * @out: (out): the value, on success.
 *
 * Returns: TRUE when @expression is a complete arithmetic expression.
 */
gboolean whelk_calc_eval(const char *expression, double *out);

/**
 * whelk_calc_format:
 * Returns: (transfer full): @value without a trailing ".0" for whole numbers,
 * which is how a calculator answer is read.
 */
char *whelk_calc_format(double value);

#endif // WHELK_CALC_H

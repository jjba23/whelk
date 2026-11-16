// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's calculator, ported from `util/calc.c`.
//!
//! `=` mode evaluates an arithmetic expression as it is typed.  The grammar is
//! deliberately the one people type:
//!
//! ```text
//! expression := term (('+' | '-') term)*
//! term       := power (('*' | '/' | '%') power)*
//! power      := unary ('^' power)?            (right associative)
//! unary      := ('-' | '+') unary | primary
//! primary    := number | '(' expression ')'
//! ```
//!
//! No variables, no functions, no units: an expression, a number.

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip_spaces(&mut self) {
        let bytes = self.text.as_bytes();

        while self.at < bytes.len() && bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_spaces();

        self.text.as_bytes().get(self.at).copied()
    }

    fn primary(&mut self) -> Option<f64> {
        if self.peek()? == b'(' {
            self.at += 1;

            let value = self.expression()?;

            if self.peek()? != b')' {
                return None;
            }

            self.at += 1;

            return Some(value);
        }

        self.number()
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_spaces();

        let start = self.at;
        let bytes = self.text.as_bytes();

        while self.at < bytes.len() && (bytes[self.at].is_ascii_digit() || bytes[self.at] == b'.') {
            self.at += 1;
        }

        if self.at == start {
            return None;
        }

        self.text[start..self.at].parse().ok()
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'-' => {
                self.at += 1;

                Some(-self.unary()?)
            }
            b'+' => {
                self.at += 1;

                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;

        if self.peek() == Some(b'^') {
            self.at += 1;

            return Some(base.powf(self.power()?));
        }

        Some(base)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.power()?;

        while let Some(operator @ (b'*' | b'/' | b'%')) = self.peek() {
            self.at += 1;

            let right = self.power()?;

            if (operator == b'/' || operator == b'%') && right == 0.0 {
                return None;
            }

            match operator {
                b'*' => value *= right,
                b'/' => value /= right,
                _ => value %= right,
            }
        }

        Some(value)
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;

        loop {
            match self.peek() {
                Some(b'+') => {
                    self.at += 1;

                    value += self.term()?;
                }
                Some(b'-') => {
                    self.at += 1;

                    value -= self.term()?;
                }
                // A typed trailing '=' is a full stop, not an error.
                Some(b'=') => {
                    self.at += 1;
                }
                _ => break,
            }
        }

        Some(value)
    }
}

/// Whether every character could belong to an arithmetic expression.
///
/// A bare word is not arithmetic: refusing it here, rather than letting the
/// number scanner accept the leading digits of something like `2x`, is what
/// makes `2x` a refusal instead of `2`.
fn is_arithmetic(expression: &str) -> bool {
    expression.chars().all(|character| {
        character.is_ascii_digit()
            || character.is_ascii_whitespace()
            || "+-*/%^().,=".contains(character)
    })
}

/// Evaluates `expression`, or `None` when it is not a complete arithmetic
/// expression.
pub fn eval(expression: &str) -> Option<f64> {
    if !is_arithmetic(expression) {
        return None;
    }

    let mut parser = Parser {
        text: expression,
        at: 0,
    };

    let value = parser.expression()?;

    parser.skip_spaces();

    if parser.at != expression.len() || !value.is_finite() {
        return None;
    }

    Some(value)
}

/// `value` without a trailing `.0` for whole numbers, which is how a
/// calculator answer is read.
pub fn format(value: f64) -> String {
    let rounded = value.round();

    if (value - rounded).abs() < 1e-9 && value.abs() < 1e15 {
        return format!("{rounded:.0}");
    }

    format_significant(value, 10)
}

/// C's `%.10g`, which is what the C calculator prints.
fn format_significant(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }

    let exponent = value.abs().log10().floor() as i32;

    if exponent < -4 || exponent >= precision as i32 {
        let text = format!("{:.*e}", precision - 1, value);
        let Some((mantissa, exponent)) = text.split_once('e') else {
            return text;
        };
        let exponent: i32 = exponent.parse().unwrap_or(0);

        return format!(
            "{}e{}{:02}",
            trim_fraction(mantissa),
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
    }

    let decimals = (precision as i32 - 1 - exponent).max(0) as usize;

    trim_fraction(&format!("{value:.decimals$}"))
}

fn trim_fraction(text: &str) -> String {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{eval, format};

    #[test]
    fn a_trailing_equals_is_a_full_stop() {
        assert_eq!(eval("7="), Some(7.0));
        assert_eq!(eval("7==="), Some(7.0));
    }

    #[test]
    fn trailing_text_is_refused() {
        assert_eq!(eval("1=2"), None);
    }

    #[test]
    fn scientific_notation_matches_the_c_printer() {
        assert_eq!(format(1e-5), "1e-05");
        assert_eq!(format(1e20), "1e+20");
        assert_eq!(format(0.0001), "0.0001");
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A small JSON reader, ported from `util/json.c`.
//!
//! niri's IPC answers (`niri msg -j workspaces`) are JSON, and the shell has no
//! JSON library to lean on: adding one for three fields would be a dependency
//! the build does not otherwise need.  This reader is deliberately small -- the
//! subset an IPC answer uses -- and owns its tree: a [`Json`] value contains its
//! children, and lookups return references into it.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// Array values, in document order.
    Array(Vec<Json>),
    /// Object entries, in key order (duplicates are preserved; lookups return
    /// the first, like the C reader).
    Object(Vec<(String, Json)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at offset {}", self.message, self.offset)
    }
}

impl std::error::Error for JsonError {}

impl Json {
    pub fn parse(text: &str) -> Result<Self, JsonError> {
        let mut parser = Parser {
            input: text,
            bytes: text.as_bytes(),
            index: 0,
            error: None,
        };
        let Some(root) = parser.parse_value() else {
            return Err(parser
                .error
                .take()
                .unwrap_or_else(|| parser.make_error("Unexpected character")));
        };

        parser.skip_whitespace();

        if parser.index != parser.bytes.len() {
            return Err(parser.make_error("Trailing content"));
        }

        Ok(root)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// The array length, or 0 for anything else (`whelk_json_array_length`).
    pub fn len(&self) -> usize {
        self.array().map_or(0, |values| values.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn at(&self, index: usize) -> Option<&Json> {
        self.array()?.get(index)
    }

    /// The value at `key` of an object, or `None` for a missing key or a
    /// non-object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.object()?
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.get(key).and_then(Json::as_bool).unwrap_or(fallback)
    }

    pub fn get_number(&self, key: &str, fallback: f64) -> f64 {
        self.get(key).and_then(Json::as_f64).unwrap_or(fallback)
    }
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    index: usize,
    error: Option<JsonError>,
}

impl Parser<'_> {
    fn make_error(&self, message: &str) -> JsonError {
        JsonError {
            message: message.to_owned(),
            offset: self.index,
        }
    }

    fn fail<T>(&mut self, message: &str) -> Option<T> {
        if self.error.is_none() {
            self.error = Some(self.make_error(message));
        }

        None
    }

    fn byte(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.index += 1;
        }
    }

    fn parse_value(&mut self) -> Option<Json> {
        self.skip_whitespace();

        match self.byte()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_string_literal().map(Json::String),
            b't' if self.starts_with("true") => {
                self.index += 4;
                Some(Json::Bool(true))
            }
            b'f' if self.starts_with("false") => {
                self.index += 5;
                Some(Json::Bool(false))
            }
            b'n' if self.starts_with("null") => {
                self.index += 4;
                Some(Json::Null)
            }
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => self.fail("Unexpected character"),
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        self.input[self.index..].starts_with(text)
    }

    fn parse_object(&mut self) -> Option<Json> {
        self.index += 1; // '{'
        self.skip_whitespace();

        let mut entries = Vec::new();

        if self.byte() == Some(b'}') {
            self.index += 1;

            return Some(Json::Object(entries));
        }

        loop {
            self.skip_whitespace();

            let key = self.parse_string_literal()?;

            self.skip_whitespace();

            if self.byte() != Some(b':') {
                return self.fail("Expected ':'");
            }

            self.index += 1;

            let value = self.parse_value()?;

            entries.push((key, value));

            self.skip_whitespace();

            match self.byte() {
                Some(b',') => self.index += 1,
                Some(b'}') => {
                    self.index += 1;

                    return Some(Json::Object(entries));
                }
                _ => return self.fail("Expected ',' or '}'"),
            }
        }
    }

    fn parse_array(&mut self) -> Option<Json> {
        self.index += 1; // '['
        self.skip_whitespace();

        let mut values = Vec::new();

        if self.byte() == Some(b']') {
            self.index += 1;

            return Some(Json::Array(values));
        }

        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();

            match self.byte() {
                Some(b',') => self.index += 1,
                Some(b']') => {
                    self.index += 1;

                    return Some(Json::Array(values));
                }
                _ => return self.fail("Expected ',' or ']'"),
            }
        }
    }

    fn parse_number(&mut self) -> Option<Json> {
        let start = self.index;

        if self.byte() == Some(b'-') {
            self.index += 1;
        }

        while matches!(self.byte(), Some(b'0'..=b'9')) {
            self.index += 1;
        }

        if self.byte() == Some(b'.') {
            self.index += 1;

            while matches!(self.byte(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }

        if matches!(self.byte(), Some(b'e' | b'E')) {
            self.index += 1;

            if matches!(self.byte(), Some(b'+' | b'-')) {
                self.index += 1;
            }

            while matches!(self.byte(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }

        let token = &self.input[start..self.index];

        match token.parse::<f64>() {
            Ok(value) => Some(Json::Number(value)),
            Err(_) => {
                self.index = start;
                self.fail("Invalid number")
            }
        }
    }

    fn parse_string_literal(&mut self) -> Option<String> {
        if self.byte() != Some(b'"') {
            return self.fail("Expected a string");
        }

        self.index += 1;

        let mut out = String::new();

        loop {
            let Some(byte) = self.byte() else {
                return self.fail("Unterminated string");
            };

            if byte == b'"' {
                self.index += 1;

                return Some(out);
            }

            if byte != b'\\' {
                let Some(character) = self.input[self.index..].chars().next() else {
                    return self.fail("Unterminated string");
                };

                out.push(character);
                self.index += character.len_utf8();

                continue;
            }

            self.index += 1;

            let Some(escape) = self.byte() else {
                return self.fail("Invalid escape");
            };

            match escape {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{0008}'),
                b'f' => out.push('\u{000c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    self.index += 1;

                    let Some(mut codepoint) = self.parse_hex4() else {
                        return self.fail("Invalid \\u escape");
                    };

                    // A surrogate pair: the low half follows as its own escape.
                    if (0xD800..=0xDBFF).contains(&codepoint)
                        && self.bytes.get(self.index) == Some(&b'\\')
                        && self.bytes.get(self.index + 1) == Some(&b'u')
                    {
                        let save = self.index;

                        self.index += 2;

                        if let Some(low) = self
                            .parse_hex4()
                            .filter(|low| (0xDC00..=0xDFFF).contains(low))
                        {
                            codepoint = 0x10000 + ((codepoint - 0xD800) << 10) + (low - 0xDC00);
                        } else {
                            self.index = save;
                        }
                    }

                    out.push(char::from_u32(codepoint).unwrap_or('\u{FFFD}'));

                    continue;
                }
                _ => return self.fail("Invalid escape"),
            }

            self.index += 1;
        }
    }

    fn parse_hex4(&mut self) -> Option<u32> {
        let mut value = 0;

        for _ in 0..4 {
            let digit = match self.byte()? {
                b'0'..=b'9' => self.byte()? - b'0',
                b'a'..=b'f' => self.byte()? - b'a' + 10,
                b'A'..=b'F' => self.byte()? - b'A' + 10,
                _ => return None,
            };

            value = (value << 4) | u32::from(digit);
            self.index += 1;
        }

        Some(value)
    }
}

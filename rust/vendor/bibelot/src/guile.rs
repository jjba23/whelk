// bibelot -- a lock screen and session authenticator written in Rust, GTK4 and
// GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A small, safe Guile runtime for loading bibelot's real Scheme configuration.
//!
//! The boundary has three rules:
//!
//! 1. A [`Guile`] handle is created once, on the main thread, and never moved.
//! 2. Every `SCM` that survives more than one C call is rooted with
//!    `scm_gc_protect_object`; values are converted to Rust data immediately.
//! 3. A Scheme exception is caught at the boundary and becomes a Rust
//!    `Result`; it never unwinds through GTK or Rust frames.
//!
//! The FFI declarations live in `sys.rs`, behind safe functions.  This module
//! uses no unsafe blocks except the two `extern "C"` callbacks Guile calls
//! back into; the C ABI stays in one reviewable place.

mod sys;

use std::cell::Cell;
use std::ffi::CString;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::theme::Theme;

/// A Scheme value as Rust data.  Only the shapes the theme uses are named;
/// everything else is [`SchemeValue::Other`] and is ignored by the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemeValue {
    Bool(bool),
    Integer(i64),
    Text(String),
    Symbol(String),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuileError {
    Initialization(String),
    Evaluation(String),
}

impl std::fmt::Display for GuileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuileError::Initialization(message) | GuileError::Evaluation(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for GuileError {}

static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A protected Scheme object.  `scm_gc_protect_object` keeps it alive until
/// this value is dropped, even if no Rust reference to it remains.
struct Root {
    value: sys::Scm,
}

impl Root {
    fn new(value: sys::Scm) -> Self {
        sys::protect(value);

        Self { value }
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        sys::unprotect(self.value);
    }
}

struct BodyContext<'a> {
    body: &'a mut dyn FnMut() -> sys::Scm,
}

struct HandlerContext {
    failed: bool,
    false_value: sys::Scm,
    describe: sys::Scm,
    message: Cell<Option<String>>,
}

unsafe extern "C" fn catch_body(data: *mut libc::c_void) -> sys::Scm {
    let context = unsafe { &mut *(data as *mut BodyContext<'_>) };

    (context.body)()
}

unsafe extern "C" fn catch_handler(
    data: *mut libc::c_void,
    _tag: sys::Scm,
    args: sys::Scm,
) -> sys::Scm {
    let context = unsafe { &mut *(data as *mut HandlerContext) };

    context.failed = true;

    // A caught exception is only useful if the reason survives.  `describe`
    // writes the handler's argument list with `write`, so a malformed settings
    // file is reported with the offending form, not just "it raised".
    let displayed = sys::call_1(context.describe, args);

    context.message.set(sys::text(displayed));

    context.false_value
}

/// The Guile runtime is global and thread-affine.  A handle is deliberately
/// neither Send nor Sync; call it from the thread that initialized it.
pub struct Guile {
    true_value: sys::Scm,
    false_value: sys::Scm,
    describe: Root,
    symbol_predicate: Root,
    _thread_bound: PhantomData<*mut ()>,
}

impl Guile {
    /// Starts Guile on this thread and puts `load_paths` first on `%load-path`.
    ///
    /// Calling this twice in one process is a Guile-level error, so the second
    /// call answers an error instead of initializing the runtime again.
    pub fn init(load_paths: &[PathBuf]) -> Result<Self, GuileError> {
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(GuileError::Initialization(
                "Guile was already initialized in this process".to_owned(),
            ));
        }

        sys::init();

        let true_value = eval_raw("#t")?;
        let false_value = eval_raw("#f")?;
        let describe = Root::new(eval_raw(
            "(lambda (args) (with-output-to-string (lambda () (write args))))",
        )?);
        let symbol_predicate = Root::new(eval_raw("(lambda (value) (symbol? value))")?);

        let runtime = Self {
            true_value,
            false_value,
            describe,
            symbol_predicate,
            _thread_bound: PhantomData,
        };

        if !load_paths.is_empty() {
            let literals: Vec<String> = load_paths
                .iter()
                .filter_map(|path| path.to_str())
                .map(scheme_string_literal)
                .collect();

            if !literals.is_empty() {
                runtime.eval(&format!(
                    "(set! %load-path (append (list {}) %load-path))",
                    literals.join(" ")
                ))?;
            }
        }

        Ok(runtime)
    }

    /// Evaluate Scheme source, turning an exception into an error.
    pub fn eval(&self, source: &str) -> Result<(), GuileError> {
        let source = CString::new(source)
            .map_err(|_| GuileError::Evaluation("Scheme source contains a NUL byte".to_owned()))?;

        self.catch(|| Ok(sys::eval(&source))).map(|_| ())
    }

    /// Read the widget theme, optionally after evaluating `config`.
    ///
    /// The file is evaluated inside `(bibelot theme)`, so it can call
    /// `configure-lock!` with no import of its own.  A missing file is not an
    /// error: the module's defaults are the theme.  A file that raises is an
    /// error, and the caller falls back to [`Theme::default`] rather than
    /// presenting a desktop with half a theme.
    pub fn theme(&self, config: Option<&Path>) -> Result<Theme, GuileError> {
        self.eval("(use-modules (bibelot theme))")?;

        // Each read starts from the module's defaults, so a configuration that
        // sets two keys cannot inherit the previous read's third one.
        let reset = self.catch(|| {
            let procedure = sys::public_ref(c"bibelot theme", c"reset-widget-theme!");

            Ok(sys::call_0(procedure))
        })?;
        let _reset = Root::new(reset);

        if let Some(config) = config
            && config.is_file()
        {
            let path = config.to_str().ok_or_else(|| {
                GuileError::Evaluation(format!(
                    "theme path is not valid UTF-8: {}",
                    config.display()
                ))
            })?;
            let path = scheme_string_literal(path);
            let module = self.catch(|| Ok(sys::resolve_module(c"bibelot theme")))?;

            if module == self.false_value {
                return Err(GuileError::Evaluation(
                    "could not resolve the (bibelot theme) module".to_owned(),
                ));
            }

            let _module = Root::new(module);
            let previous = sys::set_current_module(module);
            let loaded = self.eval(&format!("(load {path})"));

            sys::set_current_module(previous);
            loaded?;
        }

        let alist = self.catch(|| {
            let procedure = sys::public_ref(c"bibelot theme", c"widget-theme");

            Ok(sys::call_0(procedure))
        })?;
        let _alist = Root::new(alist);
        let entries = self.alist(alist)?;

        Ok(Theme::from_alist(&entries))
    }

    fn catch(
        &self,
        mut body: impl FnMut() -> Result<sys::Scm, GuileError>,
    ) -> Result<sys::Scm, GuileError> {
        let mut body_error = None;
        let mut body_function = || match body() {
            Ok(value) => value,
            Err(error) => {
                body_error = Some(error);
                self.false_value
            }
        };
        let mut body_context = BodyContext {
            body: &mut body_function,
        };
        let mut handler_context = HandlerContext {
            failed: false,
            false_value: self.false_value,
            describe: self.describe.value,
            message: Cell::new(None),
        };

        let value = sys::internal_catch(
            self.true_value,
            catch_body,
            (&mut body_context as *mut BodyContext<'_>).cast(),
            catch_handler,
            (&mut handler_context as *mut HandlerContext).cast(),
        );

        if let Some(error) = body_error {
            Err(error)
        } else if handler_context.failed {
            let detail = handler_context.message.take();
            let message = detail
                .filter(|detail| !detail.trim().is_empty())
                .map(|detail| format!("Scheme raised an exception: {detail}"))
                .unwrap_or_else(|| "Scheme raised an exception".to_owned());

            Err(GuileError::Evaluation(message))
        } else {
            Ok(value)
        }
    }

    fn text(&self, value: sys::Scm) -> Result<String, GuileError> {
        sys::text(value).ok_or_else(|| {
            GuileError::Evaluation("Scheme string could not be encoded as UTF-8".to_owned())
        })
    }

    fn symbol_text(&self, value: sys::Scm) -> Result<String, GuileError> {
        let string = self.catch(|| Ok(sys::symbol_to_string(value)))?;

        self.text(string)
    }

    fn value(&self, value: sys::Scm) -> Result<SchemeValue, GuileError> {
        if sys::is_bool(value) {
            return Ok(SchemeValue::Bool(value != self.false_value));
        }

        if sys::is_exact_integer(value) {
            return Ok(SchemeValue::Integer(sys::to_int64(value)));
        }

        if sys::is_string(value) {
            return Ok(SchemeValue::Text(self.text(value)?));
        }

        let is_symbol = self.catch(|| Ok(sys::call_1(self.symbol_predicate.value, value)))?;

        if is_symbol != self.false_value {
            return Ok(SchemeValue::Symbol(self.symbol_text(value)?));
        }

        Ok(SchemeValue::Other)
    }

    fn alist(&self, alist: sys::Scm) -> Result<Vec<(String, SchemeValue)>, GuileError> {
        let mut values = Vec::new();
        let mut cursor = alist;
        let mut seen = 0usize;

        while sys::is_pair(cursor) {
            let entry = sys::car(cursor);

            if sys::is_pair(entry) {
                let key = sys::car(entry);
                let value = sys::cdr(entry);

                values.push((self.symbol_text(key)?, self.value(value)?));
            }

            cursor = sys::cdr(cursor);
            seen += 1;

            if seen > 4096 {
                return Err(GuileError::Evaluation(
                    "Scheme alist is longer than the bridge allows".to_owned(),
                ));
            }
        }

        Ok(values)
    }
}

fn eval_raw(code: &str) -> Result<sys::Scm, GuileError> {
    let code = CString::new(code)
        .map_err(|_| GuileError::Evaluation("Scheme source contains a NUL byte".to_owned()))?;
    let value = sys::eval(&code);

    if value.is_null() {
        Err(GuileError::Evaluation(
            "Scheme evaluator returned no value".to_owned(),
        ))
    } else {
        Ok(value)
    }
}

/// `value` as a double-quoted Scheme string literal: the one escape hatch used
/// for paths, which may contain quotes or backslashes.
pub fn scheme_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);

    escaped.push('"');

    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }

    escaped.push('"');

    escaped
}

#[cfg(test)]
mod tests {
    use super::scheme_string_literal;

    #[test]
    fn literals_are_escaped() {
        assert_eq!(scheme_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(scheme_string_literal("a\\b"), "\"a\\\\b\"");
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! A small, safe Guile runtime for loading whelk's real Scheme configuration.
//!
//! The boundary has three rules:
//!
//! 1. A `Guile` handle is created once, on the main thread, and never moved.
//! 2. Every `SCM` that survives more than one C call is rooted with
//!    `scm_gc_protect_object`; values are converted to Rust data immediately.
//! 3. A Scheme exception is caught at the boundary and becomes a Rust
//!    `Result`; it never unwinds through GTK or Rust frames.
//!
//! The FFI declarations live in `sys.rs`.  This module contains no raw C
//! signatures, so the unsafe surface is easy to review.

mod sys;

use std::ffi::CString;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::slice;

use crate::bar::BarSettings;
use crate::menuapps::{self, MenuSources};
use crate::settings::{ClipboardSettings, SchemeValue, WallpaperSettings};
use crate::startmenu::MenuEntry;
use crate::widgets::WidgetConfig;

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
        unsafe {
            sys::scm_gc_protect_object(value);
        }

        Self { value }
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        unsafe {
            sys::scm_gc_unprotect_object(self.value);
        }
    }
}

struct BodyContext<'a> {
    body: &'a mut dyn FnMut() -> sys::Scm,
}

struct HandlerContext {
    failed: bool,
    false_value: sys::Scm,
}

unsafe extern "C" fn catch_body(data: *mut libc::c_void) -> sys::Scm {
    let context = unsafe { &mut *(data as *mut BodyContext<'_>) };

    (context.body)()
}

unsafe extern "C" fn catch_handler(
    data: *mut libc::c_void,
    _tag: sys::Scm,
    _args: sys::Scm,
) -> sys::Scm {
    let context = unsafe { &mut *(data as *mut HandlerContext) };

    context.failed = true;
    context.false_value
}

/// The Guile runtime is global and thread-affine.  A handle is deliberately
/// neither Send nor Sync; call it from the thread that initialized it.
pub mod widget;

pub struct Guile {
    true_value: sys::Scm,
    false_value: sys::Scm,
    symbol_predicate: Root,
    display_procedure: Root,
    _thread_bound: PhantomData<*mut ()>,
}

impl Guile {
    pub fn init(scheme_paths: &[PathBuf]) -> Result<Self, GuileError> {
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(GuileError::Initialization(
                "Guile was already initialized in this process".to_owned(),
            ));
        }

        unsafe {
            sys::scm_init_guile();
        }

        if !scheme_paths.is_empty() {
            let literals: Vec<String> = scheme_paths
                .iter()
                .filter_map(|path| path.to_str())
                .map(scheme_string_literal)
                .collect();

            if !literals.is_empty() {
                let expression = format!(
                    "(set! %load-path (append (list {}) %load-path))",
                    literals.join(" ")
                );

                eval_raw(&expression)?;
            }
        }

        let true_value = eval_raw("#t")?;
        let false_value = eval_raw("#f")?;
        let symbol_predicate = Root::new(eval_raw("(lambda (value) (symbol? value))")?);
        let display_procedure = Root::new(eval_raw(
            "(lambda (value) (with-output-to-string (lambda () (display value))))",
        )?);

        let runtime = Self {
            true_value,
            false_value,
            symbol_predicate,
            display_procedure,
            _thread_bound: PhantomData,
        };

        runtime.register_rust_version()?;
        runtime.register_widget_primitives()?;

        Ok(runtime)
    }

    pub fn eval_string(&self, code: &str) -> Result<String, GuileError> {
        let value = self.catch(|| {
            let code = CString::new(code).map_err(|_| {
                GuileError::Evaluation("Scheme source contains a NUL byte".to_owned())
            })?;

            Ok(unsafe { sys::scm_c_eval_string(code.as_ptr()) })
        })?;

        let _root = Root::new(value);
        let displayed =
            self.catch(|| Ok(unsafe { sys::scm_call_1(self.display_procedure.value, value) }))?;

        self.text(displayed)
    }

    pub fn version(&self) -> Result<String, GuileError> {
        self.eval_string("(version)")
    }

    pub fn load_bars(&self, config_path: &Path) -> Result<Vec<BarSettings>, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let alist = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("shell-bars->alists").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;

        let _alist_root = Root::new(alist);
        let value = self.scheme_value(alist)?;

        let SchemeValue::List(values) = value else {
            return Ok(Vec::new());
        };

        Ok(values
            .into_iter()
            .filter_map(|value| match value {
                SchemeValue::Alist(entries) => Some(BarSettings::from_alist(&entries)),
                _ => None,
            })
            .collect())
    }

    /// Read the configured desktop widgets, `(shell-widgets->alists)`: each
    /// one's kind, placement and own option alist.  A configuration without
    /// widgets answers an empty list, which is the opt-in default.
    pub fn load_widgets(&self, config_path: &Path) -> Result<Vec<WidgetConfig>, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let alist = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("shell-widgets->alists").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;
        let _alist_root = Root::new(alist);
        let value = self.scheme_value(alist)?;
        let SchemeValue::List(values) = value else {
            return Ok(Vec::new());
        };

        Ok(values
            .into_iter()
            .filter_map(|value| match value {
                SchemeValue::Alist(entries) => WidgetConfig::from_alist(&entries),
                _ => None,
            })
            .collect())
    }

    /// Read the shell menu as `(whelk menu)` units.  A configuration that
    /// writes no menu gets `(default-shell-menu)` from `(whelk defaults)`;
    /// the no-configuration path never reaches Guile and uses the Rust copy of
    /// that menu instead.
    pub fn load_shell_menu(&self, config_path: &Path) -> Result<Vec<MenuEntry>, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let units = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("shell-menu->units").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;
        let _units_root = Root::new(units);
        let entries = self.menu_entries(units, 0)?;

        if !entries.is_empty() {
            return Ok(entries);
        }

        // `default-shell-menu` lives in a module the settings file may never
        // have loaded; importing it is harmless when it is already there.
        self.eval_string("(use-modules (whelk defaults))")?;

        let default = self.catch(|| {
            let module = CString::new("whelk defaults").expect("static module name");
            let name = CString::new("default-shell-menu").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_0(procedure) })
        })?;
        let _default_root = Root::new(default);

        self.menu_entries(default, 0)
    }

    /// Read `(whelk menu)` units directly from SCM.  The generic value walk
    /// classifies a list whose first element is a symbol as an alist, which is
    /// right for a keyed record and wrong for a tagged menu row; the menu has
    /// its own small walk so both shapes stay unambiguous.
    fn menu_entries(&self, units: sys::Scm, depth: usize) -> Result<Vec<MenuEntry>, GuileError> {
        if depth > 8 {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut cursor = units;
        let mut seen = 0usize;

        while unsafe { sys::scm_is_pair(cursor) } != 0 {
            let unit = unsafe { sys::scm_car(cursor) };

            if unsafe { sys::scm_is_pair(unit) } != 0
                && let Some(entry) = self.menu_entry(unit, depth)?
            {
                entries.push(entry);
            }

            cursor = unsafe { sys::scm_cdr(cursor) };
            seen += 1;

            if seen > 4096 {
                return Err(GuileError::Evaluation(
                    "Scheme menu is longer than the bridge allows".to_owned(),
                ));
            }
        }

        Ok(entries)
    }

    fn menu_entry(&self, unit: sys::Scm, depth: usize) -> Result<Option<MenuEntry>, GuileError> {
        let tag = unsafe { sys::scm_car(unit) };
        let rest = unsafe { sys::scm_cdr(unit) };
        let Some(tag) = self.value_text(tag)? else {
            return Ok(None);
        };

        match tag.as_str() {
            "separator" => Ok(Some(MenuEntry::Separator)),
            "item" => {
                if unsafe { sys::scm_is_pair(rest) } == 0 {
                    return Ok(None);
                }

                let Some(label) = self
                    .value_text(unsafe { sys::scm_car(rest) })?
                    .filter(|label| !label.is_empty())
                else {
                    return Ok(None);
                };
                let action_rest = unsafe { sys::scm_cdr(rest) };
                let action = if unsafe { sys::scm_is_pair(action_rest) } != 0 {
                    self.value_text(unsafe { sys::scm_car(action_rest) })?
                        .filter(|action| !action.is_empty())
                } else {
                    None
                };

                Ok(Some(MenuEntry::Item {
                    label,
                    enabled: action.is_some(),
                    action,
                }))
            }
            "submenu" | "menu" => {
                if unsafe { sys::scm_is_pair(rest) } == 0 {
                    return Ok(None);
                }

                let Some(label) = self
                    .value_text(unsafe { sys::scm_car(rest) })?
                    .filter(|label| !label.is_empty())
                else {
                    return Ok(None);
                };
                let body = unsafe { sys::scm_cdr(rest) };
                let marker = if unsafe { sys::scm_is_pair(body) } != 0
                    && unsafe { sys::scm_is_pair(sys::scm_cdr(body)) } == 0
                {
                    self.value_text(unsafe { sys::scm_car(body) })?
                        .filter(|marker| !marker.is_empty())
                } else {
                    None
                };

                if let Some(marker) = marker {
                    return Ok(Some(MenuEntry::Submenu {
                        label,
                        marker: Some(marker),
                        entries: Vec::new(),
                    }));
                }

                Ok(Some(MenuEntry::Submenu {
                    label,
                    marker: None,
                    entries: self.menu_entries(body, depth + 1)?,
                }))
            }
            _ => Ok(None),
        }
    }

    fn value_text(&self, value: sys::Scm) -> Result<Option<String>, GuileError> {
        if unsafe { sys::scm_is_string(value) } != 0 {
            return self.text(value).map(Some);
        }

        if self.is_symbol(value)? {
            return self.symbol_text(value).map(Some);
        }

        Ok(None)
    }

    /// Read the curated menu dictionary and the Orbit corpus once.  Both are
    /// Scheme-owned data; after this call the menu bar only holds owned Rust
    /// strings, so a focus change never enters Guile.
    pub fn load_menu_sources(&self, config_path: &Path) -> Result<MenuSources, GuileError> {
        self.load_config(config_path)?;

        let dictionary =
            self.eval_value("(begin (use-modules (whelk menudict)) (menudict->alists))")?;
        let orbit = self.eval_value("(begin (use-modules (whelk orbit)) orbit-apps)")?;

        Ok(MenuSources {
            dictionary: menuapps::dictionary_from_scheme(&dictionary),
            orbit: menuapps::orbit_from_scheme(&orbit),
            ..MenuSources::default()
        })
    }

    /// Evaluate Scheme and walk the value before it can be collected.  This is
    /// the value-returning counterpart of `eval_string`, which displays its
    /// result instead.
    fn eval_value(&self, code: &str) -> Result<SchemeValue, GuileError> {
        let value = self.catch(|| {
            let code = CString::new(code).map_err(|_| {
                GuileError::Evaluation("Scheme source contains a NUL byte".to_owned())
            })?;

            Ok(unsafe { sys::scm_c_eval_string(code.as_ptr()) })
        })?;
        let _root = Root::new(value);

        self.scheme_value(value)
    }

    pub fn load_wallpaper(&self, config_path: &Path) -> Result<WallpaperSettings, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let alist = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("shell-wallpaper->alist").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;

        let _alist_root = Root::new(alist);
        let values = self.alist(alist)?;

        Ok(WallpaperSettings::from_alist(&values))
    }

    /// Save the picture chosen in the picker as the configuration's
    /// `wallpaper-selected` GUI preference, the key `(whelk prefs)` layers over
    /// `#:current` on the next start.  The primary settings file is untouched.
    pub fn remember_wallpaper(
        &self,
        config_path: &Path,
        wallpaper: &Path,
    ) -> Result<(), GuileError> {
        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let choice = wallpaper.to_str().ok_or_else(|| {
            GuileError::Evaluation("wallpaper path is not valid UTF-8".to_owned())
        })?;
        let code = format!(
            "(begin (use-modules (whelk prefs)) (save-preference! {} 'wallpaper-selected {}))",
            scheme_string_literal(config),
            scheme_string_literal(choice)
        );
        let answer = self.eval_string(&code)?;

        if answer.trim() == "#t" {
            return Ok(());
        }

        Err(GuileError::Evaluation(format!(
            "saving the wallpaper choice answered {answer}"
        )))
    }

    pub fn load_clipboard(&self, config_path: &Path) -> Result<ClipboardSettings, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let alist = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("shell-clipboard->alist").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;

        let _alist_root = Root::new(alist);
        let values = self.alist(alist)?;

        Ok(ClipboardSettings::from_alist(&values))
    }

    /// Reads `(idle-settings #:lock-after ...)`, clamped the way the C bridge
    /// clamps it: `-1` disables timed locking, `0` locks on the first hint, and
    /// a day is the largest delay the shell accepts.  A machine with no
    /// `idle-settings` gets zero, the compatible policy.
    pub fn load_idle_lock_after(&self, config_path: &Path) -> Result<i32, GuileError> {
        self.load_config(config_path)?;

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let alist = self.catch(|| {
            let module = CString::new("whelk state").expect("static module name");
            let name = CString::new("get-shell-idle").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;

        let _alist_root = Root::new(alist);
        let values = self.alist(alist)?;
        let seconds = values
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("lock-after", SchemeValue::Integer(seconds)) => Some(*seconds),
                _ => None,
            })
            .unwrap_or(0);

        Ok(seconds.clamp(-1, 86_400) as i32)
    }

    fn load_config(&self, config_path: &Path) -> Result<(), GuileError> {
        if !config_path.is_file() {
            return Err(GuileError::Evaluation(format!(
                "no settings file at {}",
                config_path.display()
            )));
        }

        let path_text = config_path.to_str().ok_or_else(|| {
            GuileError::Evaluation(format!(
                "settings path is not valid UTF-8: {}",
                config_path.display()
            ))
        })?;
        let path = self.scheme_string(path_text)?;
        let _path_root = Root::new(path);

        let loaded = self.catch(|| {
            let module = CString::new("whelk core").expect("static module name");
            let name = CString::new("load-shell-config!").expect("static procedure name");
            let procedure = unsafe { sys::scm_c_public_ref(module.as_ptr(), name.as_ptr()) };

            Ok(unsafe { sys::scm_call_1(procedure, path) })
        })?;

        if self.is_true(loaded) {
            Ok(())
        } else {
            Err(GuileError::Evaluation(format!(
                "settings file was rejected: {}",
                config_path.display()
            )))
        }
    }

    fn register_rust_version(&self) -> Result<(), GuileError> {
        let module = CString::new("whelk core").expect("static module name");
        let name = CString::new("whelk-rust-version").expect("static primitive name");

        let module = self.catch(|| Ok(unsafe { sys::scm_c_resolve_module(module.as_ptr()) }))?;

        if !self.is_true(module) {
            return Err(GuileError::Evaluation(
                "could not resolve the (whelk core) module".to_owned(),
            ));
        }

        let previous = unsafe { sys::scm_set_current_module(module) };

        let result = self.catch(|| {
            Ok(unsafe {
                sys::scm_c_define_gsubr(
                    name.as_ptr(),
                    0,
                    0,
                    0,
                    rust_version_primitive as sys::ScmSubr,
                )
            })
        });

        if result.is_ok() {
            let export = self.catch(|| {
                unsafe {
                    sys::scm_c_export(name.as_ptr(), std::ptr::null::<libc::c_char>());
                }

                Ok(self.true_value)
            });

            if let Err(error) = export {
                unsafe {
                    sys::scm_set_current_module(previous);
                }
                return Err(error);
            }
        }

        unsafe {
            sys::scm_set_current_module(previous);
        }

        result.map(|_| ())
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
        };

        let value = unsafe {
            sys::scm_internal_catch(
                self.true_value,
                catch_body,
                (&mut body_context as *mut BodyContext<'_>).cast(),
                catch_handler,
                (&mut handler_context as *mut HandlerContext).cast(),
            )
        };

        if let Some(error) = body_error {
            Err(error)
        } else if handler_context.failed {
            Err(GuileError::Evaluation(
                "Scheme raised an exception".to_owned(),
            ))
        } else {
            Ok(value)
        }
    }

    /// Scheme truth: every value except the unique `#f` object is true.
    fn is_true(&self, value: sys::Scm) -> bool {
        value != self.false_value
    }

    fn is_symbol(&self, value: sys::Scm) -> Result<bool, GuileError> {
        let result =
            self.catch(|| Ok(unsafe { sys::scm_call_1(self.symbol_predicate.value, value) }))?;

        Ok(self.is_true(result))
    }

    fn scheme_string(&self, text: &str) -> Result<sys::Scm, GuileError> {
        let text = CString::new(text)
            .map_err(|_| GuileError::Evaluation("text contains a NUL byte".to_owned()))?;

        Ok(unsafe { sys::scm_from_utf8_string(text.as_ptr()) })
    }

    fn text(&self, value: sys::Scm) -> Result<String, GuileError> {
        if unsafe { sys::scm_is_string(value) } == 0 {
            return Err(GuileError::Evaluation(
                "Scheme value is not a string".to_owned(),
            ));
        }

        let mut length = 0usize;
        let pointer = unsafe { sys::scm_to_utf8_stringn(value, &mut length) };

        if pointer.is_null() {
            return Err(GuileError::Evaluation(
                "Scheme string could not be encoded as UTF-8".to_owned(),
            ));
        }

        let bytes = unsafe { slice::from_raw_parts(pointer.cast::<u8>(), length) };
        let result = String::from_utf8_lossy(bytes).into_owned();

        unsafe {
            libc::free(pointer.cast());
        }

        Ok(result)
    }

    fn symbol_text(&self, value: sys::Scm) -> Result<String, GuileError> {
        let string = self.catch(|| Ok(unsafe { sys::scm_symbol_to_string(value) }))?;

        self.text(string)
    }

    fn scheme_value(&self, value: sys::Scm) -> Result<SchemeValue, GuileError> {
        unsafe {
            if sys::scm_is_bool(value) != 0 {
                return Ok(SchemeValue::Bool(self.is_true(value)));
            }

            if sys::scm_is_exact_integer(value) != 0 {
                return Ok(SchemeValue::Integer(sys::scm_to_int64(value)));
            }

            if sys::scm_is_number(value) != 0 {
                return Ok(SchemeValue::Number(sys::scm_to_double(value)));
            }

            if sys::scm_is_string(value) != 0 {
                return Ok(SchemeValue::Text(self.text(value)?));
            }
        }

        let is_symbol =
            self.catch(|| Ok(unsafe { sys::scm_call_1(self.symbol_predicate.value, value) }))?;

        if self.is_true(is_symbol) {
            return Ok(SchemeValue::Symbol(self.symbol_text(value)?));
        }

        if unsafe { sys::scm_is_pair(value) } != 0 {
            let first = unsafe { sys::scm_car(value) };
            let first_is_pair = unsafe { sys::scm_is_pair(first) } != 0;
            let first_is_symbol = if first_is_pair {
                self.is_symbol(unsafe { sys::scm_car(first) })?
            } else {
                false
            };

            // A list whose first element is a keyed pair is an alist.  A list
            // of alists (the bar's modules) does not start this way, so the
            // two shapes stay distinguishable without a separate marker.
            if first_is_symbol {
                return self.alist(value).map(SchemeValue::Alist);
            }

            let mut items = Vec::new();
            let mut cursor = value;
            let mut seen = 0usize;

            while unsafe { sys::scm_is_pair(cursor) } != 0 {
                items.push(self.scheme_value(unsafe { sys::scm_car(cursor) })?);
                cursor = unsafe { sys::scm_cdr(cursor) };
                seen += 1;

                if seen > 4096 {
                    return Err(GuileError::Evaluation(
                        "Scheme list is longer than the bridge allows".to_owned(),
                    ));
                }
            }

            return Ok(SchemeValue::List(items));
        }

        Ok(SchemeValue::Other)
    }

    fn alist(&self, alist: sys::Scm) -> Result<Vec<(String, SchemeValue)>, GuileError> {
        let mut values = Vec::new();
        let mut cursor = alist;
        let mut seen = 0usize;

        while unsafe { sys::scm_is_pair(cursor) } != 0 {
            let entry = unsafe { sys::scm_car(cursor) };

            if unsafe { sys::scm_is_pair(entry) } != 0 {
                let key = unsafe { sys::scm_car(entry) };
                let value = unsafe { sys::scm_cdr(entry) };

                values.push((self.symbol_text(key)?, self.scheme_value(value)?));
            }

            cursor = unsafe { sys::scm_cdr(cursor) };
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

unsafe extern "C" fn rust_version_primitive() -> sys::Scm {
    // The only allocation here is a Scheme string; the C caller has a live
    // Guile stack and the runtime is initialized.  A panic must never cross
    // this boundary, so the input is a compile-time constant.
    let version = CString::new(env!("CARGO_PKG_VERSION")).expect("version has no NUL");

    unsafe { sys::scm_from_utf8_string(version.as_ptr()) }
}

fn eval_raw(code: &str) -> Result<sys::Scm, GuileError> {
    let code = CString::new(code)
        .map_err(|_| GuileError::Evaluation("Scheme source contains a NUL byte".to_owned()))?;
    let value = unsafe { sys::scm_c_eval_string(code.as_ptr()) };

    if value.is_null() {
        Err(GuileError::Evaluation(
            "Scheme evaluator returned no value".to_owned(),
        ))
    } else {
        Ok(value)
    }
}

/// Escape a Rust string as a Scheme string literal.  This is used for load
/// paths and the settings path, both of which may contain quotes or backslashes.
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

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
//! The FFI declarations live in `sys.rs`, behind safe functions.  This module
//! and `widget.rs` use no unsafe blocks except the two `extern "C"` callbacks
//! Guile calls back into; the C ABI stays in one reviewable place.

mod sys;

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::bar::BarSettings;
use crate::frost::Frost;
use crate::menuapps::{self, MenuSources};
use crate::nightlight::Config as NightLightConfig;
use crate::settings::{ClipboardSettings, SchemeValue, WallpaperSettings};
use crate::startmenu::MenuEntry;
use bibelot::WidgetConfig;
use whelk_launcher::style::LauncherSettings;

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
    /// The settings file this handle has already evaluated, with the stamp it
    /// had when it was read.
    ///
    /// Every `load_*` group reads its own part of the shell out of the same
    /// file, so without this one shell load parses and evaluates the user's
    /// configuration once per group -- eight times -- on every reload.  The
    /// path is remembered instead, and `invalidate_config`, a file whose stamp
    /// moved, or the shell's own reload is what makes it read again.
    loaded_config: RefCell<Option<(PathBuf, std::time::SystemTime, u64)>>,
    _thread_bound: PhantomData<*mut ()>,
}

impl Guile {
    pub fn init(scheme_paths: &[PathBuf]) -> Result<Self, GuileError> {
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(GuileError::Initialization(
                "Guile was already initialized in this process".to_owned(),
            ));
        }

        sys::init();

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
            loaded_config: RefCell::new(None),
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

            Ok(sys::eval(&code))
        })?;

        let _root = Root::new(value);
        let displayed = self.catch(|| Ok(sys::call_1(self.display_procedure.value, value)))?;

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
            let procedure = sys::public_ref(c"whelk state", c"shell-bars->alists");

            Ok(sys::call_1(procedure, path))
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

    /// Read the launcher's four geometry values from the first bar's alist.
    pub fn load_launcher(&self, config_path: &Path) -> Result<LauncherSettings, GuileError> {
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
            let procedure = sys::public_ref(c"whelk state", c"shell-bars->alists");

            Ok(sys::call_1(procedure, path))
        })?;
        let _alist_root = Root::new(alist);
        let value = self.scheme_value(alist)?;
        let SchemeValue::List(values) = value else {
            return Ok(LauncherSettings::default());
        };
        let Some(SchemeValue::Alist(entries)) = values.into_iter().next() else {
            return Ok(LauncherSettings::default());
        };
        let mut settings = LauncherSettings::default();

        if let Some(SchemeValue::Integer(width)) = value_at(&entries, "launcher-width") {
            settings.width = *width as i32;
        }
        if let Some(SchemeValue::Integer(height)) = value_at(&entries, "launcher-height") {
            settings.height = *height as i32;
        }
        if let Some(SchemeValue::Integer(size)) = value_at(&entries, "launcher-icon-size") {
            settings.icon_size = *size as i32;
        }
        if let Some(SchemeValue::Integer(scale)) = value_at(&entries, "launcher-font-scale") {
            settings.font_scale = *scale as i32;
        }

        Ok(settings)
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
            let procedure = sys::public_ref(c"whelk state", c"shell-widgets->alists");

            Ok(sys::call_1(procedure, path))
        })?;
        let _alist_root = Root::new(alist);
        let value = self.scheme_value(alist)?;
        let SchemeValue::List(values) = value else {
            return Ok(Vec::new());
        };

        Ok(values
            .into_iter()
            .filter_map(|value| match value {
                SchemeValue::Alist(entries) => {
                    let entries = widget_entries(&entries);

                    WidgetConfig::from_alist(&entries)
                }
                _ => None,
            })
            .collect())
    }

    /// Read the cards' structural theme from `(bibelot theme)`.
    ///
    /// The widget project owns the module; the shell only adds its directory to
    /// `%load-path` and asks for the alist.  A missing module is a cosmetic
    /// failure, so the caller falls back to [`bibelot::Theme::default`].
    pub fn load_widget_theme(&self) -> Result<bibelot::Theme, GuileError> {
        self.eval_string("(use-modules (bibelot theme))")?;
        // Every read starts from the module's defaults, so a user file that
        // sets two keys cannot inherit the previous read's third one.
        let _ = self.eval_value("(reset-widget-theme!)")?;

        if let Some(path) = bibelot::theme::user_theme_path() {
            let path = path.to_str().ok_or_else(|| {
                GuileError::Evaluation(format!(
                    "widget theme path is not valid UTF-8: {}",
                    path.display()
                ))
            })?;

            self.eval_string(&format!("(load {})", scheme_string_literal(path)))?;
        }

        let value = self.eval_value("(widget-theme)")?;

        match value {
            SchemeValue::Alist(entries) => Ok(bibelot::Theme::from_alist(&theme_entries(&entries))),
            _ => Ok(bibelot::Theme::default()),
        }
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
            let procedure = sys::public_ref(c"whelk state", c"shell-menu->units");

            Ok(sys::call_1(procedure, path))
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
            let procedure = sys::public_ref(c"whelk defaults", c"default-shell-menu");

            Ok(sys::call_0(procedure))
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

        while sys::is_pair(cursor) {
            let unit = sys::car(cursor);

            if sys::is_pair(unit)
                && let Some(entry) = self.menu_entry(unit, depth)?
            {
                entries.push(entry);
            }

            cursor = sys::cdr(cursor);
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
        let tag = sys::car(unit);
        let rest = sys::cdr(unit);
        let Some(tag) = self.value_text(tag)? else {
            return Ok(None);
        };

        match tag.as_str() {
            "separator" => Ok(Some(MenuEntry::Separator)),
            "item" => {
                if !sys::is_pair(rest) {
                    return Ok(None);
                }

                let Some(label) = self
                    .value_text(sys::car(rest))?
                    .filter(|label| !label.is_empty())
                else {
                    return Ok(None);
                };
                let action_rest = sys::cdr(rest);
                let action = if sys::is_pair(action_rest) {
                    self.value_text(sys::car(action_rest))?
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
                if !sys::is_pair(rest) {
                    return Ok(None);
                }

                let Some(label) = self
                    .value_text(sys::car(rest))?
                    .filter(|label| !label.is_empty())
                else {
                    return Ok(None);
                };
                let body = sys::cdr(rest);
                let marker = if sys::is_pair(body) && !sys::is_pair(sys::cdr(body)) {
                    self.value_text(sys::car(body))?
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
        if sys::is_string(value) {
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

            Ok(sys::eval(&code))
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
            let procedure = sys::public_ref(c"whelk state", c"shell-wallpaper->alist");

            Ok(sys::call_1(procedure, path))
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
            // The choice is an override the next load reads: forget the
            // remembered evaluation so it is part of it.
            self.invalidate_config();

            return Ok(());
        }

        Err(GuileError::Evaluation(format!(
            "saving the wallpaper choice answered {answer}"
        )))
    }

    /// Save one GUI override through `(whelk prefs)`, with the value written as
    /// the Scheme it will be read back as: `#t`, `12`, `0.75`, `"Inter"` or
    /// `'cua`.  The key is a plain preference symbol, so anything else is
    /// refused rather than spliced into the source.
    pub fn save_preference(
        &self,
        config_path: &Path,
        key: &str,
        scheme: &str,
    ) -> Result<(), GuileError> {
        if !is_preference_key(key) {
            return Err(GuileError::Evaluation(format!(
                "'{key}' is not a preference key"
            )));
        }

        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let code = format!(
            "(begin (use-modules (whelk prefs)) (save-preference! {} '{key} {scheme}))",
            scheme_string_literal(config)
        );
        let answer = self.eval_string(&code)?;

        if answer.trim() == "#t" {
            // The override is what the next load must see.
            self.invalidate_config();

            return Ok(());
        }

        Err(GuileError::Evaluation(format!(
            "saving '{key} answered {answer}"
        )))
    }

    /// Remember NAME as this configuration's preset, or forget the remembered
    /// choice when NAME is `None`, so the settings file decides again.
    ///
    /// Nothing is applied here: the choice is a preference, and the next load
    /// is what registers its bars.  A caller that wants the switch now asks for
    /// a reload, which is also what makes the two paths agree.
    pub fn choose_preset(
        &self,
        config_path: &Path,
        name: Option<&str>,
    ) -> Result<bool, GuileError> {
        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let choice = match name {
            None => "#f".to_owned(),
            Some(name) if is_preset_name(name) => format!("'{name}"),
            Some(name) => {
                return Err(GuileError::Evaluation(format!(
                    "'{name}' is not a preset name"
                )));
            }
        };
        let answer = self.eval_string(&format!(
            "(begin (use-modules (whelk presets)) (choose-preset! {} {}))",
            scheme_string_literal(config),
            choice
        ))?;

        // The remembered evaluation was made before this choice existed.
        self.invalidate_config();

        Ok(answer.trim() == "#t")
    }

    /// Delete the GUI overrides, leaving the settings file untouched.
    pub fn reset_preferences(&self, config_path: &Path) -> Result<bool, GuileError> {
        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let answer = self.eval_string(&format!(
            "(begin (use-modules (whelk prefs)) (reset-preferences! {}))",
            scheme_string_literal(config)
        ))?;

        // Whatever was remembered answered with the overrides still in it.
        self.invalidate_config();

        Ok(answer.trim() == "#t")
    }

    /// One saved preference as text, or `None` when the file does not override
    /// it.  A window field shows the text it can type back; a symbol arrives as
    /// its name and a number as its digits.
    pub fn read_preference(
        &self,
        config_path: &Path,
        key: &str,
    ) -> Result<Option<String>, GuileError> {
        if !is_preference_key(key) {
            return Err(GuileError::Evaluation(format!(
                "'{key}' is not a preference key"
            )));
        }

        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let answer = self.eval_string(&format!(
            "(begin (use-modules (whelk prefs)) \
             (let ((value (preference-ref {} '{key}))) \
             (cond ((not value) \"\") \
             ((symbol? value) (symbol->string value)) \
             ((string? value) value) \
             (else (simple-format #f \"~a\" value)))))",
            scheme_string_literal(config)
        ))?;

        Ok((!answer.is_empty()).then_some(answer))
    }

    /// The names `(whelk themes)` knows, in its own sorted order.
    pub fn theme_names(&self) -> Result<Vec<String>, GuileError> {
        let answer = self.eval_string(
            "(begin (use-modules (whelk themes)) \
             (with-output-to-string \
             (lambda () \
             (for-each (lambda (theme) \
             (display (if (string? theme) theme (symbol->string theme))) \
             (newline)) \
             (themes)))))",
        )?;

        Ok(answer
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// The night light the shell reads, overrides included.  An unconfigured
    /// shell answers the filter's defaults, as the C bridge does.
    pub fn load_night_light(&self, config_path: &Path) -> Result<NightLightConfig, GuileError> {
        self.load_config(config_path)?;

        let config = config_path
            .to_str()
            .ok_or_else(|| GuileError::Evaluation("settings path is not valid UTF-8".to_owned()))?;
        let answer = self.eval_string(&format!(
            "(begin (use-modules (whelk state)) \
             (let ((alist (get-shell-night-light {}))) \
             (if (or (not alist) (null? alist)) \"\" \
             (simple-format #f \"~a ~a ~a ~a ~a ~a ~a\" \
             (if (assq-ref alist 'configured?) 1 0) \
             (if (assq-ref alist 'enabled?) 1 0) \
             (assq-ref alist 'mode) \
             (assq-ref alist 'temperature) \
             (assq-ref alist 'start) \
             (assq-ref alist 'end) \
             (assq-ref alist 'transition)))))",
            scheme_string_literal(config)
        ))?;
        let fields: Vec<&str> = answer.split_whitespace().collect();

        if fields.len() != 7 {
            return Ok(NightLightConfig::default());
        }

        Ok(NightLightConfig {
            configured: fields[0] == "1",
            enabled: fields[1] == "1",
            mode: if fields[2] == "always" {
                crate::nightlight::Mode::Always
            } else {
                crate::nightlight::Mode::Scheduled
            },
            temperature: fields[3].parse().unwrap_or(3400),
            start_minute: fields[4].parse().unwrap_or(19 * 60),
            end_minute: fields[5].parse().unwrap_or(7 * 60),
            transition_minutes: fields[6].parse().unwrap_or(30),
        })
    }

    /// Reads the frosted-glass finish from `(frosted-glass ...)`.  An empty
    /// alist is an unconfigured finish, which stays off.
    pub fn load_frost(&self, config_path: &Path) -> Result<Frost, GuileError> {
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
            let procedure = sys::public_ref(c"whelk state", c"shell-frost->alist");

            Ok(sys::call_1(procedure, path))
        })?;

        let _alist_root = Root::new(alist);
        let values = self.alist(alist)?;

        Ok(Frost::from_alist(&values))
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
            let procedure = sys::public_ref(c"whelk state", c"shell-clipboard->alist");

            Ok(sys::call_1(procedure, path))
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
            let procedure = sys::public_ref(c"whelk state", c"get-shell-idle");

            Ok(sys::call_1(procedure, path))
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

        // One shell load asks the same file for its bars, its wallpaper, its
        // widgets and the rest.  Evaluating it once and remembering it turns
        // eight parses of the user's configuration into one; `invalidate_config`
        // and a stamp that moved are the explicit re-read.
        if let Some((loaded_path, time, len)) = self.loaded_config.borrow().clone()
            && loaded_path == config_path
            && config_stamp(config_path) == Some((time, len))
        {
            return Ok(());
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
            let procedure = sys::public_ref(c"whelk core", c"load-shell-config!");

            Ok(sys::call_1(procedure, path))
        })?;

        if self.is_true(loaded) {
            if let Some((time, len)) = config_stamp(config_path) {
                *self.loaded_config.borrow_mut() = Some((config_path.to_owned(), time, len));
            }

            Ok(())
        } else {
            Err(GuileError::Evaluation(format!(
                "settings file was rejected: {}",
                config_path.display()
            )))
        }
    }

    /// Make the next `load_*` evaluate the settings file again.
    ///
    /// A reload exists because the file or the GUI overrides changed, so the
    /// remembered evaluation must not answer for it.  A writer that changes
    /// what a load would produce calls this itself.
    pub fn invalidate_config(&self) {
        self.loaded_config.borrow_mut().take();
    }

    fn register_rust_version(&self) -> Result<(), GuileError> {
        let module = c"whelk core";
        let name = c"whelk-rust-version";

        let module = self.catch(|| Ok(sys::resolve_module(module)))?;

        if !self.is_true(module) {
            return Err(GuileError::Evaluation(
                "could not resolve the (whelk core) module".to_owned(),
            ));
        }

        let previous = sys::set_current_module(module);

        let result = self.catch(|| {
            Ok(sys::define_gsubr(
                name,
                0,
                0,
                0,
                rust_version_primitive as sys::ScmSubr,
            ))
        });

        if result.is_ok() {
            let export = self.catch(|| {
                sys::export(name);

                Ok(self.true_value)
            });

            if let Err(error) = export {
                sys::set_current_module(previous);
                return Err(error);
            }
        }

        sys::set_current_module(previous);

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
        let result = self.catch(|| Ok(sys::call_1(self.symbol_predicate.value, value)))?;

        Ok(self.is_true(result))
    }

    fn scheme_string(&self, text: &str) -> Result<sys::Scm, GuileError> {
        let text = CString::new(text)
            .map_err(|_| GuileError::Evaluation("text contains a NUL byte".to_owned()))?;

        Ok(sys::from_utf8_string(&text))
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

    fn scheme_value(&self, value: sys::Scm) -> Result<SchemeValue, GuileError> {
        if sys::is_bool(value) {
            return Ok(SchemeValue::Bool(self.is_true(value)));
        }

        if sys::is_exact_integer(value) {
            return Ok(SchemeValue::Integer(sys::to_int64(value)));
        }

        if sys::is_number(value) {
            return Ok(SchemeValue::Number(sys::to_double(value)));
        }

        if sys::is_string(value) {
            return Ok(SchemeValue::Text(self.text(value)?));
        }

        let is_symbol = self.catch(|| Ok(sys::call_1(self.symbol_predicate.value, value)))?;

        if self.is_true(is_symbol) {
            return Ok(SchemeValue::Symbol(self.symbol_text(value)?));
        }

        if sys::is_pair(value) {
            let first = sys::car(value);
            let first_is_pair = sys::is_pair(first);
            let first_is_symbol = if first_is_pair {
                self.is_symbol(sys::car(first))?
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

            while sys::is_pair(cursor) {
                items.push(self.scheme_value(sys::car(cursor))?);
                cursor = sys::cdr(cursor);
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

        while sys::is_pair(cursor) {
            let entry = sys::car(cursor);

            if sys::is_pair(entry) {
                let key = sys::car(entry);
                let value = sys::cdr(entry);

                values.push((self.symbol_text(key)?, self.scheme_value(value)?));
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

unsafe extern "C" fn rust_version_primitive() -> sys::Scm {
    // The version is a compile-time string with a trailing NUL, so the `CStr`
    // view needs no allocation.  A panic must never cross this boundary; if the
    // impossible malformed value ever appeared, answer a safe placeholder.
    const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

    match CStr::from_bytes_with_nul(VERSION) {
        Ok(version) => sys::from_utf8_string(version),
        Err(_) => sys::from_utf8_string(c"0.0.0"),
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

/// Escape a Rust string as a Scheme string literal.  This is used for load
/// What a remembered evaluation was made from: the file's modification time and
/// size.  A file that is rewritten moves at least one of them, and the shell's
/// own reload invalidates explicitly, so a stale evaluation cannot answer for a
/// configuration that changed.
fn config_stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;

    Some((metadata.modified().ok()?, metadata.len()))
}

/// The characters a preference symbol is made of.  A key is spliced into the
/// Scheme source, so anything outside this set never reaches it.
fn is_preference_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '?'
        })
}

/// The characters a preset name is made of.  Like a preference key it is
/// spliced into the Scheme source as a symbol, so a name from the command line
/// is refused rather than escaped.
fn is_preset_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
                || character == '+'
        })
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

/// Convert one of the shell's Scheme values into the widget project's copy.
fn widget_value(value: &SchemeValue) -> bibelot::widgets::SchemeValue {
    use bibelot::widgets::SchemeValue as Widget;

    match value {
        SchemeValue::Bool(value) => Widget::Bool(*value),
        SchemeValue::Integer(value) => Widget::Integer(*value),
        SchemeValue::Number(value) => Widget::Number(*value),
        SchemeValue::Text(value) => Widget::Text(value.clone()),
        SchemeValue::Symbol(value) => Widget::Symbol(value.clone()),
        SchemeValue::List(values) => Widget::List(values.iter().map(widget_value).collect()),
        SchemeValue::Alist(entries) => Widget::Alist(widget_entries(entries)),
        SchemeValue::Other => Widget::Other,
    }
}

fn widget_entries(
    entries: &[(String, SchemeValue)],
) -> Vec<(String, bibelot::widgets::SchemeValue)> {
    entries
        .iter()
        .map(|(key, value)| (key.clone(), widget_value(value)))
        .collect()
}

/// The card theme only names integers and text; everything else is ignored by
/// `Theme::from_alist`, so the conversion can be the small theme vocabulary.
fn theme_entries(entries: &[(String, SchemeValue)]) -> Vec<(String, bibelot::guile::SchemeValue)> {
    use bibelot::guile::SchemeValue as Theme;

    entries
        .iter()
        .map(|(key, value)| {
            let value = match value {
                SchemeValue::Bool(value) => Theme::Bool(*value),
                SchemeValue::Integer(value) => Theme::Integer(*value),
                SchemeValue::Text(value) => Theme::Text(value.clone()),
                SchemeValue::Symbol(value) => Theme::Symbol(value.clone()),
                _ => Theme::Other,
            };

            (key.clone(), value)
        })
        .collect()
}

fn value_at<'a>(alist: &'a [(String, SchemeValue)], key: &str) -> Option<&'a SchemeValue> {
    alist
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::{is_preference_key, scheme_string_literal};

    #[test]
    fn literals_are_escaped() {
        assert_eq!(scheme_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(scheme_string_literal("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn preference_keys_cannot_escape_their_symbol() {
        for key in ["theme", "bar-position", "clock-24h?", "x1"] {
            assert!(is_preference_key(key), "{key}");
        }

        for key in ["", "Theme", "a b", "a)b", "a'b", "a\"b", "ä"] {
            assert!(!is_preference_key(key), "{key}");
        }
    }
}

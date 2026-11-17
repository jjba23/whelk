// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Scheme side of the widget bridge, ported from the widget half of
//! `src/guile_bridge.c`.
//!
//! A `scheme-module` is a bar module whose reading is a Guile procedure.  The
//! procedure builds a GTK widget with `whelk-widget-*` and returns it; the
//! module swaps its previous widget for that one.  This module owns both halves:
//!
//! * the primitives, registered into `(whelk core)` the first time Guile is
//!   initialized, and
//! * the call a module makes on every refresh.
//!
//! A widget crosses the boundary as an integer handle, not a smob.  The C
//! shell wraps its widget in a Scheme smob; that is a pointer into Guile's GC
//! and a macro-level ABI (`SCM_NEWSMOB`, `SCM_SMOB_DATA`) this side would have
//! to restate bit for bit.  A handle is an exact integer the registry below
//! maps to an owned `gtk::Widget`, so the values crossing the ABI are ones
//! `sys.rs` already declares and a stale handle is simply `None`.
//!
//! A procedure can also return a plain string, number or symbol, which becomes
//! a label, exactly as the C bridge accepts.  Anything else is not a widget and
//! the module keeps what it was showing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::panic::{self, AssertUnwindSafe};

use gtk::prelude::*;
use gtk4 as gtk;

use super::{Guile, GuileError, sys};

/// The message a `scheme-module` shows when it was configured without one.
pub const MISSING_UPDATE: &str = "scheme: #:update is missing";

thread_local! {
    /// The widgets the primitives built, by the handle the Scheme side holds.
    /// Guile is thread-affine and so is GTK, so a thread-local registry is the
    /// whole lifetime story: a handle cannot outlive the thread that made it.
    static HANDLES: RefCell<Handles> = RefCell::new(Handles::default());
}

#[derive(Default)]
struct Handles {
    next: i64,
    widgets: BTreeMap<i64, gtk::Widget>,
}

impl Handles {
    fn insert(&mut self, widget: gtk::Widget) -> i64 {
        self.next += 1;
        self.widgets.insert(self.next, widget);

        self.next
    }

    fn take(&mut self, handle: i64) -> Option<gtk::Widget> {
        self.widgets.remove(&handle)
    }

    fn with<R>(&self, handle: i64, use_widget: impl FnOnce(&gtk::Widget) -> R) -> Option<R> {
        self.widgets.get(&handle).map(use_widget)
    }
}

fn insert(widget: gtk::Widget) -> sys::Scm {
    let handle = HANDLES.with(|handles| handles.borrow_mut().insert(widget));

    sys::from_int64(handle)
}

fn take(handle: i64) -> Option<gtk::Widget> {
    HANDLES.with(|handles| handles.borrow_mut().take(handle))
}

fn false_value() -> sys::Scm {
    // `#f` is an immediate the runtime already owns; evaluating the literal
    // allocates nothing and cannot raise.
    sys::eval(c"#f")
}

fn true_value() -> sys::Scm {
    sys::eval(c"#t")
}

/// Run one primitive body with a panic guard: a Rust panic must never unwind
/// through Guile's C frames.
fn guarded(body: impl FnOnce() -> sys::Scm) -> sys::Scm {
    match panic::catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => false_value(),
    }
}

/// A Scheme string or symbol as Rust text.  `None` for anything else, which is
/// also what an absent optional argument is (Guile passes its undefined value).
fn scheme_text(value: sys::Scm) -> Option<String> {
    let string = if sys::is_string(value) {
        value
    } else if is_symbol(value) {
        sys::symbol_to_string(value)
    } else {
        return None;
    };

    sys::text(string)
}

/// `symbol?` is a libguile macro, not an exported function, so the predicate
/// the C bridge gets for free is asked of `(guile)` instead.  A primitive runs
/// on the same thread Guile was initialized on and inside Guile's own stack,
/// which is where a call like this belongs.
fn is_symbol(value: sys::Scm) -> bool {
    let procedure = sys::public_ref(c"guile", c"symbol?");

    !procedure.is_null() && sys::call_1(procedure, value) == true_value()
}

fn scheme_number(value: sys::Scm) -> Option<f64> {
    sys::is_number(value).then(|| sys::to_double(value))
}

fn scheme_integer(value: sys::Scm) -> Option<i64> {
    sys::is_exact_integer(value).then(|| sys::to_int64(value))
}

/// The label every text-shaped primitive builds: left-aligned, vertically
/// centred, with the caller's CSS class when it named one.
fn label(text: Option<&str>, css: Option<&str>) -> gtk::Widget {
    let label = gtk::Label::new(text);

    label.set_valign(gtk::Align::Center);
    label.set_xalign(0.0);
    label.set_halign(gtk::Align::Start);

    if let Some(css) = css.filter(|css| !css.is_empty()) {
        label.add_css_class(css);
    }

    label.upcast()
}

fn progress(fraction: f64) -> gtk::Widget {
    let progress = gtk::ProgressBar::new();

    progress.add_css_class("whelk-scheme-progress");
    progress.set_fraction(fraction.clamp(0.0, 1.0));
    progress.set_show_text(false);
    progress.set_hexpand(true);

    progress.upcast()
}

fn level(fraction: f64) -> gtk::Widget {
    let level = gtk::LevelBar::for_interval(0.0, 1.0);

    level.add_css_class("whelk-scheme-level");
    level.set_value(fraction.clamp(0.0, 1.0));
    level.set_hexpand(true);

    level.upcast()
}

fn image(name: &str, pixels: i64) -> gtk::Widget {
    let image = gtk::Image::from_icon_name(name);

    image.set_pixel_size(i32::try_from(pixels).unwrap_or(16).max(1));

    image.upcast()
}

fn sparkline(values: &[f64], width: i32, height: i32, color: Option<&str>) -> Option<gtk::Widget> {
    if values.is_empty() {
        return None;
    }

    let width = width.max(16);
    let height = height.max(8);
    let ink = color
        .and_then(|color| gtk::gdk::RGBA::parse(color).ok())
        .unwrap_or_else(|| gtk::gdk::RGBA::new(0.92, 0.92, 0.94, 1.0));
    let area = gtk::DrawingArea::new();
    let points = values.to_vec();

    area.add_css_class("whelk-scheme-sparkline");
    area.set_size_request(width, height);
    area.set_content_width(width);
    area.set_content_height(height);
    area.set_draw_func(move |_, context, width, height| {
        draw_sparkline(context, width, height, &points, &ink);
    });

    Some(area.upcast())
}

/// The C bridge's sparkline: the same stroke, the same faded fill below it.
fn draw_sparkline(
    context: &gtk::cairo::Context,
    width: i32,
    height: i32,
    values: &[f64],
    ink: &gtk::gdk::RGBA,
) {
    if values.is_empty() || width <= 2 || height <= 2 {
        return;
    }

    let mut minimum = values[0];
    let mut maximum = values[0];

    for value in &values[1..] {
        minimum = minimum.min(*value);
        maximum = maximum.max(*value);
    }

    if maximum - minimum < 0.000_001 {
        maximum = minimum + 1.0;
    }

    let step = if values.len() > 1 {
        f64::from(width - 2) / (values.len() - 1) as f64
    } else {
        0.0
    };

    for (index, value) in values.iter().enumerate() {
        let x = 1.0 + step * index as f64;
        let y = f64::from(height - 1)
            - ((value - minimum) / (maximum - minimum)) * f64::from(height - 2);

        if index == 0 {
            context.move_to(x, y);
        } else {
            context.line_to(x, y);
        }
    }

    context.set_line_width(1.4);
    context.set_line_join(gtk::cairo::LineJoin::Round);
    context.set_source_rgba(
        f64::from(ink.red()),
        f64::from(ink.green()),
        f64::from(ink.blue()),
        0.92,
    );
    let _ = context.stroke_preserve();

    context.line_to(f64::from(width - 1), f64::from(height - 1));
    context.line_to(1.0, f64::from(height - 1));
    context.close_path();
    context.set_source_rgba(
        f64::from(ink.red()),
        f64::from(ink.green()),
        f64::from(ink.blue()),
        0.16,
    );
    let _ = context.fill();
}

/// A Scheme procedure held for as long as a button needs it.  The SCM is rooted
/// with `scm_gc_protect_object` and unrooted when the closure goes away.
struct ProtectedProcedure(sys::Scm);

impl ProtectedProcedure {
    fn new(procedure: sys::Scm) -> Option<Self> {
        let is_procedure = sys::procedure_p(procedure) == true_value();

        if !is_procedure {
            return None;
        }

        sys::protect(procedure);

        Some(Self(procedure))
    }

    fn call(&self) {
        struct Call {
            procedure: sys::Scm,
        }

        unsafe extern "C" fn thunk(data: *mut libc::c_void) -> sys::Scm {
            let call = unsafe { &*(data as *const Call) };

            sys::call_0(call.procedure)
        }

        unsafe extern "C" fn handler(
            _data: *mut libc::c_void,
            _tag: sys::Scm,
            _arguments: sys::Scm,
        ) -> sys::Scm {
            false_value()
        }

        let mut call = Call { procedure: self.0 };

        // A procedure that raises leaves the button's click a no-op, the way
        // the C bridge logs the exception and carries on.
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            sys::internal_catch(
                true_value(),
                thunk,
                (&mut call as *mut Call).cast(),
                handler,
                std::ptr::null_mut(),
            );
        }));
    }
}

impl Drop for ProtectedProcedure {
    fn drop(&mut self) {
        sys::unprotect(self.0);
    }
}

// ---------------------------------------------------------------------------
// The primitives
// ---------------------------------------------------------------------------

unsafe extern "C" fn widget_label_primitive(text: sys::Scm, css: sys::Scm) -> sys::Scm {
    guarded(|| {
        let text = scheme_text(text);
        let css = scheme_text(css);

        insert(label(text.as_deref(), css.as_deref()))
    })
}

unsafe extern "C" fn widget_markup_primitive(markup: sys::Scm, css: sys::Scm) -> sys::Scm {
    guarded(|| {
        let markup = scheme_text(markup);
        let css = scheme_text(css);
        let widget = label(None, css.as_deref());

        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            label.set_markup(markup.as_deref().unwrap_or_default());
        }

        insert(widget)
    })
}

unsafe extern "C" fn widget_progress_primitive(fraction: sys::Scm) -> sys::Scm {
    guarded(|| {
        let fraction = scheme_number(fraction).unwrap_or(0.0);

        insert(progress(fraction))
    })
}

unsafe extern "C" fn widget_level_primitive(fraction: sys::Scm) -> sys::Scm {
    guarded(|| {
        let fraction = scheme_number(fraction).unwrap_or(0.0);

        insert(level(fraction))
    })
}

unsafe extern "C" fn widget_sparkline_primitive(
    values: sys::Scm,
    width: sys::Scm,
    height: sys::Scm,
    color: sys::Scm,
) -> sys::Scm {
    guarded(|| {
        let mut points = Vec::new();
        let mut cursor = values;
        let mut seen = 0usize;

        while sys::is_pair(cursor) {
            if let Some(value) = scheme_number(sys::car(cursor)) {
                points.push(value);
            }

            cursor = sys::cdr(cursor);
            seen += 1;

            if seen > 4096 {
                break;
            }
        }

        let width = scheme_integer(width).unwrap_or(90);
        let height = scheme_integer(height).unwrap_or(22);
        let color = scheme_text(color);
        let width = i32::try_from(width).unwrap_or(90);
        let height = i32::try_from(height).unwrap_or(22);

        match sparkline(&points, width, height, color.as_deref()) {
            Some(widget) => insert(widget),
            None => false_value(),
        }
    })
}

unsafe extern "C" fn widget_image_primitive(name: sys::Scm, size: sys::Scm) -> sys::Scm {
    guarded(|| {
        let name = scheme_text(name).unwrap_or_default();
        let size = scheme_integer(size).unwrap_or(16);

        insert(image(&name, size))
    })
}

unsafe extern "C" fn widget_box_primitive(orientation: sys::Scm, children: sys::Scm) -> sys::Scm {
    guarded(|| {
        let vertical = scheme_text(orientation).as_deref() == Some("vertical");
        let box_ = gtk::Box::new(
            if vertical {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            },
            6,
        );
        let mut cursor = children;
        let mut seen = 0usize;

        box_.set_valign(gtk::Align::Center);

        while sys::is_pair(cursor) {
            if let Some(handle) = scheme_integer(sys::car(cursor))
                && let Some(child) = take(handle)
            {
                box_.append(&child);
            }

            cursor = sys::cdr(cursor);
            seen += 1;

            if seen > 4096 {
                break;
            }
        }

        insert(box_.upcast())
    })
}

unsafe extern "C" fn widget_button_primitive(text: sys::Scm, procedure: sys::Scm) -> sys::Scm {
    guarded(|| {
        let Some(procedure) = ProtectedProcedure::new(procedure) else {
            return false_value();
        };
        let text = scheme_text(text).unwrap_or_default();
        let button = gtk::Button::with_label(&text);

        button.connect_clicked(move |_| procedure.call());

        insert(button.upcast())
    })
}

unsafe extern "C" fn widget_add_class_primitive(widget: sys::Scm, css: sys::Scm) -> sys::Scm {
    guarded(|| {
        if let Some(handle) = scheme_integer(widget)
            && let Some(css) = scheme_text(css)
        {
            let _ = HANDLES.with(|handles| {
                handles
                    .borrow()
                    .with(handle, |widget| widget.add_css_class(&css))
            });
        }

        widget
    })
}

/// One libguile subr entry.  `scm_c_define_gsubr` takes the C `scm_t_subr`,
/// which is `void (*)(void)`; a primitive of the wrong shape is called through
/// this pointer with the arguments libguile passes for its arity, the same
/// contract the C bridge relies on.  Keeping the cast in one place makes that
/// promise reviewable.
fn subr(primitive: *const ()) -> sys::ScmSubr {
    unsafe { std::mem::transmute::<*const (), sys::ScmSubr>(primitive) }
}

/// The primitives a `scheme-module` procedure may call, with the arities the
/// Scheme side documents.
fn primitives() -> [(&'static CStr, i32, i32, i32, sys::ScmSubr); 9] {
    [
        (
            c"whelk-widget-label",
            1,
            1,
            0,
            subr(widget_label_primitive as *const ()),
        ),
        (
            c"whelk-widget-markup",
            1,
            1,
            0,
            subr(widget_markup_primitive as *const ()),
        ),
        (
            c"whelk-widget-progress",
            1,
            0,
            0,
            subr(widget_progress_primitive as *const ()),
        ),
        (
            c"whelk-widget-level",
            1,
            0,
            0,
            subr(widget_level_primitive as *const ()),
        ),
        (
            c"whelk-widget-sparkline",
            1,
            3,
            0,
            subr(widget_sparkline_primitive as *const ()),
        ),
        (
            c"whelk-widget-image",
            1,
            1,
            0,
            subr(widget_image_primitive as *const ()),
        ),
        (
            c"whelk-widget-box",
            1,
            0,
            1,
            subr(widget_box_primitive as *const ()),
        ),
        (
            c"whelk-widget-button",
            2,
            0,
            0,
            subr(widget_button_primitive as *const ()),
        ),
        (
            c"whelk-widget-add-class",
            2,
            0,
            0,
            subr(widget_add_class_primitive as *const ()),
        ),
    ]
}

impl Guile {
    /// Define the `whelk-widget-*` primitives in `(whelk core)`.
    ///
    /// Idempotent in effect: a name defined twice is the same definition, and
    /// the C shell's bridge does the same after it has loaded the core module.
    pub(super) fn register_widget_primitives(&self) -> Result<(), GuileError> {
        let module = CString::new("whelk core").expect("static module name");
        let module = self.catch(|| Ok(sys::resolve_module(&module)))?;

        if !self.is_true(module) {
            return Err(GuileError::Evaluation(
                "could not resolve the (whelk core) module".to_owned(),
            ));
        }

        let previous = sys::set_current_module(module);
        let mut failure = None;

        for (name, required, optional, rest, procedure) in primitives() {
            let defined =
                self.catch(|| Ok(sys::define_gsubr(name, required, optional, rest, procedure)));

            if let Err(error) = defined {
                failure = Some(error);

                break;
            }

            let exported = self.catch(|| {
                sys::export(name);

                Ok(self.true_value)
            });

            if let Err(error) = exported {
                failure = Some(error);

                break;
            }
        }

        sys::set_current_module(previous);

        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// The call a module makes
// ---------------------------------------------------------------------------

struct Call<'a> {
    module: &'a CStr,
    name: &'a CStr,
    argument: &'a CStr,
    true_value: sys::Scm,
    false_value: sys::Scm,
}

unsafe extern "C" fn call_thunk(data: *mut libc::c_void) -> sys::Scm {
    let call = unsafe { &*(data as *const Call<'_>) };
    let procedure = sys::public_ref(call.module, call.name);

    if procedure.is_null() || sys::procedure_p(procedure) != call.true_value {
        return call.false_value;
    }

    sys::call_1(procedure, sys::from_utf8_string(call.argument))
}

unsafe extern "C" fn call_handler(
    data: *mut libc::c_void,
    _tag: sys::Scm,
    _arguments: sys::Scm,
) -> sys::Scm {
    let call = unsafe { &*(data as *const Call<'_>) };

    call.false_value
}

/// Call `(whelk modules) call-module-procedure` for `token` and read what it
/// built.  A missing token, an unloaded module, a raised exception, `#f` or
/// something that is not a widget all answer `None`: the module keeps the
/// widget it already shows, exactly as a failed command-backed module keeps its
/// last reading.
pub fn call_module_widget(token: &str) -> Option<gtk::Widget> {
    if token.is_empty() {
        return None;
    }

    let module = c"whelk modules";
    let name = c"call-module-procedure";
    let argument = CString::new(token).ok()?;
    let mut call = Call {
        module,
        name,
        argument: argument.as_c_str(),
        true_value: true_value(),
        false_value: false_value(),
    };
    let result = sys::internal_catch(
        call.true_value,
        call_thunk,
        (&mut call as *mut Call<'_>).cast(),
        call_handler,
        (&mut call as *mut Call<'_>).cast(),
    );

    widget_from_value(result)
}

fn widget_from_value(value: sys::Scm) -> Option<gtk::Widget> {
    if sys::is_exact_integer(value) {
        return take(sys::to_int64(value));
    }

    if sys::is_string(value) || sys::is_number(value) || is_symbol(value) {
        let text = scheme_text(value).unwrap_or_default();

        return Some(label(Some(&text), None));
    }

    None
}

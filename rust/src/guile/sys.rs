// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The complete audited libguile boundary.
//!
//! Keep the `raw` module small: every declaration there is a promise about a
//! C ABI.  `SCM` is an opaque pointer in this Guile build (`scm_unused_struct
//! *`), and `scm_t_bits` is `uintptr_t`; neither is a Rust integer that may be
//! moved between threads.  Callers only see the safe functions below, which
//! own the pointer conversions, UTF-8 copies and the `#t`/`#f` conventions.
//!
//! The safe functions are only sound while Guile has been initialized and the
//! values came from that runtime.  That is why this module is private to
//! `guile`: there is no public constructor for an `Scm`, and the runtime's
//! `Guile` handle keeps the thread-affine lifetime in one place.

use std::ffi::CStr;
use std::slice;

use libc::{c_char, c_int, c_void, size_t};

pub type Scm = *mut c_void;
pub type ScmSubr = unsafe extern "C" fn() -> Scm;
pub type CatchBody = unsafe extern "C" fn(*mut c_void) -> Scm;
pub type CatchHandler = unsafe extern "C" fn(*mut c_void, Scm, Scm) -> Scm;

mod raw {
    use super::{Scm, ScmSubr, c_char, c_int, c_void, size_t};

    unsafe extern "C" {
        pub fn scm_init_guile();
        pub fn scm_c_eval_string(text: *const c_char) -> Scm;
        pub fn scm_c_public_ref(module: *const c_char, name: *const c_char) -> Scm;
        pub fn scm_c_resolve_module(name: *const c_char) -> Scm;
        pub fn scm_set_current_module(module: Scm) -> Scm;
        pub fn scm_c_define_gsubr(
            name: *const c_char,
            required: c_int,
            optional: c_int,
            rest: c_int,
            procedure: ScmSubr,
        ) -> Scm;
        pub fn scm_c_export(name: *const c_char, ...);
        pub fn scm_call_0(procedure: Scm) -> Scm;
        pub fn scm_call_1(procedure: Scm, argument: Scm) -> Scm;
        pub fn scm_from_utf8_string(text: *const c_char) -> Scm;
        pub fn scm_to_utf8_stringn(text: Scm, length: *mut size_t) -> *mut c_char;
        pub fn scm_symbol_to_string(symbol: Scm) -> Scm;
        pub fn scm_procedure_p(value: Scm) -> Scm;
        pub fn scm_from_int64(value: i64) -> Scm;
        pub fn scm_to_int64(value: Scm) -> i64;
        pub fn scm_to_double(value: Scm) -> f64;
        pub fn scm_is_bool(value: Scm) -> c_int;
        pub fn scm_is_exact_integer(value: Scm) -> c_int;
        pub fn scm_is_number(value: Scm) -> c_int;
        pub fn scm_is_pair(value: Scm) -> c_int;
        pub fn scm_is_string(value: Scm) -> c_int;
        pub fn scm_car(pair: Scm) -> Scm;
        pub fn scm_cdr(pair: Scm) -> Scm;
        pub fn scm_gc_protect_object(value: Scm) -> Scm;
        pub fn scm_gc_unprotect_object(value: Scm) -> Scm;
        pub fn scm_internal_catch(
            tag: Scm,
            body: super::CatchBody,
            body_data: *mut c_void,
            handler: super::CatchHandler,
            handler_data: *mut c_void,
        ) -> Scm;
    }
}

/// Start Guile on this thread.  Calling it twice is a Guile-level error; the
/// `Guile` handle's process-wide guard is responsible for not doing that.
pub fn init() {
    unsafe {
        raw::scm_init_guile();
    }
}

/// Evaluate one NUL-terminated Scheme source string.
pub fn eval(source: &CStr) -> Scm {
    unsafe { raw::scm_c_eval_string(source.as_ptr()) }
}

/// Resolve `name` in the already-loaded `module`.
pub fn public_ref(module: &CStr, name: &CStr) -> Scm {
    unsafe { raw::scm_c_public_ref(module.as_ptr(), name.as_ptr()) }
}

/// Resolve a module by its Scheme name.
pub fn resolve_module(name: &CStr) -> Scm {
    unsafe { raw::scm_c_resolve_module(name.as_ptr()) }
}

/// Make `module` current and return the previous module.
pub fn set_current_module(module: Scm) -> Scm {
    unsafe { raw::scm_set_current_module(module) }
}

/// Define a primitive in the current module.
pub fn define_gsubr(
    name: &CStr,
    required: c_int,
    optional: c_int,
    rest: c_int,
    procedure: ScmSubr,
) -> Scm {
    unsafe { raw::scm_c_define_gsubr(name.as_ptr(), required, optional, rest, procedure) }
}

/// Export `name` from the current module.
pub fn export(name: &CStr) {
    unsafe {
        raw::scm_c_export(name.as_ptr(), std::ptr::null::<c_char>());
    }
}

pub fn call_0(procedure: Scm) -> Scm {
    unsafe { raw::scm_call_0(procedure) }
}

pub fn call_1(procedure: Scm, argument: Scm) -> Scm {
    unsafe { raw::scm_call_1(procedure, argument) }
}

pub fn from_utf8_string(text: &CStr) -> Scm {
    unsafe { raw::scm_from_utf8_string(text.as_ptr()) }
}

/// Copy a Scheme string into Rust, including its terminating NUL handling.
pub fn text(value: Scm) -> Option<String> {
    if !is_string(value) {
        return None;
    }

    let mut length = 0usize;
    let pointer = unsafe { raw::scm_to_utf8_stringn(value, &mut length) };

    if pointer.is_null() {
        return None;
    }

    let bytes = unsafe { slice::from_raw_parts(pointer.cast::<u8>(), length) };
    let text = String::from_utf8_lossy(bytes).into_owned();

    unsafe {
        libc::free(pointer.cast());
    }

    Some(text)
}

pub fn symbol_to_string(symbol: Scm) -> Scm {
    unsafe { raw::scm_symbol_to_string(symbol) }
}

pub fn procedure_p(value: Scm) -> Scm {
    unsafe { raw::scm_procedure_p(value) }
}

pub fn from_int64(value: i64) -> Scm {
    unsafe { raw::scm_from_int64(value) }
}

pub fn to_int64(value: Scm) -> i64 {
    unsafe { raw::scm_to_int64(value) }
}

pub fn to_double(value: Scm) -> f64 {
    unsafe { raw::scm_to_double(value) }
}

pub fn is_bool(value: Scm) -> bool {
    unsafe { raw::scm_is_bool(value) != 0 }
}

pub fn is_exact_integer(value: Scm) -> bool {
    unsafe { raw::scm_is_exact_integer(value) != 0 }
}

pub fn is_number(value: Scm) -> bool {
    unsafe { raw::scm_is_number(value) != 0 }
}

pub fn is_pair(value: Scm) -> bool {
    unsafe { raw::scm_is_pair(value) != 0 }
}

pub fn is_string(value: Scm) -> bool {
    unsafe { raw::scm_is_string(value) != 0 }
}

pub fn car(pair: Scm) -> Scm {
    unsafe { raw::scm_car(pair) }
}

pub fn cdr(pair: Scm) -> Scm {
    unsafe { raw::scm_cdr(pair) }
}

pub fn protect(value: Scm) -> Scm {
    unsafe { raw::scm_gc_protect_object(value) }
}

pub fn unprotect(value: Scm) -> Scm {
    unsafe { raw::scm_gc_unprotect_object(value) }
}

/// Run `body` with Guile's exception handler installed around it.
///
/// The caller keeps both contexts alive for the duration of the call and the
/// callbacks only touch them from Guile's own stack.  `catch` in `guile.rs` is
/// the only caller.
pub fn internal_catch(
    tag: Scm,
    body: CatchBody,
    body_data: *mut c_void,
    handler: CatchHandler,
    handler_data: *mut c_void,
) -> Scm {
    unsafe { raw::scm_internal_catch(tag, body, body_data, handler, handler_data) }
}

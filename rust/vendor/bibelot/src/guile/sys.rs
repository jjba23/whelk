// hasp -- a lock screen and session authenticator written in Rust, GTK4 and
// GNU Guile Scheme
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

/// One Guile `SCM` value.
///
/// The field is private to this module, so safe code cannot manufacture a
/// value from an arbitrary pointer: every `Scm` must come back from the
/// libguile functions below.  The transparent representation is the C ABI.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Scm(*mut c_void);

impl Scm {
    /// Guile's missing-procedure sentinel, as returned by `scm_c_public_ref`.
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

pub type CatchBody = unsafe extern "C" fn(*mut c_void) -> Scm;
pub type CatchHandler = unsafe extern "C" fn(*mut c_void, Scm, Scm) -> Scm;

mod raw {
    use super::{Scm, c_char, c_int, c_void, size_t};

    unsafe extern "C" {
        pub fn scm_init_guile();
        pub fn scm_c_eval_string(text: *const c_char) -> Scm;
        pub fn scm_c_public_ref(module: *const c_char, name: *const c_char) -> Scm;
        pub fn scm_c_resolve_module(name: *const c_char) -> Scm;
        pub fn scm_set_current_module(module: Scm) -> Scm;
        pub fn scm_call_0(procedure: Scm) -> Scm;
        pub fn scm_call_1(procedure: Scm, argument: Scm) -> Scm;
        pub fn scm_to_utf8_stringn(text: Scm, length: *mut size_t) -> *mut c_char;
        pub fn scm_symbol_to_string(symbol: Scm) -> Scm;
        pub fn scm_to_int64(value: Scm) -> i64;
        pub fn scm_is_bool(value: Scm) -> c_int;
        pub fn scm_is_exact_integer(value: Scm) -> c_int;
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

pub fn call_0(procedure: Scm) -> Scm {
    unsafe { raw::scm_call_0(procedure) }
}

pub fn call_1(procedure: Scm, argument: Scm) -> Scm {
    unsafe { raw::scm_call_1(procedure, argument) }
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

pub fn to_int64(value: Scm) -> i64 {
    unsafe { raw::scm_to_int64(value) }
}

pub fn is_bool(value: Scm) -> bool {
    unsafe { raw::scm_is_bool(value) != 0 }
}

pub fn is_exact_integer(value: Scm) -> bool {
    unsafe { raw::scm_is_exact_integer(value) != 0 }
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

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use super::Scm;

    #[test]
    fn scm_keeps_the_pointer_abi() {
        fn assert_copy<T: Copy>() {}

        assert_copy::<Scm>();
        assert_eq!(size_of::<Scm>(), size_of::<*mut std::ffi::c_void>());
        assert_eq!(align_of::<Scm>(), align_of::<*mut std::ffi::c_void>());
    }
}

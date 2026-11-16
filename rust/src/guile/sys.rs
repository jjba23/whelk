// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The complete audited libguile boundary.
//!
//! Keep this file small: every declaration here is a promise about a C ABI.
//! `SCM` is an opaque pointer in this Guile build (`scm_unused_struct *`), and
//! `scm_t_bits` is `uintptr_t`; neither is a Rust integer that may be moved
//! between threads.  The safe wrappers in `guile.rs` own all conversion and
//! rooting policy.

use libc::{c_char, c_int, c_void, size_t};

pub type Scm = *mut c_void;
pub type ScmSubr = unsafe extern "C" fn() -> Scm;
pub type CatchBody = unsafe extern "C" fn(*mut c_void) -> Scm;
pub type CatchHandler = unsafe extern "C" fn(*mut c_void, Scm, Scm) -> Scm;

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
        body: CatchBody,
        body_data: *mut c_void,
        handler: CatchHandler,
        handler_data: *mut c_void,
    ) -> Scm;
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The signal payloads logind sends, and what each one may do.

use gtk4::glib;
use whelk_rs::logind::{
    MANAGER_INTERFACE, PROPERTIES_INTERFACE, SESSION_INTERFACE, Trigger, trigger,
};

fn variant(variant_type: &str, text: &str) -> glib::Variant {
    let variant_type = glib::VariantTy::new(variant_type).expect("known variant type");

    glib::Variant::parse(Some(variant_type), text).expect("test variant parses")
}

#[test]
fn both_lock_signals_lock() {
    assert_eq!(
        trigger(MANAGER_INTERFACE, "Lock", &variant("()", "()")),
        Some(Trigger::Lock)
    );
    assert_eq!(
        trigger(SESSION_INTERFACE, "Lock", &variant("()", "()")),
        Some(Trigger::Lock)
    );
}

#[test]
fn prepare_for_sleep_locks_only_before_suspend() {
    assert_eq!(
        trigger(
            MANAGER_INTERFACE,
            "PrepareForSleep",
            &variant("(b)", "(true,)")
        ),
        Some(Trigger::Sleeping)
    );
    assert_eq!(
        trigger(
            MANAGER_INTERFACE,
            "PrepareForSleep",
            &variant("(b)", "(false,)")
        ),
        None
    );
    assert_eq!(
        trigger(MANAGER_INTERFACE, "PrepareForSleep", &variant("()", "()")),
        None
    );
    assert_eq!(
        trigger(
            MANAGER_INTERFACE,
            "PrepareForSleep",
            &variant("(s)", "('yes',)")
        ),
        None
    );
}

#[test]
fn a_vt_switch_locks_when_the_session_becomes_inactive() {
    let inactive = format!("('{SESSION_INTERFACE}', {{'Active': <false>}}, [])");
    let active = format!("('{SESSION_INTERFACE}', {{'Active': <true>}}, [])");
    let other_interface = "('org.freedesktop.login1.Manager', {'Active': <false>}, [])";
    let no_active = format!("('{SESSION_INTERFACE}', {{}}, [])");
    let only_invalidated = format!("('{SESSION_INTERFACE}', {{}}, ['Active'])");

    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &inactive)
        ),
        Some(Trigger::SessionInactive)
    );
    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &active)
        ),
        None
    );
    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", other_interface)
        ),
        None
    );
    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &no_active)
        ),
        None
    );
    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &only_invalidated)
        ),
        None
    );
}

#[test]
fn properties_changed_is_only_accepted_from_the_properties_interface() {
    let inactive = format!("('{SESSION_INTERFACE}', {{'Active': <false>}}, [])");

    assert_eq!(
        trigger(
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &inactive)
        ),
        Some(Trigger::SessionInactive)
    );
    assert_eq!(
        trigger(
            SESSION_INTERFACE,
            "PropertiesChanged",
            &variant("(sa{sv}as)", &inactive)
        ),
        None,
        "a signal named PropertiesChanged on the session interface is not one"
    );
}

#[test]
fn malformed_and_unrelated_signals_do_not_lock() {
    assert_eq!(
        trigger(MANAGER_INTERFACE, "PropertiesChanged", &variant("()", "()")),
        None
    );
    assert_eq!(
        trigger(SESSION_INTERFACE, "Unlock", &variant("()", "()")),
        None
    );
    assert_eq!(trigger("org.example", "Lock", &variant("()", "()")), None);
}

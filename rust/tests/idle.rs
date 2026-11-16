// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The idle manager without a bus: DND, the policy value, the honest
//! inhibitor failure, the IdleHint payload and the status report.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use whelk_rs::idle::{Idle, idle_hint_from_properties};

fn variant(variant_type: &str, text: &str) -> glib::Variant {
    let variant_type = glib::VariantTy::new(variant_type).expect("known variant type");

    glib::Variant::parse(Some(variant_type), text).expect("test variant parses")
}

#[test]
fn the_manager_owns_dnd_and_notifies_only_on_a_change() {
    let idle = Idle::new();
    let changes = Rc::new(Cell::new(0));
    let values = Rc::new(RefCell::new(Vec::new()));

    {
        let changes = Rc::clone(&changes);
        let values = Rc::clone(&values);

        idle.on_dnd_changed(move |dnd| {
            changes.set(changes.get() + 1);
            values.borrow_mut().push(dnd);
        });
    }

    assert!(!idle.has_dnd());

    idle.set_dnd(true);
    idle.set_dnd(true);
    idle.set_dnd(false);

    assert!(!idle.has_dnd());
    assert_eq!(changes.get(), 2);
    assert_eq!(*values.borrow(), vec![true, false]);
}

#[test]
fn the_policy_is_configurable_without_a_bus() {
    let idle = Idle::new();

    assert_eq!(idle.lock_after(), 0);

    idle.set_lock_after(300);
    assert_eq!(idle.lock_after(), 300);

    idle.set_lock_after(-1);
    assert_eq!(idle.lock_after(), -1);
}

#[test]
fn an_inhibitor_without_a_bus_fails_and_leaves_the_state_alone() {
    let idle = Idle::new();

    assert!(!idle.is_inhibited());
    assert!(!idle.set_inhibited(true, "presentation mode"));
    assert!(!idle.is_inhibited());
    assert_eq!(idle.inhibit_reason(), "");

    // Releasing something already released is not a failure.
    assert!(idle.set_inhibited(false, "presentation mode"));
}

#[test]
fn the_status_explains_the_policy_and_the_dnd_rule() {
    let idle = Idle::new();

    idle.set_lock_after(300);
    idle.set_dnd(true);

    let status = idle.describe();

    assert!(status.contains("whelk idle manager"), "{status}");
    assert!(status.contains("logind session: unavailable"), "{status}");
    assert!(status.contains("logind IdleHint:"), "{status}");
    assert!(status.contains("lock-after: 300 s"), "{status}");
    assert!(status.contains("do not disturb: on"), "{status}");
    assert!(status.contains("does not pause the lock timer"), "{status}");
    assert!(status.contains("inhibitors on the system"), "{status}");
    assert!(status.contains("Screen blanking itself"), "{status}");
}

#[test]
fn only_the_session_interface_with_an_idle_hint_changes_the_hint() {
    let session = "org.freedesktop.login1.Session";

    assert_eq!(
        idle_hint_from_properties(&variant(
            "(sa{sv}as)",
            &format!("('{session}', {{'IdleHint': <true>}}, [])")
        )),
        Some(true)
    );
    assert_eq!(
        idle_hint_from_properties(&variant(
            "(sa{sv}as)",
            &format!("('{session}', {{'IdleHint': <false>}}, [])")
        )),
        Some(false)
    );
    assert_eq!(
        idle_hint_from_properties(&variant("(sa{sv}as)", &format!("('{session}', {{}}, [])"))),
        None
    );
    assert_eq!(
        idle_hint_from_properties(&variant(
            "(sa{sv}as)",
            "('org.freedesktop.login1.Manager', {'IdleHint': <true>}, [])"
        )),
        None
    );
    assert_eq!(idle_hint_from_properties(&variant("()", "()")), None);
}

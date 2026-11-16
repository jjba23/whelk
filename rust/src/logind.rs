// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! logind's lock triggers, ported from the C locker.
//!
//! The C shell watches four signals and locks when any of them says the seat
//! is about to stop being the user's:
//!
//!   - `org.freedesktop.login1.Manager.Lock` (`loginctl lock-session`, a lock
//!     key, or another desktop asking logind to lock);
//!   - `PrepareForSleep(true)`, emitted before suspend;
//!   - `org.freedesktop.login1.Session.Lock` for this session; and
//!   - `Active=false` on the session's `PropertiesChanged`, the VT switch.
//!
//! [`trigger`] is the testable half: it maps one signal to a [`Trigger`] and
//! ignores payloads that do not say what the signal is supposed to say.
//! [`watch`] is the D-Bus half: it subscribes with the sender, path and
//! interface the bus daemon enforces, asks logind for this process's session,
//! and calls back only for the four cases above.  Locking is idempotent, so a
//! callback may arrive while the screen is already locked.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

pub const LOGIND_NAME: &str = "org.freedesktop.login1";
pub const MANAGER_PATH: &str = "/org/freedesktop/login1";
pub const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
pub const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

/// Why the session should be locked.  The variants exist so the log can say
/// which trigger fired, and so a test can tell the sleep and VT cases apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// logind's manager-level or per-session `Lock` signal.
    Lock,
    /// `PrepareForSleep(true)`: the machine is about to suspend.
    Sleeping,
    /// `Active=false`: the seat moved to another VT or session.
    SessionInactive,
}

/// Maps one signal to a lock trigger, or `None` when it does not ask for one.
///
/// The subscriptions in [`watch`] already pin sender, object path and
/// interface.  This stays strict about the payload: an unparseable
/// `PrepareForSleep` or `PropertiesChanged` is ignored rather than treated as
/// either answer.  `Lock` carries no arguments and always means lock.
pub fn trigger(
    interface_name: &str,
    signal_name: &str,
    parameters: &glib::Variant,
) -> Option<Trigger> {
    match (interface_name, signal_name) {
        (MANAGER_INTERFACE, "Lock") | (SESSION_INTERFACE, "Lock") => Some(Trigger::Lock),
        (MANAGER_INTERFACE, "PrepareForSleep") => {
            let (sleeping,) = parameters.get::<(bool,)>()?;

            sleeping.then_some(Trigger::Sleeping)
        }
        (SESSION_INTERFACE, "PropertiesChanged") => {
            let (changed_interface, changed, _invalidated) =
                parameters.get::<(String, HashMap<String, glib::Variant>, Vec<String>)>()?;

            if changed_interface != SESSION_INTERFACE {
                return None;
            }

            // `Active` missing (or only listed as invalidated) means the
            // property must be re-read, not that the session went inactive;
            // the C locker ignores that case too.
            let active = changed.get("Active")?.get::<bool>()?;

            (!active).then_some(Trigger::SessionInactive)
        }
        _ => None,
    }
}

/// Subscribes to logind on the system bus and calls `on_lock` for every
/// trigger above.  The subscriptions live for the rest of the process; a
/// failure to reach logind is logged once and leaves the caller running (the
/// lock screen is still reachable by `--lock` and by an explicit keybinding).
pub fn watch(on_lock: impl Fn() + 'static) {
    let on_lock: Rc<dyn Fn()> = Rc::new(on_lock);

    gio::bus_get(
        gio::BusType::System,
        gio::Cancellable::NONE,
        move |result| {
            let connection = match result {
                Ok(connection) => connection,
                Err(error) => {
                    log(&format!("no system bus for logind: {error}"));

                    return;
                }
            };

            // The subscriptions must outlive this callback.  Each handler
            // holds a clone of the vector that holds its own subscription;
            // that cycle is deliberate and lasts for the process lifetime.
            let subscriptions: Rc<RefCell<Vec<gio::SignalSubscription>>> =
                Rc::new(RefCell::new(Vec::new()));

            let manager_lock = connection.subscribe_to_signal(
                Some(LOGIND_NAME),
                Some(MANAGER_INTERFACE),
                Some("Lock"),
                Some(MANAGER_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                {
                    let on_lock = Rc::clone(&on_lock);
                    let keep = Rc::clone(&subscriptions);

                    move |signal| dispatch(&keep, signal, &on_lock)
                },
            );
            subscriptions.borrow_mut().push(manager_lock);

            let prepare_for_sleep = connection.subscribe_to_signal(
                Some(LOGIND_NAME),
                Some(MANAGER_INTERFACE),
                Some("PrepareForSleep"),
                Some(MANAGER_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                {
                    let on_lock = Rc::clone(&on_lock);
                    let keep = Rc::clone(&subscriptions);

                    move |signal| dispatch(&keep, signal, &on_lock)
                },
            );
            subscriptions.borrow_mut().push(prepare_for_sleep);

            // The manager-level Lock is global; the session object adds the
            // per-session Lock and the Active transition a VT switch makes.
            let session_connection = connection.clone();

            connection.call(
                Some(LOGIND_NAME),
                MANAGER_PATH,
                MANAGER_INTERFACE,
                "GetSessionByPID",
                Some(&(std::process::id(),).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
                move |result| {
                    let path = match result {
                        Ok(reply) => match reply.get::<(String,)>() {
                            Some((path,)) if !path.is_empty() => path,
                            _ => {
                                log("logind returned no session object path");

                                return;
                            }
                        },
                        Err(error) => {
                            log(&format!("could not find the logind session: {error}"));

                            return;
                        }
                    };

                    let session_lock = session_connection.subscribe_to_signal(
                        Some(LOGIND_NAME),
                        Some(SESSION_INTERFACE),
                        Some("Lock"),
                        Some(&path),
                        None,
                        gio::DBusSignalFlags::NONE,
                        {
                            let on_lock = Rc::clone(&on_lock);
                            let keep = Rc::clone(&subscriptions);

                            move |signal| dispatch(&keep, signal, &on_lock)
                        },
                    );
                    subscriptions.borrow_mut().push(session_lock);

                    let session_properties = session_connection.subscribe_to_signal(
                        Some(LOGIND_NAME),
                        Some(SESSION_INTERFACE),
                        Some("PropertiesChanged"),
                        Some(&path),
                        None,
                        gio::DBusSignalFlags::NONE,
                        {
                            let on_lock = Rc::clone(&on_lock);
                            let keep = Rc::clone(&subscriptions);

                            move |signal| dispatch(&keep, signal, &on_lock)
                        },
                    );
                    subscriptions.borrow_mut().push(session_properties);
                },
            );
        },
    );
}

fn dispatch(
    _keep_alive: &Rc<RefCell<Vec<gio::SignalSubscription>>>,
    signal: gio::DBusSignalRef,
    on_lock: &Rc<dyn Fn()>,
) {
    if let Some(reason) = trigger(signal.interface_name, signal.signal_name, signal.parameters) {
        log(&format!("logind asked to lock ({reason:?})"));
        on_lock();
    }
}

fn log(message: &str) {
    eprintln!("whelk-lock: {message}");
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The idle and inhibit manager, ported from `src/idle.c`.
//!
//! One object owns the three answers that used to be scattered:
//!
//!   - the idle timer.  The session's logind `IdleHint` starts it, and it locks
//!     after `(idle-settings #:lock-after)` seconds.  Zero means lock on the
//!     first idle hint, which is what the shell did before it had a manager;
//!   - the presentation inhibitor.  While it is on, whelk holds a real logind
//!     "idle" inhibitor (an fd from `Manager.Inhibit`) and pauses its own
//!     countdown.  The lock is a logind idle lock; a compositor that blanks on
//!     its own idle protocol is still the compositor's to control;
//!   - the do-not-disturb flag.  The notification service consults this object
//!     instead of keeping a copy.  DND silences toasts; it deliberately does
//!     not pause the lock timer, and the status says so.
//!
//! [`Idle::describe`] is the answer to "why did the screen blank, or not":
//! the hint, the timer, whelk's own lock, and every inhibitor systemd reports.
//!
//! The policy itself is in [`crate::idlepolicy`] and the inhibitor table in
//! [`crate::inhibitors`]; this module is the wiring around them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::process::Command;
use std::rc::{Rc, Weak};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::idlepolicy::{self, NO_TIMER};
use crate::inhibitors;
use crate::logind::{
    LOGIND_NAME, MANAGER_INTERFACE, MANAGER_PATH, PROPERTIES_INTERFACE, SESSION_INTERFACE,
};

const INHIBIT_TIMEOUT_MS: i32 = 5000;
const INHIBIT_REASON_DEFAULT: &str = "presentation mode";

#[derive(Clone)]
pub struct Idle {
    state: Rc<RefCell<State>>,
}

struct State {
    started: bool,
    bus: Option<gio::DBusConnection>,
    session_path: Option<String>,
    subscription: Option<gio::SignalSubscription>,

    idle: bool,
    idle_since_us: i64,
    /// Set when the presentation inhibitor is released: the elapsed idle time
    /// is frozen while it is held, so the remaining delay is honoured after.
    idle_origin_us: i64,
    inhibit_frozen_us: i64,
    inhibit_froze_idle: bool,
    lock_after: i32,
    timer: Option<glib::SourceId>,
    lock_requested: bool,

    inhibited: bool,
    inhibit_reason: String,
    /// The logind inhibitor handle.  `OwnedFd` closes it when this field is
    /// replaced or cleared, which is exactly how the logind API releases an
    /// fd-based inhibitor; no manual `close` call is needed.
    inhibit_fd: Option<OwnedFd>,

    dnd: bool,
    last_decision: String,

    lock_action: Option<Rc<dyn Fn()>>,
    dnd_changed: Vec<Rc<dyn Fn(bool)>>,
}

impl Default for Idle {
    fn default() -> Self {
        Self::new()
    }
}

impl Idle {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                started: false,
                bus: None,
                session_path: None,
                subscription: None,
                idle: false,
                idle_since_us: 0,
                idle_origin_us: 0,
                inhibit_frozen_us: 0,
                inhibit_froze_idle: false,
                lock_after: 0,
                timer: None,
                lock_requested: false,
                inhibited: false,
                inhibit_reason: String::new(),
                inhibit_fd: None,
                dnd: false,
                last_decision: String::new(),
                lock_action: None,
                dnd_changed: Vec::new(),
            })),
        }
    }

    /// Starts watching logind.  Idempotent; the manager still works (with no
    /// bus) as the owner of DND and the presentation toggle if logind is
    /// absent.
    pub fn start(&self) {
        {
            let mut state = self.state.borrow_mut();

            if state.started {
                return;
            }

            state.started = true;
        }

        let weak = Rc::downgrade(&self.state);

        gio::bus_get(
            gio::BusType::System,
            gio::Cancellable::NONE,
            move |result| {
                let Some(state) = weak.upgrade() else {
                    return;
                };

                if !state.borrow().started {
                    return;
                }

                let bus = match result {
                    Ok(bus) => bus,
                    Err(error) => {
                        log_warn(&format!(
                            "no system bus: the idle hint and logind inhibitor are \
                             unavailable: {error}"
                        ));

                        return;
                    }
                };

                state.borrow_mut().bus = Some(bus.clone());

                let weak = Rc::downgrade(&state);

                bus.call(
                    Some(LOGIND_NAME),
                    MANAGER_PATH,
                    MANAGER_INTERFACE,
                    "GetSessionByPID",
                    Some(&(std::process::id(),).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    INHIBIT_TIMEOUT_MS,
                    gio::Cancellable::NONE,
                    move |result| {
                        let Some(state) = weak.upgrade() else {
                            return;
                        };

                        if !state.borrow().started {
                            return;
                        }

                        let path = match result {
                            Ok(reply) => match reply.get::<(String,)>() {
                                Some((path,)) if !path.is_empty() => path,
                                _ => {
                                    log_debug("logind returned no session for the idle hint");

                                    return;
                                }
                            },
                            Err(error) => {
                                log_debug(&format!(
                                    "could not find the logind session for the idle hint: {error}"
                                ));

                                return;
                            }
                        };

                        subscribe_idle_hint(&state, &path);

                        // A shell restarted while the session is already idle
                        // has to learn the current hint, not wait for the next
                        // transition.
                        read_current_hint(&state);
                    },
                );
            },
        );
    }

    /// Stops the timer, releases whelk's inhibitor fd and drops the logind
    /// watch.  Closing the handle is what releases an fd-based logind
    /// inhibitor; there is no separate Release method.
    pub fn shutdown(&self) {
        let mut state = self.state.borrow_mut();

        state.started = false;

        if let Some(timer) = state.timer.take() {
            timer.remove();
        }

        state.subscription = None;
        state.session_path = None;
        state.bus = None;

        state.inhibit_fd = None;
        state.inhibited = false;
        state.inhibit_froze_idle = false;
        state.inhibit_reason.clear();
    }

    /// What to run when the timer fires.  The shell passes its lock path, so
    /// the manager does not need to know about lock screens.
    pub fn set_lock_action<F: Fn() + 'static>(&self, action: F) {
        self.state.borrow_mut().lock_action = Some(Rc::new(action));
    }

    /// Seconds of idle before locking; 0 locks immediately, negative disables.
    pub fn set_lock_after(&self, seconds: i32) {
        if self.state.borrow().lock_after == seconds {
            return;
        }

        {
            let mut state = self.state.borrow_mut();
            state.lock_after = seconds;
            state.lock_requested = false;
        }

        log_info(&format!(
            "idle policy: {}",
            if seconds < 0 {
                "timed locking disabled".to_owned()
            } else if seconds == 0 {
                "lock as soon as logind reports idle".to_owned()
            } else {
                format!("lock after {seconds} s of idle")
            }
        ));

        schedule_timer(&self.state);
    }

    pub fn lock_after(&self) -> i32 {
        self.state.borrow().lock_after
    }

    pub fn is_idle(&self) -> bool {
        self.state.borrow().idle
    }

    /// Presentation mode: take or release a logind idle inhibitor.  Returns
    /// `false` when the lock could not be taken (usually: no system bus), and
    /// leaves the state unchanged so a caller can report an honest failure.
    pub fn set_inhibited(&self, inhibited: bool, reason: &str) -> bool {
        if self.state.borrow().inhibited == inhibited {
            return true;
        }

        if inhibited {
            let Some(bus) = self.state.borrow().bus.clone() else {
                log_warn("cannot take the idle inhibitor: no system bus yet");

                return false;
            };
            let why = if reason.is_empty() {
                INHIBIT_REASON_DEFAULT
            } else {
                reason
            };
            let parameters = ("idle", "whelk", why, "block").to_variant();
            let reply = bus.call_with_unix_fd_list_sync(
                Some(LOGIND_NAME),
                MANAGER_PATH,
                MANAGER_INTERFACE,
                "Inhibit",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                INHIBIT_TIMEOUT_MS,
                gio::UnixFDList::NONE,
                gio::Cancellable::NONE,
            );

            let fd = match reply {
                Ok((reply, Some(fd_list))) => match reply.get::<(i32,)>() {
                    Some((handle,)) => unix_fd_from_list(&fd_list, handle),
                    None => None,
                },
                Ok((_reply, None)) => None,
                Err(error) => {
                    log_warn(&format!(
                        "could not take the logind idle inhibitor: {error}"
                    ));

                    return false;
                }
            };
            let Some(fd) = fd else {
                log_warn("logind returned no inhibitor handle");

                return false;
            };

            {
                let mut state = self.state.borrow_mut();

                state.inhibit_frozen_us = idle_elapsed_us(&state);
                state.inhibit_froze_idle = state.idle;
                state.inhibit_fd = Some(fd);
                state.inhibited = true;
                state.inhibit_reason = why.to_owned();
            }

            log_info(&format!("presentation inhibitor held (logind idle): {why}"));
        } else {
            {
                let mut state = self.state.borrow_mut();

                state.inhibit_fd = None;
                state.inhibited = false;
                state.inhibit_reason.clear();

                // Resume the clock where it stopped, not where wall time has
                // reached.  The frozen span can legitimately be zero, so the
                // boolean, not the number, says whether there was an idle
                // session to resume.
                if state.idle && state.inhibit_froze_idle {
                    state.idle_origin_us = now_us() - state.inhibit_frozen_us;
                }

                state.inhibit_frozen_us = 0;
                state.inhibit_froze_idle = false;
            }

            log_info("presentation inhibitor released");
        }

        // The timer is suspended while the inhibitor is held, and picks up
        // again with the remaining delay when it is released.
        schedule_timer(&self.state);

        true
    }

    pub fn is_inhibited(&self) -> bool {
        self.state.borrow().inhibited
    }

    pub fn inhibit_reason(&self) -> String {
        self.state.borrow().inhibit_reason.clone()
    }

    pub fn has_dnd(&self) -> bool {
        self.state.borrow().dnd
    }

    /// Adds the notification service's hook; every registered callback runs
    /// once per actual change.
    pub fn on_dnd_changed<F: Fn(bool) + 'static>(&self, callback: F) {
        self.state.borrow_mut().dnd_changed.push(Rc::new(callback));
    }

    pub fn set_dnd(&self, dnd: bool) {
        let callback = {
            let mut state = self.state.borrow_mut();

            if state.dnd == dnd {
                return;
            }

            state.dnd = dnd;
            state.dnd_changed.clone()
        };

        log_info(&format!(
            "do not disturb: {} (does not pause the lock timer)",
            if dnd { "on" } else { "off" }
        ));

        for callback in callback {
            callback(dnd);
        }
    }

    /// A human-readable report, including the system's inhibitor list.
    pub fn describe(&self) -> String {
        let (
            session_path,
            idle,
            idle_for,
            lock_after,
            inhibited,
            lock_requested,
            delay,
            last_decision,
            inhibit_reason,
            dnd,
        ) = {
            let state = self.state.borrow();
            let idle_for = idle_seconds(&state);
            let delay = idlepolicy::lock_delay(
                state.lock_after as i64,
                state.idle,
                state.inhibited,
                idle_for,
            );

            (
                state.session_path.clone(),
                state.idle,
                idle_for,
                state.lock_after,
                state.inhibited,
                state.lock_requested,
                delay,
                state.last_decision.clone(),
                state.inhibit_reason.clone(),
                state.dnd,
            )
        };

        let mut out = String::from("whelk idle manager\n");

        out.push_str(&format!(
            "  logind session: {}\n",
            session_path
                .as_deref()
                .unwrap_or("unavailable -- no system bus or session lookup")
        ));
        out.push_str(&format!(
            "  logind IdleHint: {}",
            if idle { "idle" } else { "active" }
        ));

        if idle {
            out.push_str(&format!(" ({idle_for} s)"));
        }

        out.push('\n');
        out.push_str(&format!(
            "  lock-after: {lock_after} s (idle-settings #:lock-after; 0 locks immediately)\n"
        ));
        out.push_str("  lock timer: ");

        if inhibited {
            out.push_str("suspended while the presentation inhibitor is held\n");
        } else if !idle {
            out.push_str("not running: the session is active\n");
        } else if lock_requested {
            out.push_str("fired; the lock has been requested\n");
        } else if delay == 0 {
            out.push_str("firing now\n");
        } else if delay == NO_TIMER {
            out.push_str("disabled by the policy\n");
        } else {
            out.push_str(&format!("{delay} s remaining\n"));
        }

        out.push_str(&format!(
            "  last decision: {}\n",
            if last_decision.is_empty() {
                "(none yet)"
            } else {
                &last_decision
            }
        ));

        if inhibited {
            out.push_str(&format!(
                "  presentation inhibitor: on ({}) -- whelk holds logind's idle \
                 lock and its own timer is paused; the compositor's blanking is \
                 still the compositor's\n",
                if inhibit_reason.is_empty() {
                    "no reason given"
                } else {
                    &inhibit_reason
                }
            ));
        } else {
            out.push_str("  presentation inhibitor: off -- run `whelk --inhibit` to hold one\n");
        }

        out.push_str(&format!(
            "  do not disturb: {} (DND silences toasts; it does not pause the lock timer)\n",
            if dnd { "on" } else { "off" }
        ));
        out.push_str("  inhibitors on the system (including whelk's own):\n");

        match Command::new("systemd-inhibit")
            .args(["--list", "--no-pager"])
            .output()
        {
            Ok(output) => {
                let list = inhibitors::parse(&String::from_utf8_lossy(&output.stdout));

                if let Some(description) = inhibitors::describe(&list) {
                    for line in description.lines() {
                        out.push_str(&format!("    {line}\n"));
                    }
                } else {
                    out.push_str("    none held\n");
                }
            }
            Err(error) => {
                out.push_str(&format!("    could not read systemd-inhibit: {error}\n"));
            }
        }

        out.push_str(
            "  Screen blanking itself belongs to the compositor and logind; whelk \
             owns the lock timer and its own idle inhibitor.\n",
        );

        out
    }
}

/// Reads `IdleHint` out of a session `PropertiesChanged` payload.  Public so
/// the payload policy is tested without a bus.
pub fn idle_hint_from_properties(parameters: &glib::Variant) -> Option<bool> {
    let (changed_interface, changed, _invalidated) =
        parameters.get::<(String, HashMap<String, glib::Variant>, Vec<String>)>()?;

    if changed_interface != SESSION_INTERFACE {
        return None;
    }

    changed.get("IdleHint")?.get::<bool>()
}

fn subscribe_idle_hint(state: &Rc<RefCell<State>>, path: &str) {
    let Some(bus) = state.borrow().bus.clone() else {
        return;
    };
    let weak = Rc::downgrade(state);
    let subscription = bus.subscribe_to_signal(
        Some(LOGIND_NAME),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
        Some(path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(idle) = idle_hint_from_properties(signal.parameters) else {
                return;
            };
            let Some(state) = weak.upgrade() else {
                return;
            };

            apply_idle_hint(&state, idle);
        },
    );

    let mut state = state.borrow_mut();
    state.session_path = Some(path.to_owned());
    state.subscription = Some(subscription);
}

fn read_current_hint(state: &Rc<RefCell<State>>) {
    let session = state.borrow();
    let (Some(bus), Some(path)) = (session.bus.clone(), session.session_path.clone()) else {
        return;
    };

    drop(session);

    let reply = bus.call_sync(
        Some(LOGIND_NAME),
        &path,
        "org.freedesktop.DBus.Properties",
        "Get",
        Some(&(SESSION_INTERFACE, "IdleHint").to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        INHIBIT_TIMEOUT_MS,
        gio::Cancellable::NONE,
    );

    match reply {
        Ok(reply) => {
            if let Some((value,)) = reply.get::<(glib::Variant,)>()
                && let Some(idle) = value.get::<bool>()
            {
                apply_idle_hint(state, idle);
            }
        }
        Err(error) => log_debug(&format!("could not read the current idle hint: {error}")),
    }
}

fn apply_idle_hint(state: &Rc<RefCell<State>>, idle: bool) {
    if state.borrow().idle == idle {
        return;
    }

    {
        let mut state = state.borrow_mut();
        state.idle = idle;
        state.idle_since_us = if idle { now_us() } else { 0 };
        state.idle_origin_us = 0;
        state.inhibit_frozen_us = 0;
        state.inhibit_froze_idle = false;
        state.lock_requested = false;
    }

    log_info(if idle {
        "logind reports the session idle"
    } else {
        "logind reports the session active"
    });

    schedule_timer(state);
}

fn schedule_timer(state: &Rc<RefCell<State>>) {
    if let Some(timer) = state.borrow_mut().timer.take() {
        timer.remove();
    }

    let (delay, decision) = {
        let state = state.borrow();
        let idle_for = idle_seconds(&state);
        let delay = idlepolicy::lock_delay(
            state.lock_after as i64,
            state.idle,
            state.inhibited,
            idle_for,
        );
        let decision = if delay == NO_TIMER {
            if !state.idle {
                "no timer: the session is active".to_owned()
            } else if state.inhibited {
                "timer suspended: whelk holds the idle inhibitor".to_owned()
            } else {
                "timed locking is disabled".to_owned()
            }
        } else if delay == 0 {
            if state.lock_after == 0 {
                "locking as soon as logind reports idle".to_owned()
            } else {
                "the idle delay has already elapsed: locking now".to_owned()
            }
        } else {
            format!(
                "lock timer armed: {delay} s left of the {} s policy",
                state.lock_after
            )
        };

        (delay, decision)
    };

    state.borrow_mut().last_decision = decision.clone();

    if delay == NO_TIMER {
        log_debug(&decision);

        return;
    }

    log_info(&decision);

    let weak = Rc::downgrade(state);
    let source = if delay == 0 {
        glib::idle_add_local_once(move || on_lock_timer(&weak))
    } else {
        glib::timeout_add_seconds_local_once(delay as u32, move || on_lock_timer(&weak))
    };

    state.borrow_mut().timer = Some(source);
}

fn on_lock_timer(state: &Weak<RefCell<State>>) {
    let Some(state) = state.upgrade() else {
        return;
    };

    state.borrow_mut().timer = None;

    let (remaining, lock_action) = {
        let mut state = state.borrow_mut();

        if !state.started || !state.idle || state.inhibited {
            return;
        }

        let idle_for = idle_seconds(&state);
        let remaining = idlepolicy::lock_delay(
            state.lock_after as i64,
            state.idle,
            state.inhibited,
            idle_for,
        );

        if remaining > 0 {
            state.last_decision = format!(
                "waiting: {remaining} s of the {} s policy left",
                state.lock_after
            );

            (remaining, None)
        } else {
            state.lock_requested = true;
            state.last_decision = format!("lock timer fired after {idle_for} s idle");

            (0, state.lock_action.clone())
        }
    };

    if remaining > 0 {
        let weak = Rc::downgrade(&state);
        let source =
            glib::timeout_add_seconds_local_once(remaining as u32, move || on_lock_timer(&weak));

        state.borrow_mut().timer = Some(source);

        return;
    }

    log_info(&state.borrow().last_decision);

    if let Some(action) = lock_action {
        action();
    }
}

fn unix_fd_from_list(list: &gio::UnixFDList, handle: i32) -> Option<OwnedFd> {
    match list.get(handle) {
        Ok(fd) => Some(fd),
        Err(error) => {
            log_warn(&format!(
                "logind returned an inhibitor fd that could not be duplicated: {error}"
            ));

            None
        }
    }
}

fn now_us() -> i64 {
    glib::monotonic_time()
}

fn idle_elapsed_us(state: &State) -> i64 {
    if !state.idle {
        return 0;
    }

    let origin = if state.idle_origin_us > 0 {
        state.idle_origin_us
    } else {
        state.idle_since_us
    };

    if origin <= 0 {
        return 0;
    }

    let elapsed = now_us() - origin;

    elapsed.max(0)
}

fn idle_seconds(state: &State) -> i64 {
    idle_elapsed_us(state) / 1_000_000
}

fn log_debug(message: &str) {
    eprintln!("whelk-idle: {message}");
}

fn log_info(message: &str) {
    eprintln!("whelk-idle: {message}");
}

fn log_warn(message: &str) {
    eprintln!("whelk-idle: {message}");
}

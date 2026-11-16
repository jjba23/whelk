// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Rust lock screen, ported from `src/locker.c`.
//!
//! Locking is not a panel that can be clicked away.  On a compositor that
//! implements `ext-session-lock-v1`, whelk asks the compositor for a session
//! lock: the compositor owns the surfaces, nothing can be placed above them,
//! and the session stays locked if this process dies.  A compositor without
//! the protocol (and X11) gets one overlay layer surface per output with the
//! keyboard taken exclusively on the primary output; that path is a
//! compatibility fallback, not a claim of compositor enforcement.
//!
//! The security properties are the C locker's, unchanged: fail closed, no
//! username field, one attempt at a time on a worker thread, the password
//! wiped as soon as PAM has seen it, a doubling delay after each refusal, and
//! no close button, Escape or click-outside that ends the lock.  There is no
//! command-line or D-Bus unlock path.
//!
//! Not ported yet: logind's `Lock`/`PrepareForSleep` subscriptions and the
//! per-monitor rebuild while the protocol lock is up.  They remain in the C
//! shell until the Rust binary becomes the installed `whelk`.

use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::lockauth::{self, AuthOutcome};
use crate::lockpolicy;

/// The handful of `gtk4-session-lock.h` entry points this slice calls.
///
/// The linked `gtk4-layer-shell` (1.2 or newer) exports them; the Rust crate's
/// bindings predate the session-lock half, so the declarations live here, next
/// to the code that uses them.  The library's own binding is used at run time
/// through the same soname the layer-shell crate links.
mod sys {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub fn gtk_session_lock_is_supported() -> i32;
        pub fn gtk_session_lock_instance_new() -> *mut c_void;
        pub fn gtk_session_lock_instance_lock(instance: *mut c_void) -> i32;
        pub fn gtk_session_lock_instance_unlock(instance: *mut c_void);
        pub fn gtk_session_lock_instance_assign_window_to_monitor(
            instance: *mut c_void,
            window: *mut c_void,
            monitor: *mut c_void,
        );
    }
}

const CSS: &str = "
    .whelk-lock-background {
        background-color: rgba(12, 14, 18, 0.92);
    }
    .whelk-lock-panel {
        padding: 32px;
        border-radius: 18px;
        background-color: rgba(24, 27, 34, 0.86);
    }
    .whelk-lock-clock {
        font-size: 64px;
        font-weight: 300;
    }
    .whelk-lock-date {
        font-size: 18px;
        opacity: 0.72;
    }
    .whelk-lock-message {
        margin-top: 8px;
        opacity: 0.82;
    }
    .whelk-lock-error {
        color: #ff8f8f;
        min-height: 24px;
    }
";

struct Locker {
    application: gtk::Application,
    windows: Vec<gtk::ApplicationWindow>,
    entry: Option<gtk::PasswordEntry>,
    error: Option<gtk::Label>,
    clocks: Vec<gtk::Label>,
    dates: Vec<gtk::Label>,
    failures: u32,
    authenticating: bool,
    retrying: bool,
    unlocked: bool,
    session_lock: Option<glib::Object>,
    session_primary: bool,
    retry_source: Option<glib::SourceId>,
    /// Keeps the application alive while no lock window is mapped yet (the
    /// compositor presents the protocol windows itself, after `lock()`).
    hold: Option<gtk::gio::ApplicationHoldGuard>,
}

use gtk::glib;

/// Runs the lock screen until a password is accepted.
pub fn run() -> glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("org.jointhefreeworld.whelk.rust.lock")
        .build();

    application.connect_activate(lock);

    application.run_with_args(&["whelk-rs-lock"])
}

fn lock(application: &gtk::Application) {
    install_css();

    let state = Rc::new(RefCell::new(Locker {
        application: application.clone(),
        windows: Vec::new(),
        entry: None,
        error: None,
        clocks: Vec::new(),
        dates: Vec::new(),
        failures: load_failures(),
        authenticating: false,
        retrying: false,
        unlocked: false,
        session_lock: None,
        session_primary: false,
        retry_source: None,
        hold: None,
    }));

    let session = start_session_lock(&state);

    if !session {
        build_fallback_windows(&state);
    }

    // A restart must not give the next guess a free attempt.
    if state.borrow().failures > 0 {
        apply_backoff(&state);
    }

    let weak = Rc::downgrade(&state);
    glib::timeout_add_seconds_local(1, move || {
        if let Some(state) = weak.upgrade() {
            tick_clock(&state);
        }

        glib::ControlFlow::Continue
    });

    tick_clock(&state);

    // The application has no visible window until the compositor or the
    // fallback presents one; do not let GTK quit in the gap.
    state.borrow_mut().hold = Some(application.hold());
}

// ---------------------------------------------------------------------------
// The compositor-enforced path
// ---------------------------------------------------------------------------

fn start_session_lock(state: &Rc<RefCell<Locker>>) -> bool {
    if unsafe { sys::gtk_session_lock_is_supported() } == 0 {
        return false;
    }

    let pointer = unsafe { sys::gtk_session_lock_instance_new() };

    if pointer.is_null() {
        return false;
    }

    let instance: glib::Object =
        unsafe { glib::translate::from_glib_full(pointer as *mut glib::gobject_ffi::GObject) };

    let weak = Rc::downgrade(state);
    instance.connect_local("monitor", false, move |values| {
        if let Some(monitor) = values
            .get(1)
            .and_then(|value| value.get::<gtk::gdk::Monitor>().ok())
        {
            if let Some(state) = weak.upgrade() {
                create_session_window(&state, &monitor);
            }
        }

        None
    });

    let weak = Rc::downgrade(state);
    instance.connect_local("locked", false, move |_| {
        log("screen locked (ext-session-lock-v1)");

        let _ = weak.upgrade();

        None
    });

    instance.connect_local("failed", false, move |_| {
        log("ext-session-lock-v1 refused the lock; the session stays locked");

        None
    });

    let weak = Rc::downgrade(state);
    instance.connect_local("unlocked", false, move |_| {
        // The compositor ended the lock on its own; do not ask it again.
        if let Some(state) = weak.upgrade() {
            if !state.borrow().unlocked {
                unlock_local(&state);
            }
        }

        None
    });

    let acquired = unsafe {
        sys::gtk_session_lock_instance_lock(instance.as_ptr().cast::<std::ffi::c_void>())
    } != 0;

    if !acquired {
        // Another locker may already own the session.  A weaker layer surface
        // would not be that lock, so there is deliberately no fallback here.
        log("ext-session-lock-v1 could not start");
    }

    state.borrow_mut().session_lock = Some(instance);

    true
}

fn create_session_window(state: &Rc<RefCell<Locker>>, monitor: &gtk::gdk::Monitor) {
    let primary = {
        let mut locker = state.borrow_mut();

        if locker.session_primary {
            false
        } else {
            locker.session_primary = true;
            true
        }
    };

    let window = gtk::ApplicationWindow::builder()
        .application(&state.borrow().application)
        .title("whelk lock")
        .build();

    window.set_deletable(false);
    window.add_css_class("whelk-lock");

    // There is no window-manager close, no Escape and no click outside that
    // ends a lock.
    window.connect_close_request(|_| glib::Propagation::Stop);

    let content = build_content(state, primary);

    window.set_child(Some(&content));

    state.borrow_mut().windows.push(window.clone());

    let instance = state.borrow().session_lock.clone();

    if let Some(instance) = instance {
        unsafe {
            sys::gtk_session_lock_instance_assign_window_to_monitor(
                instance.as_ptr().cast::<std::ffi::c_void>(),
                window.as_ptr().cast::<std::ffi::c_void>(),
                monitor.as_ptr().cast::<std::ffi::c_void>(),
            );
        }
    }

    if primary {
        focus_entry(&state.borrow());
    }

    tick_clock(state);
}

// ---------------------------------------------------------------------------
// The compatibility path
// ---------------------------------------------------------------------------

fn build_fallback_windows(state: &Rc<RefCell<Locker>>) {
    let monitors: Vec<gtk::gdk::Monitor> = gtk::gdk::Display::default()
        .map(|display| {
            let model = display.monitors();
            let mut monitors = Vec::new();

            for index in 0..model.n_items() {
                if let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() {
                    monitors.push(monitor);
                }
            }

            monitors
        })
        .unwrap_or_default();

    if monitors.is_empty() {
        log("no display to cover: the session is locked without a visible surface");

        return;
    }

    for (index, monitor) in monitors.iter().enumerate() {
        create_fallback_window(state, monitor, index == 0);
    }
}

fn create_fallback_window(state: &Rc<RefCell<Locker>>, monitor: &gtk::gdk::Monitor, primary: bool) {
    let window = gtk::ApplicationWindow::builder()
        .application(&state.borrow().application)
        .title("whelk lock")
        .build();

    window.set_deletable(false);
    window.add_css_class("whelk-lock");
    window.connect_close_request(|_| glib::Propagation::Stop);
    window.set_child(Some(&build_content(state, primary)));

    if crate::ui::layer_shell_available() {
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("whelk-lock"));
        window.set_monitor(Some(monitor));

        for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
            window.set_anchor(edge, true);
        }

        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(if primary {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::None
        });
    } else {
        // X11 and Broadway have no layer shell: an ordinary fullscreen window
        // is the documented compatibility path.
        window.set_default_size(1280, 800);
        window.fullscreen();
    }

    state.borrow_mut().windows.push(window.clone());

    window.present();

    if primary {
        focus_entry(&state.borrow());
    }

    tick_clock(state);
}

// ---------------------------------------------------------------------------
// The surfaces' content
// ---------------------------------------------------------------------------

fn build_content(state: &Rc<RefCell<Locker>>, primary: bool) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    let background = gtk::Box::new(gtk::Orientation::Vertical, 0);

    background.add_css_class("whelk-lock-background");
    background.set_hexpand(true);
    background.set_vexpand(true);
    overlay.set_child(Some(&background));

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);

    panel.add_css_class("whelk-lock-panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    overlay.add_overlay(&panel);

    let clock = gtk::Label::new(None);

    clock.add_css_class("whelk-lock-clock");
    panel.append(&clock);

    let date = gtk::Label::new(None);

    date.add_css_class("whelk-lock-date");
    panel.append(&date);

    let message = gtk::Label::new(Some(if primary {
        "Enter your password"
    } else {
        "Locked"
    }));

    message.add_css_class("whelk-lock-message");
    panel.append(&message);

    {
        let mut locker = state.borrow_mut();

        locker.clocks.push(clock);
        locker.dates.push(date);
    }

    if !primary {
        return overlay;
    }

    let entry = gtk::PasswordEntry::new();

    entry.set_show_peek_icon(false);
    entry.set_size_request(320, -1);
    entry.set_halign(gtk::Align::Center);
    panel.append(&entry);

    let entry_for_activate = entry.clone();
    let weak = Rc::downgrade(state);

    entry.connect_activate(move |entry| {
        if let Some(state) = weak.upgrade() {
            submit(&state, entry.text().to_string());
        }

        let _ = &entry_for_activate;
    });

    let unlock = gtk::Button::with_label("Unlock");

    unlock.add_css_class("whelk-lock-unlock");
    unlock.set_halign(gtk::Align::Center);
    panel.append(&unlock);

    let weak = Rc::downgrade(state);

    unlock.connect_clicked(move |_| {
        if let Some(state) = weak.upgrade() {
            let text = state
                .borrow()
                .entry
                .as_ref()
                .map(|entry| entry.text().to_string())
                .unwrap_or_default();

            submit(&state, text);
        }
    });

    let error = gtk::Label::new(None);

    error.add_css_class("whelk-lock-error");
    panel.append(&error);

    {
        let mut locker = state.borrow_mut();

        locker.entry = Some(entry);
        locker.error = Some(error);
    }

    overlay
}

fn focus_entry(locker: &Locker) {
    if let Some(entry) = &locker.entry {
        entry.grab_focus();
    }
}

fn tick_clock(state: &Rc<RefCell<Locker>>) {
    let Ok(now) = glib::DateTime::now_local() else {
        return;
    };

    let time = now
        .format("%H:%M")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "--:--".to_owned());
    let date = now
        .format("%A, %d %B")
        .map(|value| value.to_string())
        .unwrap_or_default();

    let locker = state.borrow();

    for label in &locker.clocks {
        label.set_text(&time);
    }

    for label in &locker.dates {
        label.set_text(&date);
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

fn submit(state: &Rc<RefCell<Locker>>, password: String) {
    {
        let mut locker = state.borrow_mut();
        let sensitive = locker
            .entry
            .as_ref()
            .map(gtk::prelude::WidgetExt::is_sensitive)
            .unwrap_or(false);

        if locker.unlocked
            || !lockpolicy::submission_allowed(locker.authenticating, locker.retrying, sensitive)
        {
            return;
        }

        if password.is_empty() {
            show_error(&locker, "Enter your password.");
            focus_entry(&locker);

            return;
        }

        locker.authenticating = true;

        if let Some(entry) = &locker.entry {
            entry.set_text("");
            entry.set_sensitive(false);
        }

        show_error(&locker, "Checking…");
    }

    let mut secret = password.into_bytes();
    let weak = Rc::downgrade(state);

    glib::spawn_future_local(async move {
        let outcome = gtk::gio::spawn_blocking(move || {
            let outcome = lockauth::authenticate(&mut secret);

            lockauth::wipe(&mut secret);

            outcome
        })
        .await
        .unwrap_or(AuthOutcome::Unavailable);

        if let Some(state) = weak.upgrade() {
            delivered(&state, outcome);
        }
    });
}

fn delivered(state: &Rc<RefCell<Locker>>, outcome: AuthOutcome) {
    {
        let mut locker = state.borrow_mut();

        if locker.unlocked {
            return;
        }

        locker.authenticating = false;

        if outcome == AuthOutcome::Ok {
            drop(locker);
            unlock(state);

            return;
        }

        locker.failures = locker.failures.saturating_add(1);
        save_failures(locker.failures);

        if let Some(entry) = &locker.entry {
            entry.set_sensitive(false);
        }

        show_error(
            &locker,
            if outcome == AuthOutcome::Denied {
                "Incorrect password."
            } else {
                "Authentication is unavailable. The screen stays locked."
            },
        );

        locker.retrying = true;

        let delay = lockpolicy::retry_delay_ms(locker.failures);
        let weak = Rc::downgrade(state);
        let source = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            if let Some(state) = weak.upgrade() {
                let mut locker = state.borrow_mut();

                locker.retrying = false;
                locker.retry_source = None;

                if locker.unlocked {
                    return;
                }

                if let Some(entry) = &locker.entry {
                    entry.set_text("");
                    entry.set_sensitive(true);
                    entry.grab_focus();
                }

                show_error(&locker, "Try again.");
            }
        });

        locker.retry_source = Some(source);
    }
}

fn apply_backoff(state: &Rc<RefCell<Locker>>) {
    let mut locker = state.borrow_mut();

    if locker.failures == 0 || locker.retrying || locker.unlocked {
        return;
    }

    if let Some(entry) = &locker.entry {
        entry.set_sensitive(false);
    }

    show_error(&locker, "Wait before trying again.");

    let delay = lockpolicy::retry_delay_ms(locker.failures);
    let weak = Rc::downgrade(state);
    let source = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
        if let Some(state) = weak.upgrade() {
            let mut locker = state.borrow_mut();

            locker.retrying = false;
            locker.retry_source = None;

            if locker.unlocked {
                return;
            }

            if let Some(entry) = &locker.entry {
                entry.set_text("");
                entry.set_sensitive(true);
                entry.grab_focus();
            }

            show_error(&locker, "Try again.");
        }
    });

    locker.retrying = true;
    locker.retry_source = Some(source);
}

fn show_error(locker: &Locker, message: &str) {
    if let Some(error) = &locker.error {
        error.set_text(message);
    }
}

/// Ends the local lock state after a successful PAM authentication.
fn unlock(state: &Rc<RefCell<Locker>>) {
    let (windows, session, application) = {
        let mut locker = state.borrow_mut();

        if locker.unlocked {
            return;
        }

        locker.unlocked = true;
        locker.retrying = false;
        locker.authenticating = false;

        if let Some(source) = locker.retry_source.take() {
            source.remove();
        }

        (
            std::mem::take(&mut locker.windows),
            locker.session_lock.take(),
            locker.application.clone(),
        )
    };

    clear_failures();

    match session {
        Some(instance) => {
            // The compositor owns these windows; ask it to end the lock and
            // only drop the references.
            unsafe {
                sys::gtk_session_lock_instance_unlock(instance.as_ptr().cast::<std::ffi::c_void>())
            };

            drop(windows);
        }
        None => {
            for window in windows {
                window.destroy();
            }
        }
    }

    application.quit();
}

/// The compositor ended the lock itself: forget the state without asking it
/// to unlock a second time.
fn unlock_local(state: &Rc<RefCell<Locker>>) {
    let application = {
        let mut locker = state.borrow_mut();

        if locker.unlocked {
            return;
        }

        locker.unlocked = true;

        if let Some(source) = locker.retry_source.take() {
            source.remove();
        }

        locker.session_lock = None;
        locker.windows.clear();
        locker.application.clone()
    };

    clear_failures();
    application.quit();
}

// ---------------------------------------------------------------------------
// The failed-attempt count
// ---------------------------------------------------------------------------

fn failure_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WHELK_LOCK_STATE_FILE") {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("whelk").join("lock-failures"))
}

fn load_failures() -> u32 {
    let Some(path) = failure_path() else {
        return 0;
    };

    fs::read_to_string(path)
        .ok()
        .and_then(|contents| contents.trim().parse::<u32>().ok())
        .unwrap_or(0)
        .min(60)
}

fn save_failures(failures: u32) {
    let Some(path) = failure_path() else {
        return;
    };

    let Some(directory) = path.parent() else {
        return;
    };

    if fs::create_dir_all(directory).is_err() {
        log("could not create the lock-state directory");

        return;
    }

    let _ = fs::set_permissions(
        directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    );

    let result = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut file| {
            use std::io::Write;

            writeln!(file, "{failures}")
        });

    if result.is_err() {
        log("could not persist the failed-attempt count");
    }
}

fn clear_failures() {
    if let Some(path) = failure_path() {
        let _ = fs::remove_file(path);
    }
}

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

fn install_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let css = gtk::CssProvider::new();

    css.load_from_string(CSS);

    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn log(message: &str) {
    eprintln!("whelk-lock: {message}");
}

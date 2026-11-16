// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The notification service and its store, ported from `src/notifications.c`.
//!
//! Whelk owns `org.freedesktop.Notifications`: the same process that draws the
//! bar decides how a notification is stored, grouped, and when it is allowed to
//! interrupt.  The store is deliberately independent of D-Bus and GTK, so its
//! replacement and timeout rules are testable without a session.
//!
//! The service exports the four methods a desktop-application expects
//! (`Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`)
//! and the two signals (`NotificationClosed`, `ActionInvoked`).  Do not
//! disturb belongs to [`crate::idle::Idle`] when the shell has one; the service
//! only re-exposes that flag to the bar.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::idle::Idle;

pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub const INTERFACE: &str = "org.freedesktop.Notifications";
pub const DEFAULT_TIMEOUT_MS: i32 = 10_000;

pub const CLOSED_EXPIRED: u32 = 1;
pub const CLOSED_DISMISSED: u32 = 2;
pub const CLOSED_CALL: u32 = 3;

const INTERFACE_XML: &str = "<node>
  <interface name='org.freedesktop.Notifications'>
    <method name='Notify'>
      <arg type='s' name='app_name' direction='in'/>
      <arg type='u' name='replaces_id' direction='in'/>
      <arg type='s' name='app_icon' direction='in'/>
      <arg type='s' name='summary' direction='in'/>
      <arg type='s' name='body' direction='in'/>
      <arg type='as' name='actions' direction='in'/>
      <arg type='a{sv}' name='hints' direction='in'/>
      <arg type='i' name='expire_timeout' direction='in'/>
      <arg type='u' name='id' direction='out'/>
    </method>
    <method name='CloseNotification'>
      <arg type='u' name='id' direction='in'/>
    </method>
    <method name='GetCapabilities'>
      <arg type='as' name='capabilities' direction='out'/>
    </method>
    <method name='GetServerInformation'>
      <arg type='s' name='name' direction='out'/>
      <arg type='s' name='vendor' direction='out'/>
      <arg type='s' name='version' direction='out'/>
      <arg type='s' name='spec_version' direction='out'/>
    </method>
    <signal name='NotificationClosed'>
      <arg type='u' name='id'/>
      <arg type='u' name='reason'/>
    </signal>
    <signal name='ActionInvoked'>
      <arg type='u' name='id'/>
      <arg type='s' name='action_key'/>
    </signal>
  </interface>
</node>";

thread_local! {
    /// The shell has one notification service.  Bar modules read it here, as
    /// the C modules read the shell singleton; `ui` installs it after startup
    /// and clears it during teardown.
    static CURRENT: RefCell<Option<NotificationService>> = const { RefCell::new(None) };
}

/// One `(id, label)` pair from the `Notify` actions array.  An empty id is the
/// application's default action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub label: String,
}

/// One stored notification.  The timestamp is wall-clock microseconds, as in
/// the C store, so a centre can show the local time later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub category: Option<String>,
    pub urgency: u8,
    pub transient: bool,
    pub timestamp_us: i64,
    pub expire_timeout: i32,
    pub actions: Vec<Action>,
}

impl Notification {
    /// The image the centre should try first: an explicit image path, then the
    /// application's icon, then its desktop entry.  This is the C getter's
    /// precedence.
    pub fn icon(&self) -> &str {
        if let Some(path) = self.image_path.as_deref().filter(|value| !value.is_empty()) {
            return path;
        }

        if !self.app_icon.is_empty() {
            return &self.app_icon;
        }

        self.desktop_entry.as_deref().unwrap_or("")
    }

    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub fn action_label(&self, index: usize) -> &str {
        self.actions
            .get(index)
            .map(|action| action.label.as_str())
            .unwrap_or("")
    }

    pub fn action_id(&self, index: usize) -> &str {
        self.actions
            .get(index)
            .map(|action| action.id.as_str())
            .unwrap_or("")
    }
}

/// The protocol input before it becomes a stored notification.  Keeping this
/// separate makes the store's replacement logic readable: `replaces_id` is a
/// request instruction, never a stored notification's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRequest {
    pub replaces_id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub category: Option<String>,
    pub urgency: u8,
    pub transient: bool,
    pub timestamp_us: i64,
    pub expire_timeout: i32,
    pub actions: Vec<Action>,
}

impl Default for NotificationRequest {
    fn default() -> Self {
        Self {
            replaces_id: 0,
            app_name: String::new(),
            app_icon: String::new(),
            summary: String::new(),
            body: String::new(),
            desktop_entry: None,
            image_path: None,
            category: None,
            urgency: 1,
            transient: false,
            timestamp_us: 0,
            expire_timeout: -1,
            actions: Vec::new(),
        }
    }
}

impl From<NotificationRequest> for Notification {
    fn from(request: NotificationRequest) -> Self {
        let expire_timeout = if request.expire_timeout < 0 {
            if request.urgency >= 2 {
                0
            } else {
                DEFAULT_TIMEOUT_MS
            }
        } else {
            request.expire_timeout
        };

        Self {
            id: 0,
            app_name: request.app_name,
            app_icon: request.app_icon,
            summary: request.summary,
            body: request.body,
            desktop_entry: request.desktop_entry,
            image_path: request.image_path,
            category: request.category,
            urgency: request.urgency,
            transient: request.transient,
            timestamp_us: request.timestamp_us,
            expire_timeout,
            actions: request.actions,
        }
    }
}

/// The oldest-first notification store.  It has no bus and no clock of its own;
/// a request carries its timestamp, and replacing an id preserves that id.
#[derive(Debug, Default)]
pub struct NotificationStore {
    notifications: Vec<Notification>,
    next_id: u32,
}

impl NotificationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    pub fn snapshot(&self) -> &[Notification] {
        &self.notifications
    }

    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.notifications
            .iter()
            .find(|notification| notification.id == id)
    }

    /// Add or replace a notification.  A non-zero `replaces_id` that names a
    /// stored notification updates it in place, keeping the existing id; a
    /// missing id creates a new notification.  This is the C `Notify` rule.
    pub fn notify(&mut self, request: NotificationRequest) -> u32 {
        let incoming = Notification::from(request.clone());

        if request.replaces_id != 0
            && let Some(index) = self
                .notifications
                .iter()
                .position(|notification| notification.id == request.replaces_id)
        {
            let id = self.notifications[index].id;

            self.notifications[index] = Notification { id, ..incoming };

            return id;
        }

        self.next_id = self.next_id.wrapping_add(1).max(1);

        let id = self.next_id;
        let notification = Notification { id, ..incoming };

        self.notifications.push(notification);

        id
    }

    pub fn remove(&mut self, id: u32) -> Option<Notification> {
        let index = self
            .notifications
            .iter()
            .position(|notification| notification.id == id)?;

        Some(self.notifications.remove(index))
    }

    /// Take everything out at once.  The caller emits one closed signal per
    /// returned notification; this keeps the store free of bus knowledge.
    pub fn take_all(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }
}

/// The profile the bar and the centre use.  It owns the D-Bus name but not the
/// notification rendering: the service is fully useful without a toast stack.
#[derive(Clone)]
pub struct NotificationService {
    inner: Rc<RefCell<ServiceState>>,
}

/// A live change listener.  The service keeps the callback; dropping this guard
/// removes it, so a rebuilt bar does not leave a callback behind.  A module
/// keeps the guard in its root's `destroy` handler.
pub struct NotificationSubscription {
    inner: Weak<RefCell<ServiceState>>,
    id: u64,
}

impl Drop for NotificationSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .borrow_mut()
                .listeners
                .retain(|(id, _)| *id != self.id);
        }
    }
}

/// A store event, mirroring the C service's `changed`, `posted` and `closed`
/// signals.  The centre needs the posted event to tell a replacement from a
/// fresh notification even though both keep the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationEvent {
    Changed,
    Posted(u32),
    Closed(u32),
}

/// A live event listener.  Dropping the guard removes the callback.
pub struct NotificationEventSubscription {
    inner: Weak<RefCell<ServiceState>>,
    id: u64,
}

impl Drop for NotificationEventSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .borrow_mut()
                .event_listeners
                .retain(|(id, _)| *id != self.id);
        }
    }
}

type EventListener = dyn Fn(NotificationEvent);

struct ServiceState {
    store: NotificationStore,
    idle: Option<Idle>,
    dnd: bool,
    next_listener_id: u64,
    listeners: Vec<(u64, Rc<dyn Fn()>)>,
    next_event_listener_id: u64,
    event_listeners: Vec<(u64, Rc<EventListener>)>,
    connection: Option<gio::DBusConnection>,
    registration: Option<gio::RegistrationId>,
    owner: Option<gio::OwnerId>,
    owner_acquired: bool,
}

impl NotificationService {
    pub fn new(idle: Option<Idle>) -> Self {
        let inner = Rc::new(RefCell::new(ServiceState {
            store: NotificationStore::new(),
            idle: idle.clone(),
            dnd: false,
            next_listener_id: 0,
            listeners: Vec::new(),
            next_event_listener_id: 0,
            event_listeners: Vec::new(),
            connection: None,
            registration: None,
            owner: None,
            owner_acquired: false,
        }));

        if let Some(idle) = idle {
            let weak: Weak<RefCell<ServiceState>> = Rc::downgrade(&inner);

            idle.on_dnd_changed(move |_| {
                if let Some(inner) = weak.upgrade() {
                    NotificationService { inner }.changed();
                }
            });
        }

        Self { inner }
    }

    /// The service in this shell, if `ui` installed one.
    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    /// Install or remove the process's service handle.  Called by the shell's
    /// startup and teardown, not by a module.
    pub fn install(service: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = service);
    }

    /// Ask for `org.freedesktop.Notifications`.  The store remains usable when
    /// another daemon owns the name; only the protocol signals are then
    /// unavailable, exactly as the C service's `name-lost` case.
    pub fn start(&self) {
        if self.inner.borrow().owner.is_some() {
            return;
        }

        let weak = Rc::downgrade(&self.inner);
        let owner = gio::bus_own_name(
            gio::BusType::Session,
            BUS_NAME,
            gio::BusNameOwnerFlags::NONE,
            move |connection, _name| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };

                export_interface(&inner, connection);
            },
            {
                let weak = Rc::downgrade(&self.inner);

                move |_connection, name| {
                    if let Some(inner) = weak.upgrade() {
                        inner.borrow_mut().owner_acquired = true;
                        log_info(&format!("{name} is ours: this is the notification daemon"));
                    }
                }
            },
            {
                let weak = Rc::downgrade(&self.inner);

                move |_connection, name| {
                    if let Some(inner) = weak.upgrade() {
                        inner.borrow_mut().owner_acquired = false;
                        log_warn(&format!(
                            "{name} is owned by another daemon: whelk keeps its store"
                        ));
                    }
                }
            },
        );

        self.inner.borrow_mut().owner = Some(owner);
    }

    pub fn shutdown(&self) {
        let (owner, connection, registration) = {
            let mut state = self.inner.borrow_mut();

            state.owner_acquired = false;
            (
                state.owner.take(),
                state.connection.take(),
                state.registration.take(),
            )
        };

        if let (Some(connection), Some(registration)) = (connection, registration)
            && let Err(error) = connection.unregister_object(registration)
        {
            log_warn(&format!(
                "could not unregister the notification object: {error}"
            ));
        }

        if let Some(owner) = owner {
            gio::bus_unown_name(owner);
        }
    }

    pub fn is_owner(&self) -> bool {
        self.inner.borrow().owner_acquired
    }

    pub fn count(&self) -> usize {
        self.inner.borrow().store.len()
    }

    pub fn snapshot(&self) -> Vec<Notification> {
        self.inner.borrow().store.snapshot().to_vec()
    }

    pub fn has_dnd(&self) -> bool {
        let state = self.inner.borrow();

        state.idle.as_ref().map(Idle::has_dnd).unwrap_or(state.dnd)
    }

    pub fn set_dnd(&self, dnd: bool) {
        let idle = self.inner.borrow().idle.clone();

        if let Some(idle) = idle {
            idle.set_dnd(dnd);

            return;
        }

        let changed = {
            let mut state = self.inner.borrow_mut();

            if state.dnd == dnd {
                false
            } else {
                state.dnd = dnd;

                true
            }
        };

        if changed {
            self.changed();
        }
    }

    /// Store a notification and notify the bar.  DND suppresses interrupts,
    /// not storage: the count and the centre still receive it.
    pub fn post(&self, request: NotificationRequest) -> u32 {
        let id = self.inner.borrow_mut().store.notify(request);

        self.changed();

        // DND suppresses the interrupt, not the stored history; the C service
        // emits `posted` only when the shell is free to show a toast.
        if !self.has_dnd() {
            self.publish_event(NotificationEvent::Posted(id));
        }

        id
    }

    pub fn close(&self, id: u32) -> bool {
        self.close_with_reason(id, CLOSED_DISMISSED)
    }

    pub fn expire(&self, id: u32) -> bool {
        self.close_with_reason(id, CLOSED_EXPIRED)
    }

    pub fn close_all(&self) {
        let notifications = {
            let mut state = self.inner.borrow_mut();

            state.store.take_all()
        };

        if notifications.is_empty() {
            return;
        }

        for notification in &notifications {
            self.emit_closed(notification.id, CLOSED_DISMISSED);
            self.publish_event(NotificationEvent::Closed(notification.id));
        }

        self.changed();
    }

    pub fn invoke(&self, id: u32, action: Option<&str>) -> bool {
        if self.inner.borrow().store.get(id).is_none() {
            return false;
        }

        self.emit_action(id, action.unwrap_or("default"));

        self.close_with_reason(id, CLOSED_DISMISSED)
    }

    pub fn on_changed<F: Fn() + 'static>(&self, callback: F) -> NotificationSubscription {
        let mut state = self.inner.borrow_mut();

        state.next_listener_id = state.next_listener_id.wrapping_add(1).max(1);

        let id = state.next_listener_id;

        state.listeners.push((id, Rc::new(callback)));

        NotificationSubscription {
            inner: Rc::downgrade(&self.inner),
            id,
        }
    }

    pub fn on_event<F: Fn(NotificationEvent) + 'static>(
        &self,
        callback: F,
    ) -> NotificationEventSubscription {
        let mut state = self.inner.borrow_mut();

        state.next_event_listener_id = state.next_event_listener_id.wrapping_add(1).max(1);

        let id = state.next_event_listener_id;

        state.event_listeners.push((id, Rc::new(callback)));

        NotificationEventSubscription {
            inner: Rc::downgrade(&self.inner),
            id,
        }
    }

    fn close_with_reason(&self, id: u32, reason: u32) -> bool {
        let removed = self.inner.borrow_mut().store.remove(id);

        if removed.is_none() {
            return false;
        }

        self.emit_closed(id, reason);
        self.publish_event(NotificationEvent::Closed(id));
        self.changed();

        true
    }

    fn changed(&self) {
        let listeners = self.inner.borrow().listeners.clone();
        let event_listeners = self.inner.borrow().event_listeners.clone();

        for (_, listener) in listeners {
            listener();
        }

        for (_, listener) in event_listeners {
            listener(NotificationEvent::Changed);
        }
    }

    fn publish_event(&self, event: NotificationEvent) {
        let listeners = self.inner.borrow().event_listeners.clone();

        for (_, listener) in listeners {
            listener(event);
        }
    }

    fn emit_closed(&self, id: u32, reason: u32) {
        if let Some(connection) = self.inner.borrow().connection.clone() {
            let parameters = (id, reason).to_variant();

            if let Err(error) = connection.emit_signal(
                None,
                OBJECT_PATH,
                INTERFACE,
                "NotificationClosed",
                Some(&parameters),
            ) {
                log_warn(&format!("could not emit NotificationClosed: {error}"));
            }
        }
    }

    fn emit_action(&self, id: u32, action: &str) {
        if let Some(connection) = self.inner.borrow().connection.clone() {
            let parameters = (id, action).to_variant();

            if let Err(error) = connection.emit_signal(
                None,
                OBJECT_PATH,
                INTERFACE,
                "ActionInvoked",
                Some(&parameters),
            ) {
                log_warn(&format!("could not emit ActionInvoked: {error}"));
            }
        }
    }
}

impl Drop for ServiceState {
    fn drop(&mut self) {
        if let (Some(connection), Some(registration)) =
            (self.connection.take(), self.registration.take())
        {
            let _ = connection.unregister_object(registration);
        }

        if let Some(owner) = self.owner.take() {
            gio::bus_unown_name(owner);
        }
    }
}

fn export_interface(inner: &Rc<RefCell<ServiceState>>, connection: gio::DBusConnection) {
    let info = match gio::DBusNodeInfo::for_xml(INTERFACE_XML) {
        Ok(info) => info,
        Err(error) => {
            log_warn(&format!(
                "the notification interface did not parse: {error}"
            ));

            return;
        }
    };
    let Some(interface) = info.lookup_interface(INTERFACE) else {
        return;
    };
    let weak = Rc::downgrade(inner);
    let registration = connection
        .register_object(OBJECT_PATH, &interface)
        .method_call(
            move |_connection,
                  _sender,
                  _object_path,
                  _interface_name,
                  method_name,
                  parameters,
                  invocation| {
                let Some(inner) = weak.upgrade() else {
                    invocation
                        .return_error(gio::DBusError::Failed, "the notification service is gone");

                    return;
                };
                let service = NotificationService { inner };

                handle_method(&service, method_name, &parameters, invocation);
            },
        )
        .build();

    match registration {
        Ok(registration) => {
            let mut state = inner.borrow_mut();

            state.connection = Some(connection);
            state.registration = Some(registration);
        }
        Err(error) => {
            log_warn(&format!(
                "could not export the notification interface: {error}"
            ));
        }
    }
}

fn handle_method(
    service: &NotificationService,
    method: &str,
    parameters: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    match method {
        "GetCapabilities" => {
            let capabilities = vec![
                "body",
                "body-markup",
                "actions",
                "icon-static",
                "persistence",
            ];
            let value = (capabilities,).to_variant();

            invocation.return_value(Some(&value));
        }
        "GetServerInformation" => {
            let value = ("whelk", "whelk", env!("CARGO_PKG_VERSION"), "1.2").to_variant();

            invocation.return_value(Some(&value));
        }
        "CloseNotification" => {
            let Some((id,)) = parameters.get::<(u32,)>() else {
                invocation.return_error(gio::DBusError::InvalidArgs, "CloseNotification needs a u");

                return;
            };

            service.close_with_reason(id, CLOSED_CALL);
            invocation.return_value(None);
        }
        "Notify" => match request_from(parameters) {
            Some(request) => {
                let id = service.post(request);
                let value = (id,).to_variant();

                invocation.return_value(Some(&value));
            }
            None => {
                invocation.return_error(
                    gio::DBusError::InvalidArgs,
                    "Notify parameters do not match the notification specification",
                );
            }
        },
        _ => {
            invocation.return_error(
                gio::DBusError::UnknownMethod,
                &format!("whelk does not implement {method}"),
            );
        }
    }
}

fn request_from(parameters: &glib::Variant) -> Option<NotificationRequest> {
    let (app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) =
        parameters.get::<(
            String,
            u32,
            String,
            String,
            String,
            Vec<String>,
            HashMap<String, glib::Variant>,
            i32,
        )>()?;

    let mut request = NotificationRequest {
        replaces_id,
        app_name,
        app_icon,
        summary,
        body,
        timestamp_us: glib::real_time(),
        expire_timeout,
        ..NotificationRequest::default()
    };

    for pair in actions.as_chunks::<2>().0 {
        request.actions.push(Action {
            id: pair[0].clone(),
            label: pair[1].clone(),
        });
    }

    for (key, value) in hints {
        match key.as_str() {
            "desktop-entry" | "desktop_entry" => {
                request.desktop_entry = value.get::<String>();
            }
            "image-path" | "image_path" => {
                request.image_path = value.get::<String>();
            }
            "category" => {
                request.category = value.get::<String>();
            }
            "urgency" => {
                if let Some(urgency) = value.get::<u8>() {
                    request.urgency = urgency;
                } else if let Some(urgency) = value.get::<u32>() {
                    request.urgency = urgency.min(u32::from(u8::MAX)) as u8;
                }
            }
            "transient" => {
                if let Some(transient) = value.get::<bool>() {
                    request.transient = transient;
                }
            }
            _ => {}
        }
    }

    Some(request)
}

fn log_info(message: &str) {
    eprintln!("whelk-notifications: {message}");
}

fn log_warn(message: &str) {
    eprintln!("whelk-notifications: {message}");
}

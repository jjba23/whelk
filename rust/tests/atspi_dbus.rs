// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The accessibility producer against a fake application on a private bus.
//! The walk makes synchronous D-Bus calls, so the test runs it on a worker
//! while the test thread answers the fake application's calls.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::glib;
use gtk4::glib::prelude::*;
use whelk_rs::atspi;
use whelk_rs::chord::ChordStyle;
use whelk_rs::menubar::{MenuAction, MenuEntry};

const XML: &str = "<node>
  <interface name='org.a11y.atspi.Accessible'>
    <method name='GetRole'><arg type='u' direction='out'/></method>
    <method name='GetChildren'><arg type='a(so)' direction='out'/></method>
    <method name='GetState'><arg type='au' direction='out'/></method>
    <property name='Name' type='s' access='read'/>
  </interface>
  <interface name='org.a11y.atspi.Action'>
    <method name='GetKeyBinding'>
      <arg type='i' direction='in'/><arg type='s' direction='out'/>
    </method>
    <method name='DoAction'>
      <arg type='i' direction='in'/><arg type='b' direction='out'/>
    </method>
    <property name='NActions' type='i' access='read'/>
  </interface>
  <interface name='org.a11y.atspi.Registry'>
    <method name='RegisterEvent'>
      <arg type='s' direction='in'/><arg type='as' direction='in'/>
      <arg type='s' direction='in'/>
    </method>
    <method name='DeregisterEvent'>
      <arg type='s' direction='in'/><arg type='s' direction='in'/>
    </method>
  </interface>
  <interface name='org.a11y.Bus'>
    <method name='GetAddress'><arg type='s' direction='out'/></method>
  </interface>
</node>";

const PATHS: &[&str] = &[
    "/org/a11y/atspi/accessible/root",
    "/app",
    "/menubar",
    "/file",
    "/edit",
    "/recent",
    "/recent_menu",
    "/quit",
    "/separator",
    "/prefs",
    "/document",
    "/copy",
    "/paste",
    "/export",
];

struct FakeBus {
    bus: Option<gio::TestDBus>,
    connection: Option<gio::DBusConnection>,
    unique: String,
    extra: Arc<AtomicBool>,
    registrations: Vec<gio::RegistrationId>,
}

impl FakeBus {
    fn start() -> Self {
        let bus = gio::TestDBus::new(gio::TestDBusFlags::NONE);

        bus.up();

        let address = bus.bus_address().expect("private bus address").to_string();
        let connection = gio::DBusConnection::for_address_sync(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            None::<&gio::Cancellable>,
        )
        .expect("connect to the private bus");
        let unique = connection.unique_name().expect("unique name").to_string();

        request_name(&connection, "org.a11y.atspi.Registry");
        request_name(&connection, "org.a11y.Bus");

        let node = gio::DBusNodeInfo::for_xml(XML).expect("introspection xml");
        let accessible = node
            .lookup_interface("org.a11y.atspi.Accessible")
            .expect("accessible interface");
        let action = node
            .lookup_interface("org.a11y.atspi.Action")
            .expect("action interface");
        let registry = node
            .lookup_interface("org.a11y.atspi.Registry")
            .expect("registry interface");
        let a11y_bus = node
            .lookup_interface("org.a11y.Bus")
            .expect("bus interface");
        let extra = Arc::new(AtomicBool::new(false));
        let mut registrations = Vec::new();

        for path in PATHS {
            let service = unique.clone();
            let children_extra = Arc::clone(&extra);
            let id = connection
                .register_object(path, &accessible)
                .method_call(
                    move |bus, sender, path, interface, method, params, invocation| {
                        accessible_call(
                            bus,
                            sender,
                            path,
                            interface,
                            method,
                            params,
                            invocation,
                            &service,
                            children_extra.load(Ordering::Relaxed),
                        )
                    },
                )
                .property(accessible_property)
                .build()
                .expect("register accessible object");

            registrations.push(id);

            let id = connection
                .register_object(path, &action)
                .method_call(action_call)
                .property(action_property)
                .build()
                .expect("register action object");

            registrations.push(id);
        }

        let mut server = Self {
            bus: Some(bus),
            connection: Some(connection),
            unique,
            extra,
            registrations,
        };

        let address = address.clone();
        let connection = server.connection.as_ref().expect("connection");

        let id = connection
            .register_object("/org/a11y/atspi/registry", &registry)
            .method_call(
                move |_bus, _sender, _path, _interface, method, _params, invocation| {
                    if method == "RegisterEvent" || method == "DeregisterEvent" {
                        invocation.return_value(None);
                    } else {
                        unknown_method(invocation, method);
                    }
                },
            )
            .build()
            .expect("register the fake registry");

        server.registrations.push(id);

        let id = connection
            .register_object("/org/a11y/bus", &a11y_bus)
            .method_call(
                move |_bus, _sender, _path, _interface, method, _params, invocation| {
                    if method == "GetAddress" {
                        invocation.return_value(Some(&(address.clone(),).to_variant()));
                    } else {
                        unknown_method(invocation, method);
                    }
                },
            )
            .build()
            .expect("register the fake accessibility bus");

        server.registrations.push(id);

        server
    }

    fn emit_children_changed(&self) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let affected = (
            self.unique.clone(),
            glib::variant::ObjectPath::try_from("/export").unwrap(),
        )
            .to_variant();
        let payload = (
            "insert".to_owned(),
            4i32,
            0i32,
            affected,
            HashMap::<String, glib::Variant>::new(),
        )
            .to_variant();

        connection
            .emit_signal(
                None,
                "/menubar",
                "org.a11y.atspi.Event.Object",
                "ChildrenChanged",
                Some(&payload),
            )
            .expect("emit ChildrenChanged");
    }
}

impl Drop for FakeBus {
    fn drop(&mut self) {
        self.connection.take();

        if let Some(bus) = self.bus.take() {
            bus.down();
        }
    }
}

fn request_name(connection: &gio::DBusConnection, name: &str) {
    let reply = connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(name, 0u32).to_variant()),
            Some(glib::VariantTy::new("(u)").expect("reply type")),
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
        )
        .expect("request a bus name");
    let result = reply
        .child_value(0)
        .get::<u32>()
        .expect("RequestName reply");

    assert!(result == 1 || result == 2, "could not own {name}");
}

fn unknown_method(invocation: gio::DBusMethodInvocation, method: &str) {
    invocation.return_dbus_error(
        "org.freedesktop.DBus.Error.UnknownMethod",
        &format!("no such method: {method}"),
    );
}

fn fake_role(path: &str) -> u32 {
    match path {
        "/menubar" => 34,
        "/file" | "/edit" | "/recent_menu" => 33,
        "/separator" => 50,
        "/prefs" => 8,
        "/copy" => 45,
        _ => 35,
    }
}

fn fake_name(path: &str) -> &'static str {
    match path {
        "/org/a11y/atspi/accessible/root" | "/app" => "Fake App",
        "/menubar" => "Menu Bar",
        "/file" => "File",
        "/edit" => "Edit",
        "/quit" => "Quit",
        "/prefs" => "Preferences",
        "/recent" | "/recent_menu" => "Recent",
        "/document" => "Document",
        "/copy" => "Copy",
        "/paste" => "Paste",
        "/export" => "Export",
        _ => "",
    }
}

fn fake_binding(path: &str) -> &'static str {
    match path {
        "/quit" => "<Primary>q",
        "/copy" => "Ctrl+C",
        "/export" => "Ctrl+E",
        _ => "",
    }
}

fn fake_enabled(path: &str) -> bool {
    path != "/paste"
}

fn fake_children(path: &str, extra: bool) -> &'static [&'static str] {
    match path {
        "/org/a11y/atspi/accessible/root" => &["/app"],
        "/app" => &["/menubar"],
        "/menubar" => &["/file", "/edit"],
        "/file" if extra => &["/quit", "/separator", "/recent", "/prefs", "/export"],
        "/file" => &["/quit", "/separator", "/recent", "/prefs"],
        "/edit" => &["/copy", "/paste"],
        "/recent" => &["/recent_menu"],
        "/recent_menu" => &["/document"],
        _ => &[],
    }
}

#[allow(clippy::too_many_arguments)]
fn accessible_call(
    _bus: gio::DBusConnection,
    _sender: Option<&str>,
    path: &str,
    _interface: Option<&str>,
    method: &str,
    _params: glib::Variant,
    invocation: gio::DBusMethodInvocation,
    service: &str,
    extra: bool,
) {
    match method {
        "GetRole" => invocation.return_value(Some(&(fake_role(path),).to_variant())),
        "GetState" => {
            let word = if fake_enabled(path) {
                (1u32 << 8) | (1u32 << 24)
            } else {
                0
            };

            invocation.return_value(Some(&(vec![word],).to_variant()));
        }
        "GetChildren" => {
            // The real accessibility bus sometimes sends the object path as
            // `o`, not `s`; send it as an object path so the reader exercises
            // the conversion it must support.
            let children: Vec<(String, glib::variant::ObjectPath)> = fake_children(path, extra)
                .iter()
                .map(|child| {
                    (
                        service.to_owned(),
                        glib::variant::ObjectPath::try_from(*child)
                            .expect("fake child object path"),
                    )
                })
                .collect();

            invocation.return_value(Some(&(children,).to_variant()));
        }
        _ => unknown_method(invocation, method),
    }
}

fn accessible_property(
    _bus: gio::DBusConnection,
    _sender: Option<&str>,
    path: &str,
    interface: &str,
    property: &str,
) -> glib::Variant {
    if interface == "org.a11y.atspi.Accessible" && property == "Name" {
        fake_name(path).to_variant()
    } else {
        ().to_variant()
    }
}

fn action_call(
    _bus: gio::DBusConnection,
    _sender: Option<&str>,
    path: &str,
    _interface: Option<&str>,
    method: &str,
    _params: glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    match method {
        "GetKeyBinding" => {
            invocation.return_value(Some(&(fake_binding(path).to_owned(),).to_variant()));
        }
        "DoAction" => invocation.return_value(Some(&(true,).to_variant())),
        _ => unknown_method(invocation, method),
    }
}

fn action_property(
    _bus: gio::DBusConnection,
    _sender: Option<&str>,
    _path: &str,
    interface: &str,
    property: &str,
) -> glib::Variant {
    if interface == "org.a11y.atspi.Action" && property == "NActions" {
        1i32.to_variant()
    } else {
        ().to_variant()
    }
}

fn read(app_name: &str, pid: i32) -> Vec<whelk_rs::menubar::Menu> {
    let app_name = app_name.to_owned();
    let reader =
        std::thread::spawn(move || atspi::read_menus_once(&app_name, pid, ChordStyle::Cua));

    while !reader.is_finished() {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(1));
    }

    reader.join().expect("accessibility reader thread")
}

fn wait_for_changes(
    changes: &Rc<RefCell<Vec<Vec<whelk_rs::menubar::Menu>>>>,
    target: usize,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);

    while changes.borrow().len() < target && Instant::now() < deadline {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(2));
    }

    changes.borrow().len() >= target
}

#[test]
fn reads_a_fake_application_menu_bar_over_a_private_bus() {
    let fake = FakeBus::start();
    let menus = read("Fake App", 0);

    assert_eq!(menus.len(), 2);
    assert_eq!(menus[0].label, "File");
    assert_eq!(menus[0].entries.len(), 4);

    assert!(matches!(
        &menus[0].entries[0],
        MenuEntry::Item { label, enabled: true, accel: Some(accel), action: Some(MenuAction::Atspi { service, path, .. }), .. }
            if label == "Quit"
                && accel == "Ctrl+Q"
                && path == "/quit"
                && service == &fake.unique
    ));
    assert!(matches!(&menus[0].entries[1], MenuEntry::Separator));
    assert!(matches!(
        &menus[0].entries[2],
        MenuEntry::Item { label, submenu, .. }
            if label == "Recent"
                && matches!(&submenu[0], MenuEntry::Item { label, .. } if label == "Document")
    ));
    assert!(matches!(
        &menus[1].entries[0],
        MenuEntry::Item { label, enabled: true, accel: Some(accel), .. }
            if label == "Copy" && accel == "Ctrl+C"
    ));
    assert!(matches!(
        &menus[1].entries[1],
        MenuEntry::Item { label, enabled: false, .. } if label == "Paste"
    ));

    // A unique accessibility name can be matched by process id even when the
    // application calls itself something else.
    assert_eq!(read("Wrong Name", std::process::id() as i32).len(), 2);

    assert!(read("Nothing Here", 0).is_empty());

    // The production watch must deliver the initial tree and follow an
    // accessibility event.  The fake application adds Export to File after
    // the first delivery, exactly as a rebuild does.
    let changes = Rc::new(RefCell::new(Vec::new()));
    let delivered = Rc::clone(&changes);
    let watch = atspi::Watch::start("Fake App".to_owned(), 0, ChordStyle::Cua, move |menus| {
        delivered.borrow_mut().push(menus)
    })
    .expect("start the accessibility watch");

    assert!(wait_for_changes(&changes, 1));
    assert_eq!(changes.borrow()[0][0].entries.len(), 4);

    fake.extra.store(true, Ordering::Relaxed);
    fake.emit_children_changed();

    assert!(wait_for_changes(&changes, 2));
    assert_eq!(changes.borrow()[1][0].entries.len(), 5);
    assert!(matches!(
        &changes.borrow()[1][0].entries[4],
        MenuEntry::Item { label, enabled: true, accel: Some(accel), .. }
            if label == "Export" && accel == "Ctrl+E"
    ));

    drop(watch);
    std::thread::sleep(Duration::from_millis(200));
}

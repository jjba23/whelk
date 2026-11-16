// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The bus half of the notification service.  This test needs a disposable
//! session bus (`dbus-run-session`), so it only runs when the operator sets
//! `WHELK_NOTIFICATION_DBUS_TEST=1`; the pure store rules live in
//! `tests/notifications.rs` and run everywhere.

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use whelk_rs::notifications::{BUS_NAME, INTERFACE, NotificationService, OBJECT_PATH};

#[test]
fn notify_and_close_cross_the_session_bus() {
    if std::env::var_os("WHELK_NOTIFICATION_DBUS_TEST").is_none() {
        return;
    }

    let service = NotificationService::new(None);

    service.start();

    let deadline = Instant::now() + Duration::from_secs(2);

    while !service.is_owner() && Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        service.is_owner(),
        "the test session bus did not grant the name"
    );

    let client = thread::spawn(|| {
        let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .expect("client session bus connection");
        let capabilities = connection
            .call_sync(
                Some(BUS_NAME),
                OBJECT_PATH,
                INTERFACE,
                "GetCapabilities",
                None,
                None,
                gio::DBusCallFlags::NONE,
                5_000,
                gio::Cancellable::NONE,
            )
            .expect("GetCapabilities reply")
            .get::<(Vec<String>,)>()
            .expect("capability type");
        let information = connection
            .call_sync(
                Some(BUS_NAME),
                OBJECT_PATH,
                INTERFACE,
                "GetServerInformation",
                None,
                None,
                gio::DBusCallFlags::NONE,
                5_000,
                gio::Cancellable::NONE,
            )
            .expect("GetServerInformation reply")
            .get::<(String, String, String, String)>()
            .expect("server information type");
        let hints: HashMap<String, glib::Variant> = HashMap::new();
        let parameters = (
            "whelk-test",
            0u32,
            "dialog-information",
            "A test notification",
            "This notification crossed the session bus.",
            Vec::<String>::new(),
            hints,
            -1i32,
        )
            .to_variant();
        let id = connection
            .call_sync(
                Some(BUS_NAME),
                OBJECT_PATH,
                INTERFACE,
                "Notify",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                5_000,
                gio::Cancellable::NONE,
            )
            .expect("Notify reply")
            .get::<(u32,)>()
            .expect("notification id")
            .0;

        connection
            .call_sync(
                Some(BUS_NAME),
                OBJECT_PATH,
                INTERFACE,
                "CloseNotification",
                Some(&(id,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5_000,
                gio::Cancellable::NONE,
            )
            .expect("CloseNotification reply");

        (capabilities.0, information, id)
    });

    while !client.is_finished() {
        glib::MainContext::default().iteration(false);
        thread::sleep(Duration::from_millis(10));
    }

    let (capabilities, information, id) = client.join().expect("D-Bus client thread");
    let expected = vec![
        "body",
        "body-markup",
        "actions",
        "icon-static",
        "persistence",
    ];

    assert_eq!(id, 1);
    assert_eq!(capabilities, expected);
    assert_eq!(information.0, "whelk");
    assert_eq!(information.1, "whelk");
    assert_eq!(information.3, "1.2");
    assert_eq!(service.count(), 0);

    service.shutdown();
}

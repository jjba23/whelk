// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::cell::Cell;
use std::rc::Rc;

use whelk_rs::notifications::{
    Action, DEFAULT_TIMEOUT_MS, Notification, NotificationRequest, NotificationService,
    NotificationStore,
};

fn request(summary: &str) -> NotificationRequest {
    NotificationRequest {
        app_name: "mail".to_owned(),
        app_icon: "mail-client".to_owned(),
        summary: summary.to_owned(),
        body: "one new message".to_owned(),
        timestamp_us: 1_700_000_000_000_000,
        ..NotificationRequest::default()
    }
}

#[test]
fn notify_assigns_stable_ids_and_keeps_oldest_first() {
    let mut store = NotificationStore::new();
    let first = store.notify(request("first"));
    let second = store.notify(request("second"));

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(store.len(), 2);
    assert_eq!(store.snapshot()[0].id, first);
    assert_eq!(store.snapshot()[1].id, second);
    assert_eq!(store.snapshot()[0].app_name, "mail");
    assert_eq!(store.snapshot()[0].body, "one new message");
    assert_eq!(store.snapshot()[0].urgency, 1);
    assert_eq!(store.snapshot()[0].expire_timeout, DEFAULT_TIMEOUT_MS);
}

#[test]
fn timeout_resolution_follows_urgency() {
    let mut store = NotificationStore::new();

    let normal = store.notify(request("normal"));
    let critical = store.notify(NotificationRequest {
        urgency: 2,
        ..request("critical")
    });
    let explicit = store.notify(NotificationRequest {
        expire_timeout: 500,
        ..request("explicit")
    });
    let zero = store.notify(NotificationRequest {
        expire_timeout: 0,
        ..request("sticky")
    });

    assert_eq!(
        store.get(normal).unwrap().expire_timeout,
        DEFAULT_TIMEOUT_MS
    );
    assert_eq!(store.get(critical).unwrap().expire_timeout, 0);
    assert_eq!(store.get(explicit).unwrap().expire_timeout, 500);
    assert_eq!(store.get(zero).unwrap().expire_timeout, 0);
}

#[test]
fn replaces_id_updates_in_place_and_keeps_identity() {
    let mut store = NotificationStore::new();
    let original = store.notify(request("first"));
    let replaced = store.notify(NotificationRequest {
        replaces_id: original,
        summary: "second".to_owned(),
        body: "updated body".to_owned(),
        actions: vec![Action {
            id: "open".to_owned(),
            label: "Open".to_owned(),
        }],
        ..request("ignored")
    });

    assert_eq!(replaced, original);
    assert_eq!(store.len(), 1);
    assert_eq!(store.get(original).unwrap().summary, "second");
    assert_eq!(store.get(original).unwrap().body, "updated body");
    assert_eq!(store.get(original).unwrap().action_id(0), "open");
    assert_eq!(store.get(original).unwrap().action_label(0), "Open");
    assert_eq!(store.notify(request("third")), 2);
}

#[test]
fn unknown_replaces_id_creates_a_new_notification() {
    let mut store = NotificationStore::new();
    let first = store.notify(request("first"));
    let replacement = store.notify(NotificationRequest {
        replaces_id: 99,
        ..request("replacement")
    });

    assert_eq!(first, 1);
    assert_eq!(replacement, 2);
    assert_eq!(store.len(), 2);
}

#[test]
fn remove_and_take_all_drop_stored_notifications() {
    let mut store = NotificationStore::new();
    let first = store.notify(request("first"));
    let second = store.notify(request("second"));
    let third = store.notify(request("third"));

    let removed = store.remove(second).expect("second notification");

    assert_eq!(removed.id, second);
    assert_eq!(store.len(), 2);
    assert_eq!(store.snapshot()[0].id, first);
    assert_eq!(store.snapshot()[1].id, third);
    assert!(store.remove(second).is_none());

    let remaining = store.take_all();

    assert_eq!(remaining.len(), 2);
    assert!(store.is_empty());
}

#[test]
fn icon_prefers_image_path_then_app_icon_then_desktop_entry() {
    let notification = Notification {
        id: 1,
        app_name: "mail".to_owned(),
        app_icon: "mail-client".to_owned(),
        summary: String::new(),
        body: String::new(),
        desktop_entry: Some("org.example.Mail".to_owned()),
        image_path: Some("/tmp/avatar.png".to_owned()),
        category: None,
        urgency: 1,
        transient: false,
        timestamp_us: 0,
        expire_timeout: 0,
        actions: Vec::new(),
    };

    assert_eq!(notification.icon(), "/tmp/avatar.png");

    let without_image = Notification {
        image_path: None,
        ..notification.clone()
    };

    assert_eq!(without_image.icon(), "mail-client");

    let without_icon = Notification {
        app_icon: String::new(),
        ..without_image
    };

    assert_eq!(without_icon.icon(), "org.example.Mail");
}

#[test]
fn service_tracks_dnd_and_notifies_listeners_on_changes() {
    let service = NotificationService::new(None);
    let changes = Rc::new(Cell::new(0));
    let callback_changes = changes.clone();

    let _subscription =
        service.on_changed(move || callback_changes.set(callback_changes.get() + 1));

    let id = service.post(request("hello"));

    assert_eq!(service.count(), 1);
    assert_eq!(service.snapshot()[0].id, id);
    assert!(!service.has_dnd());
    assert_eq!(changes.get(), 1);

    service.set_dnd(true);

    assert!(service.has_dnd());
    assert_eq!(changes.get(), 2);

    // Setting the same value is not a visible change.
    service.set_dnd(true);

    assert_eq!(changes.get(), 2);

    service.set_dnd(false);

    assert!(!service.has_dnd());
    assert_eq!(changes.get(), 3);

    assert!(service.close(id));
    assert_eq!(service.count(), 0);
    assert_eq!(changes.get(), 4);
    assert!(!service.close(id));
}

#[test]
fn dropping_a_subscription_stops_change_callbacks() {
    let service = NotificationService::new(None);
    let changes = Rc::new(Cell::new(0));
    let callback_changes = changes.clone();
    let subscription = service.on_changed(move || callback_changes.set(callback_changes.get() + 1));

    service.post(request("counted"));

    assert_eq!(changes.get(), 1);

    drop(subscription);
    service.post(request("not counted"));

    assert_eq!(changes.get(), 1);
}

#[test]
fn invoke_closes_an_existing_notification_and_ignores_missing_ids() {
    let service = NotificationService::new(None);
    let id = service.post(request("hello"));

    assert!(service.invoke(id, Some("open")));
    assert_eq!(service.count(), 0);
    assert!(!service.invoke(id, None));
}

#[test]
fn close_all_clears_the_store_without_emitting_for_an_empty_one() {
    let service = NotificationService::new(None);
    let changes = Rc::new(Cell::new(0));
    let callback_changes = changes.clone();

    let _subscription =
        service.on_changed(move || callback_changes.set(callback_changes.get() + 1));
    service.post(request("first"));
    service.post(request("second"));
    service.close_all();

    assert_eq!(service.count(), 0);
    assert_eq!(changes.get(), 3);

    service.close_all();

    assert_eq!(changes.get(), 3);
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::bar::{BarSettings, Edge as BarEdge};
use whelk_rs::notificationcenter::{
    TOAST_LIMIT, ToastQueue, body_markup, group_app, group_notifications, panel_summary,
    time_label, toast_top_margin,
};
use whelk_rs::notifications::Notification;

fn notification(id: u32, app: &str, summary: &str, icon: &str) -> Notification {
    Notification {
        id,
        app_name: app.to_owned(),
        app_icon: icon.to_owned(),
        summary: summary.to_owned(),
        body: String::new(),
        desktop_entry: None,
        image_path: None,
        category: None,
        urgency: 1,
        transient: false,
        timestamp_us: 1_700_000_000_000_000,
        expire_timeout: 0,
        actions: Vec::new(),
    }
}

#[test]
fn empty_application_names_share_the_readable_group() {
    let empty = notification(1, "", "one", "");
    let named = notification(2, "chat", "two", "chat-client");

    assert_eq!(group_app(&empty), "Notifications");
    assert_eq!(group_app(&named), "chat");
}

#[test]
fn groups_follow_first_appearance_and_put_the_newest_card_on_top() {
    let snapshot = vec![
        notification(1, "chat", "oldest", "chat-icon"),
        notification(2, "build", "build", "build-icon"),
        notification(3, "chat", "middle", "newer-chat-icon"),
        notification(4, "chat", "newest", "latest-chat-icon"),
    ];
    let groups = group_notifications(&snapshot);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].app, "chat");
    assert_eq!(groups[0].icon, "chat-icon");
    assert_eq!(
        groups[0]
            .notifications
            .iter()
            .map(|notification| notification.summary.as_str())
            .collect::<Vec<_>>(),
        ["newest", "middle", "oldest"]
    );
    assert_eq!(groups[1].app, "build");
    assert_eq!(groups[1].notifications.len(), 1);
}

#[test]
fn an_empty_store_has_an_empty_summary() {
    assert_eq!(panel_summary(0), "Nothing new");
    assert_eq!(panel_summary(1), "1 notification");
    assert_eq!(panel_summary(3), "3 notifications");
    assert_eq!(group_notifications(&[]).len(), 0);
}

#[test]
fn the_toast_queue_keeps_newest_first_and_drops_the_oldest_at_the_limit() {
    let mut queue = ToastQueue::default();

    for id in 1..=TOAST_LIMIT as u32 {
        assert_eq!(queue.posted(id), None);
    }

    assert_eq!(queue.ids(), &[5, 4, 3, 2, 1]);
    assert_eq!(queue.posted(6), Some(1));
    assert_eq!(queue.ids(), &[6, 5, 4, 3, 2]);
}

#[test]
fn posting_an_existing_id_moves_it_to_the_front_instead_of_duplicating_it() {
    let mut queue = ToastQueue::default();

    queue.posted(1);
    queue.posted(2);
    queue.posted(3);

    assert_eq!(queue.posted(2), None);
    assert_eq!(queue.ids(), &[2, 3, 1]);

    assert!(queue.remove(3));
    assert!(!queue.remove(3));
    assert_eq!(queue.ids(), &[2, 1]);

    queue.clear();

    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
}

#[test]
fn toast_margins_clear_a_top_bar_only() {
    let top = BarSettings {
        edge: BarEdge::Top,
        height: 38,
        ..BarSettings::default()
    };
    let bottom = BarSettings {
        edge: BarEdge::Bottom,
        height: 38,
        ..BarSettings::default()
    };

    assert_eq!(toast_top_margin(&top), 46);
    assert_eq!(toast_top_margin(&bottom), 8);
}

#[test]
fn only_valid_pango_markup_is_treated_as_markup() {
    assert_eq!(body_markup("<b>bold</b>"), Some("<b>bold</b>"));
    assert_eq!(body_markup("<b>unterminated"), None);
    assert_eq!(body_markup("a & b"), None);
    assert_eq!(body_markup("plain text"), Some("plain text"));
    assert_eq!(body_markup(""), None);
}

#[test]
fn the_time_label_is_a_local_hour_and_minute() {
    let label = time_label(1_700_000_000_000_000);

    assert_eq!(label.len(), 5);
    assert_eq!(label.as_bytes()[2], b':');
}

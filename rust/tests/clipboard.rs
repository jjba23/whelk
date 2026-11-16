// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::clipboard::{
    Clipboard, Entry, History, Status, encode_record, entry_detail, entry_label, format_age,
    format_size, mime_is_plain_text, parse_records, pick_mime, types_secret,
};
use whelk_rs::settings::ClipboardSettings;

#[test]
fn mime_policy_prefers_rich_content_and_refuses_secrets() {
    let types = "image/jpeg\ntext/html\ntext/plain;charset=utf-8\n";

    assert_eq!(pick_mime(types, false), Some("image/jpeg"));
    assert_eq!(pick_mime(types, true), Some("text/plain;charset=utf-8"));
    assert!(mime_is_plain_text("UTF8_STRING"));
    assert!(!mime_is_plain_text("text/html"));
    assert!(types_secret("x-kde-passwordManagerHint\nsecret"));
    assert!(!types_secret("text/plain\nimage/png"));
}

#[test]
fn records_carry_binary_payloads_and_multiple_copies() {
    let mut pending = encode_record("image/png", b"\x89PNG\0\xff", b"");
    pending.extend_from_slice(&encode_record("text/plain", b"hello", b"hello"));
    let mut history = History::new();

    assert_eq!(parse_records(&mut pending, &mut history), 2);
    assert!(pending.is_empty());
    assert_eq!(history.len(), 2);
    assert_eq!(history.entries()[0].text, "hello");
    assert_eq!(history.entries()[1].data.as_ref(), b"\x89PNG\0\xff");
}

#[test]
fn malformed_records_are_dropped_without_a_partial_entry() {
    let mut pending = b"WHELK1 2 1\nxx".to_vec();
    let mut history = History::new();

    assert_eq!(parse_records(&mut pending, &mut history), 0);
    assert!(pending.is_empty());
    assert!(history.is_empty());
}

#[test]
fn labels_and_ages_are_useful_in_a_one_line_row() {
    let image = Entry::new("image/png", "", vec![1]);
    let files = Entry::new("text/uri-list", "file:///a\nfile:///b\n", Vec::new());
    let text = Entry::new(
        "text/plain",
        "a long first line\nsecond line",
        b"a long first line\nsecond line".to_vec(),
    );

    assert_eq!(entry_label(&image), "[Image — image/png]");
    assert_eq!(entry_label(&files), "2 files");
    assert_eq!(entry_label(&text), "a long first line");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(2048), "2.0 KiB");
    assert_eq!(format_age(1_000_000, 301_000_000), "5m ago");
    assert!(entry_detail(&text).contains("Text"));
}

#[test]
fn service_mutations_keep_pins_and_the_single_undo() {
    let clipboard = Clipboard::new(ClipboardSettings::default());

    assert_eq!(clipboard.status(), Status::Unavailable);
    assert!(clipboard.add("text/plain", "one", b"one".to_vec()));
    assert!(clipboard.add("text/plain", "two", b"two".to_vec()));

    let two = clipboard.entries()[0].id.clone();

    assert!(clipboard.toggle_pin(&two));
    assert!(clipboard.entries()[0].pinned);
    assert!(clipboard.remove(&two));
    assert!(clipboard.undo_remove());
    assert!(clipboard.find(&two).is_some());

    assert!(clipboard.clear_unpinned());
    assert_eq!(clipboard.count(), 1);
    clipboard.clear();
    assert_eq!(clipboard.count(), 0);
}

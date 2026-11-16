// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::json::Json;

/* The shapes below are the ones niri actually answers with:
 *   niri msg -j focused-window  -> {"id":3,"title":"...","app_id":"..."}
 *   niri msg -j workspaces      -> [{"id":1,"idx":1,...,"active_window_id":null}]
 */

#[test]
fn object() {
    let document =
        r#"{"id": 7, "title": "a \"quoted\" café", "ok": true, "none": null, "list": [1, 2, {}]}"#;
    let root = Json::parse(document).expect("parse the document");

    assert!(matches!(root, Json::Object(_)));
    assert_eq!(root.get_number("id", -1.0), 7.0);
    assert_eq!(root.get_str("title"), Some("a \"quoted\" café"));
    assert!(root.get_bool("ok", false));
    assert!(root.get("none").is_some_and(Json::is_null));

    let list = root.get("list").expect("the list");
    assert_eq!(list.len(), 3);
    assert_eq!(list.at(1).and_then(Json::as_f64), Some(2.0));
    assert!(matches!(list.at(2), Some(Json::Object(_))));
    assert_eq!(list.at(3), None);

    // A missing key is a fallback, not a crash.
    assert_eq!(root.get_str("absent"), None);
    assert!(!root.get_bool("absent", false));
    assert_eq!(root.get_number("absent", 4.5), 4.5);
    assert!(root.get_bool("id", true));
}

#[test]
fn workspaces() {
    let document = r#"[{"id":1,"idx":1,"name":"one","is_active":true,"is_focused":true,"active_window_id":12},{"id":2,"idx":2,"name":null,"is_active":false,"is_focused":false,"active_window_id":null}]"#;
    let root = Json::parse(document).expect("parse the workspaces");

    assert_eq!(root.len(), 2);

    let first = root.at(0).expect("the first workspace");
    assert_eq!(first.get_number("idx", -1.0), 1.0);
    assert!(first.get_bool("is_active", false));
    assert!(matches!(
        first.get("active_window_id"),
        Some(Json::Number(_))
    ));

    let second = root.at(1).expect("the second workspace");
    assert!(second.get("active_window_id").is_some_and(Json::is_null));
}

#[test]
fn invalid_documents() {
    assert!(Json::parse(r#"{"a": 1"#).is_err());
    assert!(Json::parse("").is_err());
    assert!(Json::parse(r#"{"a": 1} trailing"#).is_err());
    assert!(Json::parse("not json").is_err());
    assert!(Json::parse(r#"{"a" 1}"#).is_err());
    assert!(Json::parse(r#"[1 2]"#).is_err());
}

#[test]
fn escapes_and_surrogate_pairs() {
    let value = Json::parse(r#""\u00e9 \ud83d\ude00 \n\t\\/\"""#).expect("escapes");

    assert_eq!(value.as_str(), Some("é 😀 \n\t\\/\""));

    // A lone surrogate is replaced rather than producing invalid UTF-8.
    let lone = Json::parse(r#""\ud800""#).expect("lone surrogate");

    assert_eq!(lone.as_str(), Some("\u{FFFD}"));
}

#[test]
fn numbers_and_duplicate_keys() {
    let root = Json::parse(r#"{"a": -1.5e2, "a": 2, "zero": 0}"#).expect("numbers");

    assert_eq!(root.get_number("a", 0.0), -150.0);
    assert_eq!(root.get_number("zero", -1.0), 0.0);
}

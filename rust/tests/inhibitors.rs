// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The inhibitor reader turns `systemd-inhibit --list` into the answer to "why
//! did my screen not blank".  The parsing is the part worth testing: the
//! command's columns are padded, and a reason can contain spaces.

use whelk_rs::inhibitors::{Inhibitor, describe, parse};

const SAMPLE: &str = "WHO            UID  USER  PID  COMM           WHAT                   WHY                          MODE\n\
ModemManager   0    root  754  ModemManager   sleep                  Modem dialing                delay\n\
firefox        1000  joe   4242  firefox        idle                   Playing video                block\n";

#[test]
fn parses_the_padded_columns_and_keeps_spaces_in_a_reason() {
    let list = parse(SAMPLE);

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].who, "ModemManager");
    assert_eq!(list[0].what, "sleep");
    assert_eq!(list[0].why, "Modem dialing");
    assert_eq!(list[0].mode, "delay");
    assert_eq!(list[1].why, "Playing video");

    let text = describe(&list).expect("two inhibitors have a description");

    assert!(text.contains("2 inhibitors"), "{text}");
    assert!(
        text.contains("sleep — ModemManager: Modem dialing"),
        "{text}"
    );
    assert!(text.contains("Playing video"), "{text}");
    assert!(text.contains("(block)"), "{text}");
}

#[test]
fn empty_text_and_the_header_produce_nothing() {
    assert!(parse("").is_empty());
    assert!(parse("WHO UID USER PID COMM WHAT WHY MODE\n").is_empty());
    assert_eq!(describe(&[]), None);
}

#[test]
fn a_changed_table_shape_is_not_an_inhibitor() {
    assert!(parse("systemd-inhibit: command not found\n").is_empty());
}

#[test]
fn one_row_uses_the_singular_and_the_documented_defaults() {
    let list = vec![Inhibitor {
        who: String::new(),
        what: String::new(),
        why: String::new(),
        mode: String::new(),
    }];

    assert_eq!(
        describe(&list).as_deref(),
        Some("1 inhibitor\nidle — an application: no reason given (block)")
    );
}

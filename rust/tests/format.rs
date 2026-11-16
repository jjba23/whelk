// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The three helpers every module uses: substitution, markup escaping and
//! truncation on a character boundary.

use whelk_rs::format::{
    first_line, icon_markup, markup_escape, str_gib, strip_newline, subst, truncate_utf8,
};

#[test]
fn substitution_replaces_every_placeholder() {
    assert_eq!(subst("{icon} {usage}%", "usage", "42"), "{icon} 42%");
    assert_eq!(subst("{a}{a}", "a", "x"), "xx");
    assert_eq!(subst("{usage}", "other", "42"), "{usage}");
    assert_eq!(subst("plain", "", "x"), "plain");
}

#[test]
fn markup_is_escaped_for_pango() {
    assert_eq!(markup_escape("a & <b>"), "a &amp; &lt;b&gt;");
    assert_eq!(markup_escape("\"quoted\""), "&quot;quoted&quot;");
}

#[test]
fn an_icon_can_name_its_font_family() {
    assert_eq!(icon_markup("", Some("Icons")), "");
    assert_eq!(icon_markup("<", None), "&lt;");
    assert_eq!(
        icon_markup("n", Some("Icons \"Mono\"")),
        "<span font_family=\"Icons &quot;Mono&quot;\">n</span>"
    );
}

#[test]
fn truncation_happens_on_a_character_boundary() {
    assert_eq!(truncate_utf8("hello", 0), "hello");
    assert_eq!(truncate_utf8("hello", 5), "hello");
    assert_eq!(truncate_utf8("hello world", 6), "hello…");
    assert_eq!(truncate_utf8("héllo wörld", 5), "héllo…");
    assert_eq!(truncate_utf8("short", 99), "short");
}

#[test]
fn line_helpers_use_the_first_line_and_trim_endings() {
    assert_eq!(first_line("first\nsecond"), "first");
    assert_eq!(first_line("first\r\nsecond"), "first");
    assert_eq!(first_line("only"), "only");
    assert_eq!(first_line("a\r\r\n"), "a");
    assert_eq!(strip_newline("text\n\r"), "text");
}

#[test]
fn sizes_are_binary_gibibytes() {
    assert_eq!(str_gib(9_856_000), "9.4 GiB");
    assert_eq!(str_gib(1_048_576), "1.0 GiB");
}

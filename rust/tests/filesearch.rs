// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Searching the whole home directory: which paths a query finds, in what
//! order, and what the walk is told to skip.

use whelk_rs::filesearch::{command, ignored, rank, score};

#[test]
fn a_basename_match_beats_a_parent_directory_match() {
    assert!(
        score("notes", "/home/joe/notes/lista.txt", false)
            > score("notes", "/home/joe/lista-notes/tmp.txt", false)
    );
}

#[test]
fn a_prefix_beats_a_substring() {
    assert!(score("bar", "/home/joe/bar.c", false) > score("bar", "/home/joe/foobar.c", false));
}

#[test]
fn a_shallow_match_beats_a_deep_one() {
    assert!(
        score("whelk", "/home/joe/whelk/src/bar.c", false)
            > score("whelk", "/home/joe/a/b/c/d/e/whelk/bar.c", false)
    );
}

#[test]
fn matching_is_case_insensitive() {
    assert!(score("WHELK", "/home/joe/whelk/README.org", false) > -1);
    assert!(score("whelk", "/home/joe/Whelk/README.org", false) > -1);
}

#[test]
fn misses_are_misses() {
    assert_eq!(score("zzzz", "/home/joe/whelk", true), -1);
    assert_eq!(score("", "/home/joe/whelk", true), 0);
    assert_eq!(score("a", "", false), -1);
}

#[test]
fn files_beat_directories_of_the_same_name() {
    assert!(
        score("whelk", "/home/joe/src/whelk", true) < score("whelk", "/home/joe/src/whelk", false)
    );
}

#[test]
fn ranking_orders_and_limits_the_walk() {
    let output = "f\t/home/joe/notes/lista.txt\n\
f\t/home/joe/whelk/README.org\n\
d\t/home/joe/whelk\n\
f\t/home/joe/whelk/src/bar.c\n\
\n";

    assert_eq!(
        rank("whelk", output, 10),
        vec![
            "/home/joe/whelk",
            "/home/joe/whelk/README.org",
            "/home/joe/whelk/src/bar.c",
        ]
    );
    assert_eq!(rank("whelk", output, 1).len(), 1);
    assert!(rank("", "/home/joe/whelk\n", 10).is_empty());
}

#[test]
fn the_command_prunes_noise_and_prints_types() {
    let command = command("/home/joe", 6);

    assert!(command.contains("'/home/joe'"), "{command}");
    assert!(command.contains("-maxdepth 6"), "{command}");
    assert!(command.contains("-prune"), "{command}");
    assert!(command.contains("-type d"), "{command}");
    assert!(command.contains("-printf"), "{command}");
    assert!(!command.contains("-iname"), "{command}");

    for name in ignored() {
        assert!(command.contains(&format!("-name {name}")), "{name}");
    }
}

#[test]
fn a_root_with_a_quote_is_quoted_safely() {
    let command = command("/home/o'brien", 2);

    assert!(command.contains("'/home/o'\\''brien'"), "{command}");
}

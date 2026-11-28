// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The counts the manuals state, checked against the code that owns them.
//!
//! A number in prose has no way to notice that it stopped being true: a module
//! is added, a test file is written, and the page keeps saying yesterday's
//! number for a release or two.  These tests are the notice.  They count what
//! the code declares -- `#[test]` functions, the factory table's kinds, the
//! bar modules -- and compare that with the words the pages use.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate lives in the repository")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative))
        .unwrap_or_else(|error| panic!("could not read {relative}: {error}"))
}

fn rust_sources(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            rust_sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Every `#[test]` function in the crate, comment lines skipped: a page that
/// mentions the attribute in prose is not a test.
fn count_tests() -> usize {
    let mut files = Vec::new();

    rust_sources(&root().join("rust/src"), &mut files);
    rust_sources(&root().join("rust/tests"), &mut files);

    files
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));

            // The attribute on its own line: a line that merely mentions it,
            // in a comment or in this test's own filter, is not a test.
            source
                .lines()
                .filter(|line| line.trim_start().starts_with("#[test]"))
                .count()
        })
        .sum()
}

/// The kinds the bar's factory table answers to.
fn module_kinds() -> usize {
    let source = read("rust/src/barfactory.rs");
    let table = source
        .split("const FACTORIES: &[(&str, ModuleFactory)] = &[")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("the factory table is declared in barfactory.rs");

    table
        .lines()
        .filter(|line| line.trim_start().starts_with("(\""))
        .count()
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];

const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// The word the manuals use for a count, as in "twenty-five module kinds".
fn words(count: usize) -> String {
    if count < ONES.len() {
        ONES[count].to_owned()
    } else if count < 100 && count.is_multiple_of(10) {
        TENS[count / 10].to_owned()
    } else if count < 100 {
        format!("{}-{}", TENS[count / 10], ONES[count % 10])
    } else {
        count.to_string()
    }
}

#[test]
fn the_stated_test_count_is_the_real_one() {
    let count = count_tests();

    for page in ["docs/roadmap.org", "docs/development.org"] {
        let text = read(page);

        assert!(
            text.contains(&format!("{count} tests")),
            "{page} must say \"{count} tests\"; the suite has {count}. \
             Adding or removing a test changes the number the pages owe."
        );
    }
}

#[test]
fn the_stated_module_count_is_the_real_one() {
    let kinds = module_kinds();
    let word = words(kinds);

    for (page, phrase) in [
        ("docs/modules.org", format!("{word} module kinds")),
        ("README.org", format!("{word} module kinds")),
        ("docs/index.org", format!("the {word} kinds")),
    ] {
        let text = read(page).to_lowercase();

        assert!(
            text.contains(&phrase.to_lowercase()),
            "{page} must say \"{phrase}\"; the factory table has {kinds} kinds."
        );
    }

    // The page's own table is the list of those kinds, so it has to grow with
    // the factory table rather than a release later.
    let rows = read("docs/modules.org")
        .lines()
        .filter(|line| line.starts_with("| =") && line.contains("-module="))
        .count();

    assert_eq!(
        rows, kinds,
        "docs/modules.org has {rows} module rows and barfactory.rs {kinds} kinds"
    );
}

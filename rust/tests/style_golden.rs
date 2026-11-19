// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Golden CSS: the sheet a representative bar is given, byte for byte.
//!
//! The inputs are small settings files under `tests/fixtures/css/`: the built-in
//! bar, the XP preset, one bar per theme, both ends of the spacing clamp and a
//! bar that brings its own `#:css`.  Each `.scm` has a `.css` beside it holding
//! what `whelk --print-css` printed for it.  A change to a selector, a derived
//! inset or a material formula fails here instead of waiting for someone to
//! look at the bar.
//!
//! The goldens are read-only in an ordinary run.  After deciding a change is
//! right, refresh them deliberately:
//!
//! ```text
//! WHELK_BLESS=1 cargo test -p whelk-rs --test style_golden
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate lives in the repository")
        .to_path_buf()
}

fn fixtures() -> PathBuf {
    repository().join("tests/fixtures/css")
}

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scheme")
}

/// Every case, by file stem, in a stable order.
fn cases(fixtures: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(fixtures)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", fixtures.display()));
    let mut cases: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "scm"))
        .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .collect();

    cases.sort();

    cases
}

/// The sheet `--print-css` prints for one case, without its first line.
///
/// That line is the flag's own header (`/* whelk X.Y.Z: bar "main", theme … */`).
/// It names the release, and a version bump is not a stylesheet change; the case
/// file already says which bar and theme it is.
fn generated(fixtures: &Path, case: &str, state: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_whelk"))
        // A settings-window override lives under XDG_STATE_HOME and wins over
        // the file, so the fixtures get a state directory of their own.
        .env("XDG_STATE_HOME", state)
        .env("WHELK_SCHEME_DIR", scheme_dir())
        .arg("--print-css")
        .arg("--config")
        .arg(fixtures.join(format!("{case}.scm")))
        .output()
        .expect("run whelk --print-css");

    assert!(
        output.status.success(),
        "{case}.scm did not load: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .split_once('\n')
        .map(|(_header, sheet)| sheet.to_owned())
        .unwrap_or_default()
}

/// Where two sheets first disagree, in a line a test failure can carry.
fn difference(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();

    for (index, (left, right)) in expected.iter().zip(actual.iter()).enumerate() {
        if left != right {
            return format!("line {}: expected {left:?}, got {right:?}", index + 1);
        }
    }

    match expected.len().cmp(&actual.len()) {
        std::cmp::Ordering::Equal => "the sheets are equal".to_owned(),
        std::cmp::Ordering::Less => format!(
            "the sheet has {} more line(s) than the fixture",
            actual.len() - expected.len()
        ),
        std::cmp::Ordering::Greater => format!(
            "the fixture has {} more line(s) than the sheet",
            expected.len() - actual.len()
        ),
    }
}

#[test]
fn the_generated_skins_match_their_fixtures() {
    let fixtures = fixtures();
    let cases = cases(&fixtures);

    assert!(
        !cases.is_empty(),
        "no CSS cases in {}: the fixtures are the point",
        fixtures.display()
    );

    let state = tempfile::tempdir().expect("a state directory");
    let bless = std::env::var_os("WHELK_BLESS").is_some();
    let mut failures = Vec::new();

    for case in &cases {
        let actual = generated(&fixtures, case, state.path());
        let path = fixtures.join(format!("{case}.css"));

        if bless {
            std::fs::write(&path, &actual)
                .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));

            continue;
        }

        let expected = std::fs::read_to_string(&path).unwrap_or_default();

        if expected != actual {
            failures.push(format!("  {case}.css: {}", difference(&expected, &actual)));
        }
    }

    assert!(
        failures.is_empty(),
        "the generated CSS no longer matches its fixtures:\n{}\n\
         Read the case file and the diff; if the new sheet is right, refresh it with:\n  \
         WHELK_BLESS=1 cargo test -p whelk-rs --test style_golden",
        failures.join("\n")
    );
}

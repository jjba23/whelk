// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Guile half of the test suite.  Meson used to register these scripts;
//! they still pin the built-in bar, the menu dictionary and the settings
//! constructors, so Cargo runs them beside the Rust tests.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ has a workspace parent")
        .to_path_buf()
}

#[test]
fn the_scheme_suite_agrees_with_the_installed_data() {
    let root = root();
    let scheme = root.join("scheme");
    let tests = root.join("tests");
    let guile = Command::new("guile").arg("--version").output();

    if guile.is_err() {
        eprintln!("skipping the Scheme suite: guile is not installed");

        return;
    }

    let cases: &[(&str, &[&str])] = &[
        ("test_themes.scm", &[]),
        ("test_menudict.scm", &[]),
        ("test_schememodule.scm", &[]),
        ("test_defaults.scm", &["settings/settings.scm"]),
        ("test_presets.scm", &[]),
        ("test_barmonitors.scm", &[]),
        ("test_prefs.scm", &[]),
        ("test_wallpaper.scm", &[]),
        ("test_widgets.scm", &[]),
    ];

    for (script, extra) in cases {
        let output = Command::new("guile")
            .arg("--no-auto-compile")
            .arg("-L")
            .arg(&scheme)
            .arg(tests.join(script))
            .args(extra.iter().map(|path| root.join(path)))
            .env("WHELK_TEST_DIR", &tests)
            .env("WHELK_CSS_PATH", root.join("resources/css/whelk.css"))
            .env("GUILE_LOAD_PATH", &scheme)
            .output()
            .unwrap_or_else(|error| panic!("could not run {script}: {error}"));

        assert!(
            output.status.success(),
            "{script} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(Path::new(&root.join("scheme/whelk/core.scm")).is_file());
}

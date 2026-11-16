// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_whelk-rs")
}

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scheme")
}

fn run(args: &[&str], config: Option<&Path>) -> std::process::Output {
    let mut command = Command::new(binary());
    command.env("WHELK_SCHEME_DIR", scheme_dir());
    command.args(args);

    if let Some(config) = config {
        command.arg("--config").arg(config);
    }

    command.output().expect("run whelk-rs")
}

#[test]
fn version_does_not_need_a_display_or_a_config() {
    let output = run(&["--version"], None);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("whelk-rs {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn check_config_reads_scheme_and_reports_the_policy() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("settings.scm");
    std::fs::write(
        &config,
        r#"
(configure-shell!
 #:bars (list (bar #:name 'main))
 #:wallpaper (wallpaper-settings #:enabled? #t
                                #:paths (list "/walls")
                                #:interval 300
                                #:animation 'slide-left
                                #:fit 'contain))
"#,
    )
    .expect("write settings");

    let output = run(&["--check-config"], Some(&config));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("wallpaper: enabled"));
    assert!(stdout.contains("slide-left"));
    assert!(stdout.contains("300"));
}

#[test]
fn a_missing_config_still_reports_the_default_policy() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("missing.scm");

    let output = run(&["--check-config"], Some(&config));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("wallpaper: off"));
}

#[test]
fn a_broken_config_exits_with_a_clear_error() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("settings.scm");
    std::fs::write(&config, "(configure-shell! #:bars (").expect("write settings");

    let output = run(&["--check-config"], Some(&config));

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wallpaper settings"));
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The no-file path is also the recovery path for a settings file deleted while
// the shell is running: it must produce the built-in bar without initializing
// Guile or asking for a display.

use std::path::PathBuf;

use whelk_rs::config::ConfigLoader;

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scheme")
}

#[test]
fn a_missing_file_loads_the_builtin_bar_and_a_clear_label() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("missing.scm");
    let mut loader = ConfigLoader::new(path.clone(), Vec::new());
    let loaded = loader.load().expect("default configuration");

    assert!(!loaded.wallpaper.enabled);
    assert_eq!(loaded.bars.len(), 1);
    assert_eq!(loaded.bars[0].name, "main");
    assert_eq!(loaded.idle_lock_after, 0);
    assert_eq!(
        loader.label(),
        format!("{} (missing; using defaults)", path.display())
    );
}

#[test]
fn a_file_path_is_the_label_when_the_file_exists() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("settings.scm");

    std::fs::write(&path, "(configure-shell! #:bars (list (bar #:name 'main)))")
        .expect("write settings");

    let loader = ConfigLoader::new(path.clone(), Vec::new());

    assert_eq!(loader.label(), path.display().to_string());
}

#[test]
fn a_real_configuration_can_be_reloaded_with_the_same_loader() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("settings.scm");

    std::fs::write(
        &path,
        "(configure-shell! #:bars (list (bar #:name 'one) (bar #:name 'two)))",
    )
    .expect("write first settings");

    let mut loader = ConfigLoader::new(path.clone(), vec![scheme_dir()]);
    let first = loader.load().expect("load first settings");

    assert_eq!(first.bars.len(), 2);
    assert_eq!(first.bars[0].name, "one");
    assert_eq!(first.bars[1].name, "two");

    std::fs::write(
        &path,
        "(configure-shell! #:bars (list (bar #:name 'reloaded)))",
    )
    .expect("write second settings");

    let second = loader.load().expect("reload settings");

    assert_eq!(second.bars.len(), 1);
    assert_eq!(second.bars[0].name, "reloaded");
}

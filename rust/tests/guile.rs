// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::PathBuf;

use whelk_rs::guile::{Guile, scheme_string_literal};
use whelk_rs::wallpaper::{Animation, Fit};

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scheme")
}

fn write_settings(directory: &std::path::Path, body: &str) -> PathBuf {
    let path = directory.join("settings.scm");
    std::fs::write(&path, body).expect("write test settings");
    path
}

#[test]
fn scheme_strings_are_escaped() {
    assert_eq!(scheme_string_literal(r#"a"b\c"#), r#""a\"b\\c""#);
    assert_eq!(scheme_string_literal("line\nnext"), "\"line\\nnext\"");
}

// Guile is process-global and thread-affine.  Keep every call in one test so
// Cargo's test threads cannot initialize or enter it twice.
#[test]
fn guile_reads_the_real_shell_wallpaper_settings() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config_path = write_settings(
        directory.path(),
        r#"
(configure-shell!
 #:wallpaper (wallpaper-settings
              #:enabled? #t
              #:paths (list "/walls" "/more/walls")
              #:interval 300
              #:shuffle? #f
              #:animation 'zoom
              #:animation-duration 800
              #:fit 'contain
              #:ken-burns? #t
              #:video? #f
              #:mute-video? #f
              #:current "/walls/start.avif"
              #:picker-width 1200
              #:picker-height 700
              #:picker-columns 5))
"#,
    );

    let guile = Guile::init(&[scheme_dir()]).expect("initialize Guile");
    let version = guile.version().expect("read Guile version");
    assert!(version.starts_with("3."), "unexpected Guile {version}");

    let settings = guile
        .load_wallpaper(&config_path)
        .expect("load wallpaper settings");

    assert!(settings.configured);
    assert!(settings.enabled);
    assert_eq!(
        settings.paths,
        vec![PathBuf::from("/walls"), PathBuf::from("/more/walls")]
    );
    assert_eq!(settings.interval_seconds, 300);
    assert!(!settings.shuffle);
    assert_eq!(settings.animation, Animation::Zoom);
    assert_eq!(settings.animation_ms, 800);
    assert_eq!(settings.fit, Fit::Contain);
    assert!(settings.ken_burns);
    assert!(!settings.video);
    assert!(!settings.mute_video);
    assert_eq!(settings.current, Some(PathBuf::from("/walls/start.avif")));
    assert_eq!(settings.picker_width, 1200);
    assert_eq!(settings.picker_height, 700);
    assert_eq!(settings.picker_columns, 5);

    let rust_version = guile
        .eval_string("(begin (use-modules (whelk core)) (whelk-rust-version))")
        .expect("call the Rust primitive from Scheme");
    assert_eq!(rust_version, env!("CARGO_PKG_VERSION"));

    // Repeated loads prove that the alist is protected for the whole
    // extraction, not rooted by luck.
    for _ in 0..32 {
        let again = guile.load_wallpaper(&config_path).expect("reload");
        assert_eq!(again.animation, Animation::Zoom);
    }

    let missing = directory.path().join("missing.scm");
    assert!(guile.load_wallpaper(&missing).is_err());

    let broken = write_settings(directory.path(), "(configure-shell! #:wallpaper (");
    assert!(guile.load_wallpaper(&broken).is_err());

    // The evaluator is still usable after the caught exception.
    assert!(
        guile
            .eval_string("(+ 1 2)")
            .expect("evaluate")
            .contains('3')
    );
}

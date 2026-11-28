// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The Scheme theme, read through the real Guile runtime.
//!
//! libtest runs a `#[test]` on a worker thread; Guile's GC signal delivery
//! needs the process main thread.  This test binary therefore has its own main
//! and no libtest harness, and one `Guile` handle serves every check.

use std::path::PathBuf;

use bibelot::guile::Guile;
use bibelot::theme::Theme;

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scheme")
}

fn defaults_match_the_scheme_module(guile: &Guile) {
    let theme = guile.theme(None).expect("the default theme");

    assert_eq!(
        theme,
        Theme::default(),
        "scheme/bibelot/theme.scm and Theme::default must agree"
    );
    assert!(theme.extra_css.is_empty());
}

fn a_user_file_overrides_only_what_it_sets(guile: &Guile) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("theme.scm");

    std::fs::write(
        &path,
        r##"
(configure-widgets! #:card-padding 18
                    #:card-gap 12
                    #:extra-css ".whelk-widget-clock-time { letter-spacing: 2px; }")
"##,
    )
    .expect("write the theme");

    let theme = guile.theme(Some(&path)).expect("the configured theme");

    assert_eq!(theme.card_padding, 18);
    assert_eq!(theme.card_gap, 12);
    assert_eq!(Theme::default().card_padding, 14);

    let css = theme.css();

    assert!(css.contains("padding: 18px;"));
    assert!(css.contains("letter-spacing: 2px;"));
}

fn a_configuration_that_raises_is_refused(guile: &Guile) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("theme.scm");

    std::fs::write(&path, "(configure-widgets! #:card-padding").expect("write the broken theme");

    let error = guile
        .theme(Some(&path))
        .expect_err("a broken theme must not be accepted");

    assert!(
        format!("{error}").contains("Scheme raised an exception"),
        "{error}"
    );

    // The runtime is still usable after the caught exception.
    let theme = guile.theme(None).expect("the defaults after a broken file");

    assert_eq!(theme, Theme::default());
}

fn main() {
    let guile = Guile::init(&[scheme_dir()]).expect("Guile starts");

    defaults_match_the_scheme_module(&guile);
    a_user_file_overrides_only_what_it_sets(&guile);
    a_configuration_that_raises_is_refused(&guile);
}

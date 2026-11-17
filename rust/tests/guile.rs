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
              #:picker-columns 5)
 #:idle (idle-settings #:lock-after 300)
 #:bars (list (bar #:name 'rust-test
                    #:height 40
                    #:background "rgba(10, 20, 30, 0.5)"
                    #:opacity 0.8
                    #:left (list (custom-module #:exec "printf hi" #:interval 0))
                    #:right (list (clock-module #:format "%H:%M"))))
 #:widgets (list (desktop-widget 'clock #:x 40 #:y 64)
                (desktop-widget 'notes
                                #:x 400 #:y 64
                                #:options '((text . "Ship the release.")))))
"#,
    );

    // The preference file is keyed by the settings file's stem.  Keep the
    // test's "settings.scm" from reading the developer's real overrides.
    unsafe {
        // The test owns the process and no other test enters Guile, so this
        // cannot race with a concurrent environment read.
        std::env::set_var("XDG_STATE_HOME", directory.path());
    }

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

    assert_eq!(
        guile
            .load_idle_lock_after(&config_path)
            .expect("load idle settings"),
        300
    );

    // A settings file that names no menu gets the default one, read through
    // the same Scheme units a custom menu uses.
    let menu = guile
        .load_shell_menu(&config_path)
        .expect("load shell menu");

    assert!(!menu.is_empty());
    assert!(menu.iter().any(|entry| matches!(
        entry,
        whelk_rs::startmenu::MenuEntry::Item {
            label,
            ..
        } if label == "About This System"
    )));

    let custom_path = directory.path().join("custom-menu.scm");

    std::fs::write(
        &custom_path,
        r#"
(configure-shell!
 #:bars '()
 #:shell-menu (list (list 'item "Search" "shell:launcher")
                     (list 'separator)
                     (list 'submenu "Recent Items" "shell:recent-items")))
"#,
    )
    .expect("write custom menu settings");

    let custom = guile
        .load_shell_menu(&custom_path)
        .expect("load custom shell menu");

    assert_eq!(custom.len(), 3);
    assert!(matches!(
        &custom[0],
        whelk_rs::startmenu::MenuEntry::Item {
            label,
            action: Some(action),
            enabled: true,
        } if label == "Search" && action == "shell:launcher"
    ));
    assert!(matches!(
        &custom[2],
        whelk_rs::startmenu::MenuEntry::Submenu {
            marker: Some(marker),
            ..
        } if marker == "shell:recent-items"
    ));

    // The desktop widgets' placement and own options come from the same
    // settings file, through `(shell-widgets->alists)`.
    let widgets = guile
        .load_widgets(&config_path)
        .expect("load desktop widgets");

    assert_eq!(widgets.len(), 2);
    assert_eq!(widgets[0].spec.kind, "clock");
    assert_eq!((widgets[0].x, widgets[0].y), (40, 64));
    assert_eq!((widgets[0].width, widgets[0].height), (220, 240));
    assert_eq!(widgets[0].monitor, "default");
    assert_eq!(widgets[1].title(), "Notes");
    assert_eq!((widgets[1].x, widgets[1].y), (400, 64));
    assert_eq!(
        widgets[1].spec.text("text").as_deref(),
        Some("Ship the release.")
    );

    let menu_sources = guile
        .load_menu_sources(&config_path)
        .expect("load application menu sources");
    let firefox = menu_sources
        .dictionary_for("org.mozilla.firefox")
        .expect("the dictionary knows Firefox");

    assert_eq!(firefox.name, "Firefox");

    let firefox_file = firefox
        .menus
        .iter()
        .find(|menu| menu.label == "File")
        .expect("Firefox has a File menu");

    assert!(firefox_file.items.iter().any(|item| {
        item.label == "New Tab"
            && item
                .sequence
                .as_ref()
                .and_then(|sequence| sequence.format(whelk_rs::chord::ChordStyle::Cua))
                .as_deref()
                == Some("Ctrl+T")
    }));

    let orbit = menu_sources
        .orbit_for("org.kde.akregator")
        .expect("the Orbit corpus knows Akregator");

    assert_eq!(orbit.name, "Akregator");
    assert!(orbit.menus.len() >= 8);

    let orbit_file = orbit
        .menus
        .iter()
        .find(|menu| menu.label == "File")
        .expect("Akregator has a File menu");

    assert!(orbit_file.entries.iter().any(|entry| {
        matches!(entry, whelk_rs::menuapps::OrbitEntry::Item { label, shortcut, .. }
            if label == "Print…" && shortcut.as_deref() == Some("<Ctrl>P"))
    }));

    let bars = guile.load_bars(&config_path).expect("load bar settings");
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].name, "rust-test");
    assert_eq!(bars[0].height, 40);
    assert_eq!(bars[0].opacity, 0.8);
    assert_eq!(bars[0].left[0].kind, "custom");
    assert_eq!(bars[0].left[0].integer("interval"), Some(0));
    assert_eq!(bars[0].clock_format().as_deref(), Some("%H:%M"));

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

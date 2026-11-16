// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::settings::{SchemeValue, WallpaperSettings, config_path_from, expand_home};
use whelk_rs::wallpaper::{Animation, Fit};

fn alist(entries: &[(&str, SchemeValue)]) -> Vec<(String, SchemeValue)> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[test]
fn defaults_match_the_c_shell() {
    let settings = WallpaperSettings::default();

    assert!(!settings.configured);
    assert!(!settings.enabled);
    assert!(settings.paths.is_empty());
    assert_eq!(settings.interval_seconds, 900);
    assert!(settings.shuffle);
    assert_eq!(settings.animation, Animation::Crossfade);
    assert_eq!(settings.animation_ms, 1200);
    assert_eq!(settings.fit, Fit::Cover);
    assert!(!settings.ken_burns);
    assert!(settings.video);
    assert!(settings.mute_video);
    assert_eq!(settings.current, None);
    assert_eq!(settings.picker_width, 960);
    assert_eq!(settings.picker_height, 640);
    assert_eq!(settings.picker_columns, 4);
}

#[test]
fn the_scheme_alist_becomes_typed_settings() {
    let values = alist(&[
        ("configured?", SchemeValue::Bool(true)),
        ("enabled?", SchemeValue::Bool(true)),
        (
            "paths",
            SchemeValue::List(vec![
                SchemeValue::Text("~/Walls".into()),
                SchemeValue::Text("/srv/walls".into()),
                SchemeValue::Text(String::new()),
            ]),
        ),
        ("interval", SchemeValue::Integer(300)),
        ("shuffle?", SchemeValue::Bool(false)),
        ("animation", SchemeValue::Symbol("slide-left".into())),
        ("animation-duration", SchemeValue::Integer(800)),
        ("fit", SchemeValue::Symbol("contain".into())),
        ("ken-burns?", SchemeValue::Bool(true)),
        ("video?", SchemeValue::Bool(false)),
        ("mute-video?", SchemeValue::Bool(false)),
        ("current", SchemeValue::Text("~/Walls/start.avif".into())),
        ("picker-width", SchemeValue::Integer(1200)),
        ("picker-height", SchemeValue::Integer(700)),
        ("picker-columns", SchemeValue::Integer(5)),
    ]);

    let settings = WallpaperSettings::with_home(&values, Some("/home/joe"));

    assert!(settings.configured);
    assert!(settings.enabled);
    assert_eq!(
        settings.paths,
        vec![
            std::path::PathBuf::from("/home/joe/Walls"),
            std::path::PathBuf::from("/srv/walls"),
        ]
    );
    assert_eq!(settings.interval_seconds, 300);
    assert!(!settings.shuffle);
    assert_eq!(settings.animation, Animation::SlideLeft);
    assert_eq!(settings.animation_ms, 800);
    assert_eq!(settings.fit, Fit::Contain);
    assert!(settings.ken_burns);
    assert!(!settings.video);
    assert!(!settings.mute_video);
    assert_eq!(
        settings.current,
        Some(std::path::PathBuf::from("/home/joe/Walls/start.avif"))
    );
    assert_eq!(settings.picker_width, 1200);
    assert_eq!(settings.picker_height, 700);
    assert_eq!(settings.picker_columns, 5);
}

#[test]
fn an_empty_alist_is_an_unconfigured_policy() {
    let settings = WallpaperSettings::with_home(&[], Some("/home/joe"));

    assert!(!settings.configured);
    assert!(!settings.enabled);
    assert_eq!(settings.animation, Animation::Crossfade);
}

#[test]
fn unknown_names_fall_back_like_the_c_bridge() {
    let values = alist(&[
        ("configured?", SchemeValue::Bool(true)),
        ("animation", SchemeValue::Symbol("explode".into())),
        ("fit", SchemeValue::Symbol("stretch".into())),
    ]);

    let settings = WallpaperSettings::with_home(&values, None);

    assert_eq!(settings.animation, Animation::Crossfade);
    assert_eq!(settings.fit, Fit::Cover);
}

#[test]
fn numbers_and_geometry_are_clamped() {
    let values = alist(&[
        ("configured?", SchemeValue::Bool(true)),
        ("interval", SchemeValue::Integer(1)),
        ("animation-duration", SchemeValue::Integer(20_000)),
        ("picker-width", SchemeValue::Integer(100)),
        ("picker-height", SchemeValue::Integer(9_000)),
        ("picker-columns", SchemeValue::Integer(40)),
    ]);

    let settings = WallpaperSettings::with_home(&values, None);
    assert_eq!(settings.interval_seconds, 5);
    assert_eq!(settings.animation_ms, 10_000);
    assert_eq!(settings.picker_width, 320);
    assert_eq!(settings.picker_height, 2160);
    assert_eq!(settings.picker_columns, 12);

    let values = alist(&[
        ("interval", SchemeValue::Integer(-4)),
        ("animation-duration", SchemeValue::Integer(-2)),
        ("picker-columns", SchemeValue::Integer(0)),
    ]);
    let settings = WallpaperSettings::with_home(&values, None);
    assert_eq!(settings.interval_seconds, 0);
    assert_eq!(settings.animation_ms, 0);
    assert_eq!(settings.picker_columns, 1);

    let values = alist(&[("interval", SchemeValue::Integer(700_000))]);
    assert_eq!(
        WallpaperSettings::with_home(&values, None).interval_seconds,
        604_800
    );
}

#[test]
fn a_single_path_and_a_single_current_string_are_accepted() {
    let values = alist(&[
        ("configured?", SchemeValue::Bool(true)),
        ("paths", SchemeValue::Text("/walls".into())),
        ("current", SchemeValue::Text("/walls/one.webp".into())),
    ]);

    let settings = WallpaperSettings::with_home(&values, None);
    assert_eq!(settings.paths, vec![std::path::PathBuf::from("/walls")]);
    assert_eq!(
        settings.current,
        Some(std::path::PathBuf::from("/walls/one.webp"))
    );
}

#[test]
fn home_expansion_only_touches_the_documented_prefix() {
    assert_eq!(
        expand_home("~/Walls", Some("/home/joe")),
        std::path::PathBuf::from("/home/joe/Walls")
    );
    assert_eq!(
        expand_home("~/", Some("/home/joe")),
        std::path::PathBuf::from("/home/joe/")
    );
    assert_eq!(
        expand_home("~other/Walls", Some("/home/joe")),
        std::path::PathBuf::from("~other/Walls")
    );
    assert_eq!(
        expand_home("/srv/walls", None),
        std::path::PathBuf::from("/srv/walls")
    );
    assert_eq!(
        expand_home("~/Walls", None),
        std::path::PathBuf::from("~/Walls")
    );
}

#[test]
fn summary_is_a_usable_bar_line() {
    let settings = WallpaperSettings::default();
    assert!(settings.summary().contains("off"));

    let values = alist(&[
        ("configured?", SchemeValue::Bool(true)),
        ("enabled?", SchemeValue::Bool(true)),
        ("interval", SchemeValue::Integer(300)),
        ("animation", SchemeValue::Symbol("slide-left".into())),
        ("fit", SchemeValue::Symbol("contain".into())),
    ]);
    let summary = WallpaperSettings::with_home(&values, None).summary();

    assert!(summary.contains("wallpaper"));
    assert!(summary.contains("slide-left"));
    assert!(summary.contains("300"));
}

#[test]
fn config_path_follows_xdg_and_home_rules() {
    assert_eq!(
        config_path_from(Some("/tmp/config"), Some("/home/joe")),
        std::path::PathBuf::from("/tmp/config/whelk/settings.scm")
    );
    assert_eq!(
        config_path_from(None, Some("/home/joe")),
        std::path::PathBuf::from("/home/joe/.config/whelk/settings.scm")
    );
    // g_get_user_config_dir ignores a relative XDG_CONFIG_HOME.
    assert_eq!(
        config_path_from(Some("relative/config"), Some("/home/joe")),
        std::path::PathBuf::from("/home/joe/.config/whelk/settings.scm")
    );
}

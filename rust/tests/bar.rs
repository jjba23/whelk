// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::bar::{BarSettings, Edge, Layer, When};
use whelk_rs::settings::SchemeValue;

fn alist(entries: Vec<(&str, SchemeValue)>) -> Vec<(String, SchemeValue)> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn module(kind: &str, options: Vec<(&str, SchemeValue)>) -> SchemeValue {
    SchemeValue::Alist(alist(vec![
        ("kind", SchemeValue::Symbol(kind.to_owned())),
        ("options", SchemeValue::Alist(alist(options))),
    ]))
}

#[test]
fn defaults_match_the_c_bar_config() {
    let bar = BarSettings::default();

    assert_eq!(bar.name, "main");
    assert_eq!(bar.edge, Edge::Top);
    assert_eq!(bar.layer, Layer::Top);
    assert_eq!(bar.monitor, "all");
    assert!(bar.monitors.is_empty());
    assert_eq!(bar.when, When::Always);
    assert_eq!(bar.height, 32);
    assert_eq!(bar.spacing, 6);
    assert_eq!(bar.padding, 12);
    assert_eq!(bar.radius, 0);
    assert_eq!(bar.opacity, 1.0);
    assert!(bar.exclusive);
    assert_eq!(bar.launcher_width, 660);
    assert_eq!(bar.launcher_height, 640);
    assert_eq!(bar.launcher_icon_size, 24);
    assert_eq!(bar.launcher_font_scale, 100);
    assert!(bar.left.is_empty());
    assert!(bar.right.is_empty());
}

#[test]
fn the_scheme_alist_becomes_a_typed_bar() {
    let values = alist(vec![
        ("name", SchemeValue::Symbol("dock".into())),
        ("theme", SchemeValue::Symbol("gloss-2007".into())),
        ("edge", SchemeValue::Symbol("bottom".into())),
        ("layer", SchemeValue::Symbol("overlay".into())),
        ("monitor", SchemeValue::Text("all".into())),
        (
            "monitors",
            SchemeValue::List(vec![SchemeValue::Text("all".into())]),
        ),
        ("when", SchemeValue::Symbol("multi".into())),
        ("height", SchemeValue::Integer(40)),
        ("margin-top", SchemeValue::Integer(1)),
        ("margin-right", SchemeValue::Integer(2)),
        ("margin-bottom", SchemeValue::Integer(3)),
        ("margin-left", SchemeValue::Integer(4)),
        ("exclusive?", SchemeValue::Bool(false)),
        ("spacing", SchemeValue::Integer(8)),
        ("padding", SchemeValue::Integer(16)),
        ("font", SchemeValue::Text("Inter".into())),
        ("font-size", SchemeValue::Text("11pt".into())),
        ("icon-font", SchemeValue::Text("Icons".into())),
        ("foreground", SchemeValue::Text("#f4f4f6".into())),
        (
            "background",
            SchemeValue::Text("rgba(26, 26, 28, 0.82)".into()),
        ),
        ("accent", SchemeValue::Text("#3b9eff".into())),
        ("border-color", SchemeValue::Text("#000000".into())),
        ("radius", SchemeValue::Integer(14)),
        ("opacity", SchemeValue::Number(0.5)),
        ("blur", SchemeValue::Integer(70)),
        ("shine", SchemeValue::Number(1.5)),
        ("reflection", SchemeValue::Number(-1.0)),
        ("flare", SchemeValue::Number(0.25)),
        ("css", SchemeValue::Text(".custom { color: red; }".into())),
        ("launcher-width", SchemeValue::Integer(700)),
        ("launcher-height", SchemeValue::Integer(650)),
        ("launcher-icon-size", SchemeValue::Integer(32)),
        ("launcher-font-scale", SchemeValue::Integer(90)),
        ("overflow-expanded?", SchemeValue::Bool(true)),
        (
            "modules-left",
            SchemeValue::List(vec![module(
                "start",
                vec![("icon", SchemeValue::Text("\u{f303}".into()))],
            )]),
        ),
        (
            "modules-center",
            SchemeValue::List(vec![module(
                "clock",
                vec![("format", SchemeValue::Text("%H:%M".into()))],
            )]),
        ),
        (
            "modules-right",
            SchemeValue::List(vec![module(
                "cpu",
                vec![("interval", SchemeValue::Integer(2))],
            )]),
        ),
        ("overflow", SchemeValue::List(vec![module("tray", vec![])])),
    ]);

    let bar = BarSettings::from_alist(&values);

    assert_eq!(bar.name, "dock");
    assert_eq!(bar.theme.as_deref(), Some("gloss-2007"));
    assert_eq!(bar.edge, Edge::Bottom);
    assert_eq!(bar.layer, Layer::Overlay);
    assert_eq!(bar.monitor, "all");
    assert!(bar.monitors.is_empty());
    assert_eq!(bar.when, When::Multi);
    assert_eq!(bar.height, 40);
    assert_eq!(bar.margin_top, 1);
    assert_eq!(bar.margin_right, 2);
    assert_eq!(bar.margin_bottom, 3);
    assert_eq!(bar.margin_left, 4);
    assert!(!bar.exclusive);
    assert_eq!(bar.spacing, 8);
    assert_eq!(bar.padding, 16);
    assert_eq!(bar.font.as_deref(), Some("Inter"));
    assert_eq!(bar.font_size.as_deref(), Some("11pt"));
    assert_eq!(bar.icon_font.as_deref(), Some("Icons"));
    assert_eq!(bar.radius, 14);
    assert_eq!(bar.opacity, 0.5);
    assert_eq!(bar.blur, 64);
    assert_eq!(bar.shine, 1.0);
    assert_eq!(bar.reflection, 0.0);
    assert_eq!(bar.flare, 0.25);
    assert_eq!(bar.launcher_width, 700);
    assert_eq!(bar.launcher_height, 650);
    assert_eq!(bar.launcher_icon_size, 32);
    assert_eq!(bar.launcher_font_scale, 90);
    assert!(bar.overflow_expanded);
    assert_eq!(bar.left[0].kind, "start");
    assert_eq!(bar.left[0].text("icon").as_deref(), Some("\u{f303}"));
    assert_eq!(bar.center[0].kind, "clock");
    assert_eq!(bar.clock_format().as_deref(), Some("%H:%M"));
    assert_eq!(bar.right[0].integer("interval"), Some(2));
    assert_eq!(bar.overflow[0].kind, "tray");
}

#[test]
fn an_explicit_monitor_list_wins_and_all_clears_it() {
    let values = alist(vec![
        ("monitor", SchemeValue::Text("DP-1".into())),
        (
            "monitors",
            SchemeValue::List(vec![
                SchemeValue::Text("DP-1".into()),
                SchemeValue::Text("HDMI-A-1".into()),
            ]),
        ),
    ]);

    let bar = BarSettings::from_alist(&values);

    assert_eq!(bar.monitor, "DP-1");
    assert_eq!(bar.monitors, vec!["DP-1", "HDMI-A-1"]);

    let values = alist(vec![
        ("monitor", SchemeValue::Text("DP-1".into())),
        (
            "monitors",
            SchemeValue::List(vec![
                SchemeValue::Text("HDMI-A-1".into()),
                SchemeValue::Text("all".into()),
            ]),
        ),
    ]);

    let bar = BarSettings::from_alist(&values);

    assert_eq!(bar.monitor, "all");
    assert!(bar.monitors.is_empty());
}

#[test]
fn unknown_names_fall_back_like_the_c_bridge() {
    let values = alist(vec![
        ("edge", SchemeValue::Symbol("diagonal".into())),
        ("layer", SchemeValue::Symbol("somewhere".into())),
        ("when", SchemeValue::Symbol("sometimes".into())),
        ("name", SchemeValue::Text(String::new())),
    ]);

    let bar = BarSettings::from_alist(&values);

    assert_eq!(bar.name, "main");
    assert_eq!(bar.edge, Edge::Top);
    assert_eq!(bar.layer, Layer::Top);
    assert_eq!(bar.when, When::Always);
}

#[test]
fn empty_modules_and_empty_options_are_omitted() {
    let values = alist(vec![(
        "modules-right",
        SchemeValue::List(vec![
            SchemeValue::Alist(alist(vec![
                ("kind", SchemeValue::Symbol(String::new())),
                ("options", SchemeValue::Alist(alist(vec![]))),
            ])),
            module(
                "clock",
                vec![
                    ("format", SchemeValue::Text("%H".into())),
                    ("empty", SchemeValue::Text(String::new())),
                ],
            ),
        ]),
    )]);

    let bar = BarSettings::from_alist(&values);

    assert_eq!(bar.right.len(), 1);
    assert_eq!(bar.right[0].kind, "clock");
    assert_eq!(bar.right[0].text("empty"), None);
}

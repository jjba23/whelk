// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The finish is one setting read once and painted by every surface.  These
// tests pin the reading (clamps, the unconfigured default) and the two halves
// of the paint: the glass spare a tint, and the same glass on each skin.

use whelk_rs::bar::BarSettings;
use whelk_rs::frost::{self, Frost};
use whelk_rs::settings::SchemeValue;
use whelk_rs::style;

fn alist(entries: &[(&str, SchemeValue)]) -> Vec<(String, SchemeValue)> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[test]
fn an_unconfigured_finish_stays_off() {
    let frost = Frost::from_alist(&[]);

    assert!(
        !frost.enabled,
        "a settings file that never mentions frost gets none"
    );
    assert!(frost.layers().is_empty());
    assert!(frost.background().is_none());
    assert!(frost.highlight().is_none());
    assert!(frost.drop_shadow().is_none());
    assert!(frost.border().is_none());
}

#[test]
fn the_numbers_are_read_and_clamped() {
    let frost = Frost::from_alist(&alist(&[
        ("enabled?", SchemeValue::Bool(true)),
        ("tint", SchemeValue::Text("#1a1410".to_owned())),
        ("opacity", SchemeValue::Number(0.55)),
        ("warmth", SchemeValue::Integer(7)),
        ("grain", SchemeValue::Number(-2.0)),
        ("highlight", SchemeValue::Number(0.4)),
        ("shadow", SchemeValue::Number(0.9)),
    ]));

    assert!(frost.enabled);
    assert_eq!(frost.tint, "#1a1410");
    assert!((frost.opacity - 0.55).abs() < f64::EPSILON);
    assert_eq!(frost.warmth, 1.0, "warmth above one is clamped");
    assert_eq!(frost.grain, 0.0, "grain below zero is clamped");
    assert!((frost.highlight - 0.4).abs() < f64::EPSILON);
    assert!((frost.shadow - 0.9).abs() < f64::EPSILON);
}

#[test]
fn the_glass_adds_two_warm_gradients_and_a_grain() {
    let frost = Frost {
        enabled: true,
        ..Frost::default()
    };
    let layers = frost.layers();

    assert_eq!(layers.len(), 3);
    assert!(layers[0].starts_with("radial-gradient("));
    assert!(layers[1].starts_with("radial-gradient("));
    assert!(layers[2].starts_with("repeating-linear-gradient("));
}

#[test]
fn a_tint_sets_the_background_and_an_empty_one_keeps_the_surface_colour() {
    let tinted = Frost {
        enabled: true,
        tint: "#1a1410".to_owned(),
        opacity: 0.5,
        ..Frost::default()
    };
    let plain = Frost {
        enabled: true,
        ..Frost::default()
    };

    assert_eq!(
        tinted.background().as_deref(),
        Some("alpha(#1a1410, 0.500)")
    );
    assert!(
        plain.background().is_none(),
        "no tint means the bar's own colour survives"
    );
}

#[test]
fn a_disabled_finish_paints_nothing_into_the_skin() {
    frost::install(Frost::default());

    let css = style::css("whelk-main", &BarSettings::default(), "whelk-bar", None);

    assert!(!css.contains("radial-gradient"));
    assert!(!css.contains("repeating-linear-gradient"));
}

#[test]
fn an_enabled_finish_reaches_every_skin() {
    frost::install(Frost {
        enabled: true,
        ..Frost::default()
    });

    for class in [
        "whelk-bar",
        "whelk-launcher",
        "whelk-panel",
        "whelk-widget-card",
        "whelk-osd",
    ] {
        let css = style::css("whelk-main", &BarSettings::default(), class, None);

        assert!(css.contains("radial-gradient"), "{class} is not frosted");
        assert!(
            css.contains("repeating-linear-gradient"),
            "{class} has no grain"
        );
        assert!(
            css.contains("inset 0 1px rgba(255, 244, 230"),
            "{class} has no top highlight"
        );
    }

    frost::install(Frost::default());
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use whelk_rs::bar::BarSettings;
use whelk_rs::style;

fn near_color(css: &str, expected: &str) {
    assert!(css.contains(expected), "missing {expected:?} in:\n{css}");
}

#[test]
fn widget_ids_are_css_safe() {
    assert_eq!(style::widget_id("main"), "whelk-main");
    assert_eq!(style::widget_id(""), "whelk-surface");
    assert_eq!(style::widget_id("gpu: 1/bar"), "whelk-gpu--1-bar");
}

#[test]
fn solid_colours_force_opacity_and_keep_invalid_values() {
    assert_eq!(style::solid(None), None);
    assert_eq!(style::solid(Some("")), None);
    assert_eq!(
        style::solid(Some("#ffffff")).as_deref(),
        Some("rgb(255, 255, 255)")
    );
    // The C helper multiplies by 1.0; it normalises the notation but does not
    // invent an alpha the source colour did not have.
    assert_eq!(
        style::solid(Some("rgba(0, 0, 0, 0.5)")).as_deref(),
        Some("rgba(0, 0, 0, 0.500)")
    );
    assert_eq!(
        style::solid(Some("not-a-colour")).as_deref(),
        Some("not-a-colour")
    );
}

#[test]
fn the_generated_material_matches_the_c_layout() {
    let bar = BarSettings {
        foreground: Some("#f4f4f6".into()),
        background: Some("rgba(26, 26, 28, 0.8)".into()),
        accent: Some("#3b9eff".into()),
        border_color: Some("#000000".into()),
        font: Some("Inter".into()),
        font_size: Some("11pt".into()),
        icon_font: Some("Icons".into()),
        radius: 10,
        opacity: 0.5,
        blur: 4,
        shine: 0.5,
        reflection: 0.2,
        flare: 0.1,
        padding: 12,
        css: Some(".custom { color: red; }".into()),
        ..BarSettings::default()
    };

    let css = style::css("whelk-main", &bar, "whelk-bar", bar.css.as_deref());

    near_color(&css, "window#whelk-main .whelk-bar {");
    near_color(&css, "color: #f4f4f6;");
    near_color(&css, "background-color: rgba(26, 26, 28, 0.400);");
    near_color(&css, "background-image:");
    near_color(&css, "radial-gradient(ellipse at 50% 0%");
    near_color(&css, "linear-gradient(to bottom");
    near_color(&css, "linear-gradient(to top");
    near_color(
        &css,
        "box-shadow: inset 0 1px rgba(255, 255, 255, 0.170), 0 4px 4px rgba(0, 0, 0, 0.30);",
    );
    near_color(&css, "border: 1px solid #000000;");
    near_color(&css, "border-radius: 10px;");
    near_color(&css, "font-family: \"Inter\";");
    near_color(&css, "font-size: 11pt;");
    near_color(&css, "window#whelk-main .whelk-bar { padding: 0 12px; }");
    near_color(
        &css,
        "window#whelk-main .whelk-icon {\n  font-family: \"Icons\";\n}",
    );
    near_color(
        &css,
        "window#whelk-main .whelk-workspace.active { color: #3b9eff; }",
    );
    near_color(
        &css,
        "button.accent { background-color: rgba(26, 26, 28, 0.8);",
    );
    near_color(
        &css,
        "/* The configuration's own rules */\n.custom { color: red; }",
    );
}

#[test]
fn an_empty_material_has_no_invented_rules() {
    let css = style::css("whelk-main", &BarSettings::default(), "whelk-bar", None);

    near_color(&css, "window#whelk-main .whelk-bar {");
    near_color(&css, "border-radius: 0px;");
    assert!(!css.contains("background-color:"));
    assert!(!css.contains("background-image:"));
    assert!(!css.contains("box-shadow:"));
    assert!(!css.contains("font-family:"));
}

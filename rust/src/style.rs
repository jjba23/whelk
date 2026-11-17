// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The generated surface skin, ported from `src/style.c`.
//!
//! The structural CSS stays structural; this module turns one bar's palette
//! and material options into the rules for that bar's window id.  It has no GTK
//! widgets in it, so the output can be asserted without a display.

use std::fmt::Write;

use gtk::gdk::RGBA;
use gtk4 as gtk;

use crate::bar::BarSettings;
use crate::frost;

/// The CSS id for a named surface.  Every character GTK might treat specially
/// becomes a dash, exactly as `whelk_style_widget_id` does.
pub fn widget_id(name: &str) -> String {
    let source = if name.is_empty() { "surface" } else { name };
    let mut id = String::from("whelk-");

    for character in source.chars() {
        id.push(
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            },
        );
    }

    id
}

/// A colour normalised through GTK's parser, or `None` when there is no
/// colour.  This mirrors `whelk_style_solid`: it keeps the source alpha.
pub fn solid(color: Option<&str>) -> Option<String> {
    color
        .filter(|value| !value.is_empty())
        .map(|value| color_with_alpha(Some(value), 1.0))
}

/// Generate one bar's skin, scoped to `widget_id`.
pub fn css(
    widget_id: &str,
    bar: &BarSettings,
    root_class: &str,
    extra_css: Option<&str>,
) -> String {
    let root = format!("window#{widget_id} .{root_class}");
    let mut output = String::new();

    append_material(&mut output, bar, &root);

    if root_class == "whelk-bar" {
        let _ = writeln!(output, "{root} {{ padding: 0 {}px; }}", bar.padding);
    }

    if let Some(icon_font) = bar.icon_font.as_deref().filter(|value| !value.is_empty()) {
        let _ = writeln!(
            output,
            "window#{widget_id} .whelk-icon {{\n  font-family: \"{icon_font}\";\n}}"
        );
    }

    if let Some(accent) = bar.accent.as_deref().filter(|value| !value.is_empty()) {
        let button_background = bar.background.as_deref().unwrap_or("transparent");

        let _ = write!(
            output,
            "window#{widget_id} .whelk-workspace.active {{ color: {accent}; }}\n\
             window#{widget_id} .whelk-battery.charging {{ color: {accent}; }}\n\
             window#{widget_id} .whelk-accent {{ color: {accent}; }}\n\
             window#{widget_id} button.accent {{ background-color: {button_background}; color: {accent}; }}\n"
        );
    }

    if let Some(extra_css) = extra_css.filter(|value| !value.is_empty()) {
        output.push_str("/* The configuration's own rules */\n");
        output.push_str(extra_css);
        output.push('\n');
    }

    output
}

fn append_material(output: &mut String, bar: &BarSettings, selector: &str) {
    let frost = frost::current();
    let frosted_background = frost.background();
    let has_frosted_background = frosted_background.is_some();
    let configured_background = bar
        .background
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let background = frosted_background
        .unwrap_or_else(|| color_with_alpha(bar.background.as_deref(), bar.opacity));

    let _ = writeln!(output, "{selector} {{");

    if let Some(foreground) = bar.foreground.as_deref().filter(|value| !value.is_empty()) {
        let _ = writeln!(output, "  color: {foreground};");
    }

    if configured_background || has_frosted_background {
        let _ = writeln!(output, "  background-color: {background};");
    }

    append_effects(output, bar, &frost);

    let border = bar
        .border_color
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|border| format!("1px solid {border}"))
        .or_else(|| frost.border());

    if let Some(border) = border {
        let _ = writeln!(output, "  border: {border};");
    }

    let _ = writeln!(output, "  border-radius: {}px;", bar.radius);

    if let Some(font) = bar.font.as_deref().filter(|value| !value.is_empty()) {
        let _ = writeln!(output, "  font-family: \"{font}\";");
    }

    if let Some(font_size) = bar.font_size.as_deref().filter(|value| !value.is_empty()) {
        let _ = writeln!(output, "  font-size: {font_size};");
    }

    append_shadow(output, bar, &frost);
    output.push_str("}\n");
}

fn append_effects(output: &mut String, bar: &BarSettings, frost: &frost::Frost) {
    let accent = bar.accent.as_deref().filter(|value| !value.is_empty());
    let mut layers = Vec::new();

    if bar.flare > 0.0 {
        let start = color_with_alpha(accent, bar.flare * 0.30);
        let end = color_with_alpha(accent, 0.0);
        layers.push(format!(
            "    radial-gradient(ellipse at 50% 0%, {start} 0%, {end} 48%)"
        ));
    }

    if bar.shine > 0.0 {
        let start = color_with_alpha(Some("#ffffff"), bar.shine * 0.18);
        let end = color_with_alpha(Some("#ffffff"), 0.0);
        layers.push(format!(
            "    linear-gradient(to bottom, {start} 0%, {end} 36%)"
        ));
    }

    if bar.reflection > 0.0 {
        let start = color_with_alpha(Some("#ffffff"), bar.reflection * 0.10);
        let end = color_with_alpha(Some("#ffffff"), 0.0);
        layers.push(format!(
            "    linear-gradient(to top, {start} 0%, {end} 32%)"
        ));
    }

    // The finish goes under the configured effects: a bar's own shine or flare
    // was asked for explicitly and keeps painting on top of the glass.
    layers.extend(frost.layers());

    if !layers.is_empty() {
        let _ = writeln!(output, "  background-image:\n{};", layers.join(",\n"));
    }
}

fn append_shadow(output: &mut String, bar: &BarSettings, frost: &frost::Frost) {
    let mut parts = Vec::new();

    if bar.shine > 0.0 {
        parts.push(format!(
            "inset 0 1px rgba(255, 255, 255, {:.3})",
            (bar.shine * 0.34).clamp(0.0, 1.0)
        ));
    } else if let Some(highlight) = frost.highlight() {
        // A configured shine already draws the top edge; do not paint it twice.
        parts.push(highlight);
    }

    if bar.blur > 0 {
        parts.push(format!("0 4px {}px rgba(0, 0, 0, 0.30)", bar.blur));
    }

    if let Some(shadow) = frost.drop_shadow() {
        parts.push(shadow);
    }

    if parts.is_empty() {
        return;
    }

    let _ = writeln!(output, "  box-shadow: {};", parts.join(", "));
}

fn color_with_alpha(color: Option<&str>, scale: f64) -> String {
    let Some(color) = color.filter(|value| !value.is_empty()) else {
        return "transparent".to_owned();
    };

    let Ok(rgba) = RGBA::parse(color) else {
        return color.to_owned();
    };

    let alpha = (f64::from(rgba.alpha()) * scale.clamp(0.0, 1.0)).clamp(0.0, 1.0) as f32;
    let rgba = rgba.with_alpha(alpha);

    css_color(rgba)
}

fn css_color(rgba: RGBA) -> String {
    let red = (rgba.red() * 255.0).round() as u8;
    let green = (rgba.green() * 255.0).round() as u8;
    let blue = (rgba.blue() * 255.0).round() as u8;

    if rgba.alpha() >= 0.9995 {
        format!("rgb({red}, {green}, {blue})")
    } else {
        format!("rgba({red}, {green}, {blue}, {:.3})", rgba.alpha())
    }
}

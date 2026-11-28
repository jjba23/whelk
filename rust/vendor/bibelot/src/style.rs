// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The generated card skin, ported from whelk's `src/style.c` rules.
//!
//! The host shell supplies a [`Skin`](crate::skin::Skin): the palette, the
//! material options and the frosted-glass finish in force.  This module turns
//! that into one scoped stylesheet and has no GTK widgets in it, so the output
//! can be asserted without a display.

use std::fmt::Write;

use gtk::gdk::RGBA;
use gtk4 as gtk;

use crate::frost;
use crate::skin::Skin;

pub const SURFACE_ALPHA: f32 = 0.94;

/// The CSS id for a named card.  Every character GTK might treat specially
/// becomes a dash, exactly as whelk's surfaces do.
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

/// Generate one card's skin, scoped to `widget_id`.
pub fn css(widget_id: &str, skin: &Skin, root_class: &str, extra_css: Option<&str>) -> String {
    let root = format!("window#{widget_id} .{root_class}");
    let mut output = String::new();

    append_material(&mut output, skin, &root, root_class);

    if let Some(extra_css) = extra_css.filter(|value| !value.is_empty()) {
        output.push_str("/* The configuration's own rules */\n");
        output.push_str(extra_css);
        output.push('\n');
    }

    output
}

fn append_material(output: &mut String, bar: &Skin, selector: &str, root_class: &str) {
    let frost = &bar.frost;
    let frosted_background = frost.background();
    let has_frosted_background = frosted_background.is_some();
    let configured_background = bar
        .background
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let background = frosted_background
        .unwrap_or_else(|| color_with_alpha(bar.background.as_deref(), bar.opacity));
    let background = if floating(root_class) {
        alpha_at_least(&background, SURFACE_ALPHA)
    } else {
        background
    };

    let _ = writeln!(output, "{selector} {{");

    if let Some(foreground) = bar.foreground.as_deref().filter(|value| !value.is_empty()) {
        let _ = writeln!(output, "  color: {foreground};");
    }

    if configured_background || has_frosted_background {
        let _ = writeln!(output, "  background-color: {background};");
    }

    append_effects(output, bar, frost);

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

    append_shadow(output, bar, frost);
    output.push_str("}\n");
}

fn append_effects(output: &mut String, bar: &Skin, frost: &frost::Frost) {
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

fn append_shadow(output: &mut String, bar: &Skin, frost: &frost::Frost) {
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

/// Whether a generated skin belongs to a sheet of content rather than to the
/// bar.  A floating sheet is read *over* the desktop, so its backing has to be
/// more solid than the bar's glass.
fn floating(root_class: &str) -> bool {
    matches!(
        root_class,
        "whelk-panel"
            | "whelk-toast-stack"
            | "whelk-launcher"
            | "whelk-wallpaper-picker"
            | "whelk-wallpaper-menu"
            | "settings-page"
    )
}

/// Raise a CSS colour's alpha to `floor`, leaving the colour and everything
/// else about it alone.  Handles both GTK's own `alpha(colour, a)` notation
/// (which a frost tint is written in) and `rgba()`.  A transparent colour is a
fn alpha_at_least(color: &str, floor: f32) -> String {
    let floor = floor.clamp(0.0, 1.0);

    if let Some(inner) = color
        .trim()
        .strip_prefix("alpha(")
        .and_then(|value| value.strip_suffix(')'))
        && let Some((tint, alpha)) = inner.rsplit_once(',')
        && let Ok(alpha) = alpha.trim().parse::<f32>()
        && let Ok(rgba) = RGBA::parse(tint.trim())
    {
        if alpha <= 0.0 {
            return color.to_owned();
        }

        return css_color(rgba.with_alpha(alpha.clamp(0.0, 1.0).max(floor)));
    }

    let Ok(rgba) = RGBA::parse(color) else {
        return color.to_owned();
    };

    if rgba.alpha() <= 0.0 {
        return color.to_owned();
    }

    css_color(rgba.with_alpha(rgba.alpha().max(floor)))
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

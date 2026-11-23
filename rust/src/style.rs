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

/// The least opaque a floating surface may be.
///
/// A bar is a 32px strip the compositor can blur; a menu, panel or launcher is
/// a sheet of content over whatever is behind it, and layer-shell offers no
/// backdrop filter for it.  At the bar's own 0.68 alpha the calendar used to
/// read through the Window menu and the network list showed the wallpaper's
/// stripes as if they were row separators.  Nothing that has to be read is
/// allowed to be more transparent than this.
pub const SURFACE_ALPHA: f32 = 0.94;

/// A colour normalised through GTK's parser, or `None` when there is no
/// colour.  A floating surface's background goes through here, so its alpha is
/// raised to at least [`SURFACE_ALPHA`]; a transparent colour stays
/// transparent, and a value GTK cannot parse is passed through untouched.
pub fn solid(color: Option<&str>) -> Option<String> {
    color
        .filter(|value| !value.is_empty())
        .map(|value| alpha_at_least(value, SURFACE_ALPHA))
}

/// The horizontal padding inside one bar module, from the bar's `spacing`.
///
/// macOS's menu bar is the model: a status item keeps a few points around its
/// content and a little more between two items, and the two numbers are not
/// independent.  Here the gap between modules *is* `spacing` (the row's box
/// spacing) and each module keeps half of it inside its pill, so two readings
/// stand twice that apart.  At the default spacing of 6 a module is inset 3px
/// and two neighbouring ones are 12px from text to text -- the proportion the
/// menu bar uses.
pub fn module_padding(spacing: i32) -> i32 {
    (spacing / 2).clamp(2, 12)
}

/// The gap between an icon and its label inside one module.  A hair tighter
/// than the padding, so a reading reads as one thing rather than two.
pub fn module_inner_spacing(spacing: i32) -> i32 {
    (spacing * 2 / 3).max(2)
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

    append_material(&mut output, bar, &root, root_class);

    if root_class == "whelk-bar" {
        // One set of metrics for every reading: the bar's own padding at the
        // ends, the same inset inside every module, and the row's `spacing`
        // between them.  All three come from the configuration, so a bar that
        // changes its spacing changes the whole rhythm rather than one number
        // of it -- which is what made the gaps between modules look uneven:
        // the inset used to be a constant 6px in the stylesheet while the gap
        // was whatever `#:spacing` said, and the tray, workspace and folded
        // controls each added a little of their own on top.
        let pad = module_padding(bar.spacing);

        let _ = writeln!(output, "{root} {{ padding: 0 {}px; }}", bar.padding);
        let _ = writeln!(
            output,
            "window#{widget_id} .whelk-module,\n\
             window#{widget_id} .whelk-overflow-toggle,\n\
             window#{widget_id} .whelk-overflow-more {{\n  padding: 0 {pad}px;\n}}"
        );
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

    // The state classes a reading can wear are only half of the contract; the
    // sheet is the other half.  A pack that is running down says so in colour
    // even on a bar with no accent, and a profile that is loud or quiet has ink
    // of its own.  `#:css` comes after this, so a bar that wants its own amber,
    // red or green has the last word.
    let _ = write!(
        output,
        "window#{widget_id} .whelk-battery.warning {{ color: #ffb340; }}\n\
         window#{widget_id} .whelk-battery.critical {{ color: #ff5f56; font-weight: bold; }}\n\
         window#{widget_id} .whelk-battery.full {{ opacity: 0.72; }}\n\
         window#{widget_id} .whelk-power-profile.performance {{ color: #ffb340; }}\n\
         window#{widget_id} .whelk-power-profile.power-saver {{ color: #8fd16a; }}\n"
    );

    if let Some(extra_css) = extra_css.filter(|value| !value.is_empty()) {
        output.push_str("/* The configuration's own rules */\n");
        output.push_str(extra_css);
        output.push('\n');
    }

    output
}
fn append_material(output: &mut String, bar: &BarSettings, selector: &str, root_class: &str) {
    let frost = frost::current();
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
/// deliberate choice and stays transparent.
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

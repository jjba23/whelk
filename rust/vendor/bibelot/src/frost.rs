// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The frosted-glass finish every surface shares.
//!
//! A Wayland client cannot blur what is behind it: layer-shell has no backdrop
//! filter, so the blur itself belongs to the compositor -- niri's
//! `layer-rule { match namespace="^whelk-rust$"; background-effect { blur true } }`,
//! Hyprland's `layerrule = blur, whelk-rust`, SwayFX's
//! `layer_effects "whelk-rust" { blur enable }`.  What whelk owns is the glass:
//! a warm translucent tint, two soft gradients that catch the light like a
//! pane, and a faint grain that keeps a blurred backdrop from banding.  This
//! module holds that finish and the settings it comes from.
//!
//! The finish is installed once from the loaded configuration and read by
//! [`crate::style`], so the bar, the launcher, the widget cards and every panel
//! are frosted by the same rule instead of six near-copies.

/// The finish applied to every surface while [`enabled`](Self::enabled) is on.
#[derive(Debug, Clone, PartialEq)]
pub struct Frost {
    pub enabled: bool,
    /// A CSS colour laid over the surface's own background.  Empty keeps the
    /// surface's configured colour, so the gradients and the grain can be
    /// added without repainting a bar's palette.
    pub tint: String,
    /// The tint's alpha, `0.0..=1.0`.
    pub opacity: f64,
    /// How strongly the warm gradients show, `0.0..=1.0`.
    pub warmth: f64,
    /// The frosted grain, `0.0..=1.0`; it also hides banding in a blurred
    /// backdrop.
    pub grain: f64,
    /// The hairline highlight along the top edge, `0.0..=1.0`.
    pub highlight: f64,
    /// The soft drop shadow under a floating surface, `0.0..=1.0`.
    pub shadow: f64,
}

impl Default for Frost {
    fn default() -> Self {
        Self {
            enabled: false,
            tint: String::new(),
            opacity: 0.62,
            warmth: 0.35,
            grain: 0.06,
            highlight: 0.30,
            shadow: 0.35,
        }
    }
}

impl Frost {
    /// The background the finish asks for, or `None` to keep the surface's own.
    ///
    /// `alpha()` is GTK's own colour function, so a hex, `rgb()` or `rgba()`
    /// tint all work and the configured alpha is scaled rather than replaced.
    pub fn background(&self) -> Option<String> {
        let tint = self.tint.trim();

        if !self.enabled || tint.is_empty() {
            return None;
        }

        Some(format!("alpha({tint}, {:.3})", self.opacity))
    }

    /// The `background-image` layers the finish adds, in paint order.
    pub fn layers(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }

        let mut layers = Vec::new();

        if self.warmth > 0.0 {
            let strong = (self.warmth * 0.20).clamp(0.0, 0.60);
            let soft = (self.warmth * 0.12).clamp(0.0, 0.40);

            // Warm light from the top-left, and its reflection from the
            // bottom-right: the two directions that make a pane read as glass
            // instead of a flat translucent rectangle.
            layers.push(format!(
                "radial-gradient(120% 160% at 8% -20%, rgba(255, 198, 148, {strong:.3}) 0%, rgba(255, 198, 148, 0) 62%)"
            ));
            layers.push(format!(
                "radial-gradient(90% 120% at 96% 120%, rgba(255, 172, 124, {soft:.3}) 0%, rgba(255, 172, 124, 0) 58%)"
            ));
        }

        if self.grain > 0.0 {
            let grain = (self.grain * 0.25).clamp(0.0, 0.15);

            // A hard-stop repeating gradient is a cheap static dither: no image,
            // no shader, and it breaks up the banding a blurred backdrop shows.
            layers.push(format!(
                "repeating-linear-gradient(115deg, rgba(255, 255, 255, {grain:.3}) 0 1px, rgba(255, 255, 255, 0) 1px 3px)"
            ));
        }

        layers
    }

    /// The hairline along the top edge, as one `box-shadow` entry.
    pub fn highlight(&self) -> Option<String> {
        if !self.enabled || self.highlight <= 0.0 {
            return None;
        }

        Some(format!(
            "inset 0 1px rgba(255, 244, 230, {:.3})",
            (self.highlight * 0.40).clamp(0.0, 0.80)
        ))
    }

    /// The soft shadow that lets a floating pane sit above the desktop.
    pub fn drop_shadow(&self) -> Option<String> {
        if !self.enabled || self.shadow <= 0.0 {
            return None;
        }

        Some(format!(
            "0 18px 46px rgba(0, 0, 0, {:.3})",
            (self.shadow * 0.70).clamp(0.0, 0.80)
        ))
    }

    /// The hairline that keeps a frosted edge readable against a light
    /// wallpaper, when the configuration did not name a border of its own.
    pub fn border(&self) -> Option<String> {
        if !self.enabled || self.highlight <= 0.0 {
            return None;
        }

        Some(format!(
            "1px solid rgba(255, 238, 220, {:.3})",
            (self.highlight * 0.10).clamp(0.0, 0.30)
        ))
    }
}

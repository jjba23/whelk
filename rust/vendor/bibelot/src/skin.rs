// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The material a card is drawn with.
//!
//! The host shell owns the palette and the frosted-glass settings; it maps
//! whichever of its own bar records is in force into this small struct.  A card
//! can then override only the keys it named without knowing anything about the
//! shell around it.

use crate::frost::Frost;

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub accent: Option<String>,
    pub border_color: Option<String>,
    pub radius: i32,
    pub opacity: f64,
    pub blur: i32,
    pub shine: f64,
    pub reflection: f64,
    pub flare: f64,
    pub font: Option<String>,
    pub font_size: Option<String>,
    pub frost: Frost,
}

impl Default for Skin {
    fn default() -> Self {
        Self {
            foreground: None,
            background: Some("#181b22".to_owned()),
            accent: None,
            border_color: None,
            radius: 12,
            opacity: 0.94,
            blur: 0,
            shine: 0.0,
            reflection: 0.0,
            flare: 0.0,
            font: None,
            font_size: None,
            frost: Frost::default(),
        }
    }
}

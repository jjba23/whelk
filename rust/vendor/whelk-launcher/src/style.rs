// whelk-launcher -- a launcher overlay for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's own configuration and palette.
//!
//! A host maps its bar or theme into [`LauncherStyle`].  Nothing here imports a
//! bar crate: the launcher can run beside any shell, or without one.

/// The four geometry values a settings file writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LauncherSettings {
    pub width: i32,
    pub height: i32,
    pub icon_size: i32,
    pub font_scale: i32,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            width: 660,
            height: 640,
            icon_size: 24,
            font_scale: 100,
        }
    }
}

/// The palette and material a host gives the launcher card.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LauncherStyle {
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
    pub icon_font: Option<String>,
    pub css: Option<String>,
    pub settings: LauncherSettings,
}

impl LauncherStyle {
    pub fn with_settings(mut self, settings: LauncherSettings) -> Self {
        self.settings = settings;

        self
    }
}

/// The CSS id for a named surface.
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

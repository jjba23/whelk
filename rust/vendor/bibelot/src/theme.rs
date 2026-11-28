// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The cards' structural look, as Scheme hands it over.
//!
//! The defaults here are the same values `scheme/bibelot/theme.scm` starts
//! with.  The palette and material still come from the host's generated skin;
//! this theme owns the card's own rhythm and a final stylesheet for everything
//! the named keys do not cover.

use std::path::PathBuf;

use crate::guile::SchemeValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Space inside every card, in pixels.
    pub card_padding: u32,
    /// The vertical gap between a card's header, body and footer.
    pub card_gap: i32,
    /// Rules appended after the structural card stylesheet, so a theme can
    /// restyle any widget class without recompiling.
    pub extra_css: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            card_padding: 14,
            card_gap: 9,
            extra_css: String::new(),
        }
    }
}

impl Theme {
    /// Apply the alist `(widget-theme)` returned.  Unknown keys are ignored so
    /// a newer Scheme module can carry keys an older Rust side does not know.
    pub fn from_alist(entries: &[(String, SchemeValue)]) -> Self {
        let mut theme = Self::default();

        for (key, value) in entries {
            match (key.as_str(), value) {
                ("card-padding", SchemeValue::Integer(value)) => {
                    theme.card_padding = pixels(*value, 0, 96);
                }
                ("card-gap", SchemeValue::Integer(value)) => {
                    theme.card_gap = pixels(*value, 0, 96) as i32;
                }
                ("extra-css", SchemeValue::Text(value)) => theme.extra_css = value.clone(),
                _ => {}
            }
        }

        theme
    }

    /// The rules layered over the structural card stylesheet.
    pub fn css(&self) -> String {
        format!(
            ".whelk-widget-card {{\n    padding: {}px;\n}}\n{}",
            self.card_padding, self.extra_css
        )
    }
}

fn pixels(value: i64, minimum: u32, maximum: u32) -> u32 {
    value.clamp(i64::from(minimum), i64::from(maximum)) as u32
}

/// The directories `(bibelot theme)` may live in, most specific first.
///
/// Only paths the user or the installer chose.  A relative directory and an
/// arbitrary build directory are attacker-writable in packaged builds; the
/// source-tree candidate is compiled in for debug builds only.
pub fn scheme_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(directory) = std::env::var_os("BIBELOT_SCHEME_DIR") {
        candidates.push(PathBuf::from(directory));
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(bin) = executable.parent()
        && let Some(prefix) = bin.parent()
    {
        candidates.push(prefix.join("share/guile/site/3.0"));
    }

    #[cfg(debug_assertions)]
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scheme"));

    candidates.push(PathBuf::from("/usr/share/guile/site/3.0"));

    let mut paths = Vec::new();

    for candidate in candidates {
        let directory = std::fs::canonicalize(&candidate).unwrap_or(candidate);

        if directory.is_dir() && !paths.contains(&directory) {
            paths.push(directory);
        }
    }

    paths
}

/// The user's override file: `BIBELOT_THEME` when set, otherwise
/// `$XDG_CONFIG_HOME/bibelot/theme.scm` (or `~/.config/bibelot/theme.scm`).
///
/// The path is returned even when the file does not exist if the environment
/// names it explicitly, so a host can report the mistake.  The default path is
/// only returned when it is a file.
pub fn user_theme_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BIBELOT_THEME") {
        return Some(PathBuf::from(path));
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let path = base.join("bibelot/theme.scm");

    path.is_file().then_some(path)
}

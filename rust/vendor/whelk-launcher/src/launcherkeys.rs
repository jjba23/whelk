// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The launcher's own key map, ported from `util/launcherkeys.c`.
//!
//! whelk does not own the session's keyboard: a compositor binding opens the
//! launcher.  Once it is open, the launcher reads one key of its own -- the
//! prefix that chooses a mode, `@` for windows, `>` for a command, and so on --
//! and that map is whelk's to change.

/// The modes in the order the settings window lists them.
pub const MODES: [&str; 10] = [
    "app",
    "window",
    "run",
    "calc",
    "file",
    "web",
    "ssh",
    "quit",
    "clipboard",
    "menuitem",
];

const DEFAULT_PREFIXES: [&str; 10] = ["", "@", ">", "=", "/", "?", "%", "!", ";", ":"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherKeys {
    default_mode: String,
    prefixes: Vec<String>,
}

impl Default for LauncherKeys {
    fn default() -> Self {
        Self::new()
    }
}

impl LauncherKeys {
    /// The built-in map: the prefixes above and `app` as the mode `whelk
    /// --launcher` opens with.
    pub fn new() -> Self {
        Self {
            default_mode: "app".to_owned(),
            prefixes: DEFAULT_PREFIXES
                .iter()
                .map(|prefix| (*prefix).to_owned())
                .collect(),
        }
    }

    /// Whether `mode` is one of the launcher's modes.  An unknown mode in the
    /// override file is ignored rather than opening an empty overlay.
    pub fn is_mode(mode: &str) -> bool {
        MODES.contains(&mode)
    }

    pub fn mode_count() -> usize {
        MODES.len()
    }

    pub fn mode_at(index: usize) -> Option<&'static str> {
        MODES.get(index).copied()
    }

    /// Sets the mode `whelk --launcher` opens with; an unknown mode is ignored
    /// and the current one is kept.
    pub fn set_default_mode(&mut self, mode: &str) {
        if Self::is_mode(mode) {
            self.default_mode = mode.to_owned();
        }
    }

    pub fn default_mode(&self) -> &str {
        if self.default_mode.is_empty() {
            "app"
        } else {
            &self.default_mode
        }
    }

    /// Replaces the prefix for `mode`; an empty string disables it.  A prefix
    /// may be longer than one character, and the longer match wins.
    pub fn set_prefix(&mut self, mode: &str, prefix: &str) {
        let Some(index) = MODES.iter().position(|candidate| *candidate == mode) else {
            return;
        };

        self.prefixes[index] = prefix.to_owned();
    }

    /// The configured prefix, or the built-in one when this map has none (and
    /// an empty string for an unknown mode).
    pub fn prefix(&self, mode: &str) -> &str {
        let Some(index) = MODES.iter().position(|candidate| *candidate == mode) else {
            return "";
        };

        self.prefixes
            .get(index)
            .map(String::as_str)
            .unwrap_or(DEFAULT_PREFIXES[index])
    }

    /// The mode a prefix selects, with the text after the prefix.  An empty
    /// prefix is a disabled one: `app` has no key of its own.
    pub fn mode_for_text<'a>(&self, text: &'a str) -> Option<(&'static str, &'a str)> {
        let mut best: Option<(&'static str, usize)> = None;

        for mode in MODES {
            let prefix = self.prefix(mode);

            if prefix.is_empty() {
                continue;
            }

            let length = prefix.len();

            if best.is_some_and(|(_, best_length)| length <= best_length)
                || !text.starts_with(prefix)
            {
                continue;
            }

            best = Some((mode, length));
        }

        best.map(|(mode, length)| (mode, &text[length..]))
    }
}

// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Text helpers, ported from `util/format.c`.
//!
//! A module renders a configuration string such as `"{icon} {usage}%"` by
//! substituting named values.  Plain substitution, escaping and truncation live
//! here because every module needs exactly these three and no module needs SCM.

use gtk::glib;
use gtk4 as gtk;

/// Replaces every `{key}` in `text`, or returns a copy when there is none.
pub fn subst(text: &str, key: &str, value: &str) -> String {
    if key.is_empty() {
        return text.to_owned();
    }

    text.replace(&format!("{{{key}}}"), value)
}

/// Escapes `text` for Pango markup (`&` becomes `&amp;`).
pub fn markup_escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

/// `icon` as a markup span naming `font_family`, so the glyph is drawn from
/// the face the configuration meant instead of whichever face fontconfig
/// happens to answer at the bar's weight.
pub fn icon_markup(icon: &str, font_family: Option<&str>) -> String {
    if icon.is_empty() {
        return String::new();
    }

    let escaped_icon = markup_escape(icon);

    match font_family.filter(|family| !family.is_empty()) {
        Some(family) => format!(
            "<span font_family=\"{}\">{}</span>",
            markup_escape(family),
            escaped_icon
        ),
        None => escaped_icon,
    }
}

/// Shortens `text` on a character boundary and ends it with an ellipsis when
/// it had to be.  A non-positive `max_chars` means no limit.
pub fn truncate_utf8(text: &str, max_chars: i32) -> String {
    if max_chars <= 0 {
        return text.to_owned();
    }

    let max_chars = max_chars as usize;

    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let head: String = text.chars().take(max_chars).collect();

    format!("{}…", head.trim_end())
}

/// The first line of `text` without its line ending.  A tray or a CLI that
/// answers in paragraphs becomes a bar reading.
pub fn first_line(text: &str) -> &str {
    strip_newline(text.split('\n').next().unwrap_or(""))
}

/// Removes trailing line endings (the C helper strips every trailing `\n` or
/// `\r`, not just one pair).
pub fn strip_newline(text: &str) -> &str {
    text.trim_end_matches(['\r', '\n'])
}

/// `kibibytes` as a human-readable binary size, e.g. `"9.4 GiB"`.
pub fn str_gib(kibibytes: u64) -> String {
    format!("{:.1} GiB", kibibytes as f64 / (1024.0 * 1024.0))
}

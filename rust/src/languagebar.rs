// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The active keyboard layout, ported from `modules/language.c`.
//!
//! niri's plain-text listing marks the active layout with an asterisk and gives
//! it by display name.  `#:codes` names the XKB codes in the same order, so a
//! bar can show `US` rather than `English (US)`.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const COMMAND: &str = "niri msg keyboard-layouts";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageReading {
    pub text: String,
    pub name: String,
}

/// Find the `* 0 English (US)` row in niri's layout listing.
pub fn parse(text: &str) -> Option<Layout> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('*') else {
            continue;
        };
        let rest = rest.trim_start();
        let index_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let index = rest[..index_end].parse().ok()?;
        let name = rest[index_end..].trim().to_owned();

        return Some(Layout { index, name });
    }

    None
}

/// The configured code for the active index, uppercased like the C module.
pub fn code(codes: Option<&str>, index: usize) -> Option<String> {
    let code = codes?.split(',').nth(index)?;

    (!code.is_empty()).then(|| code.to_ascii_uppercase())
}

pub fn reading(text: &str, codes: Option<&str>) -> Option<LanguageReading> {
    let layout = parse(text)?;
    let text = code(codes, layout.index).unwrap_or_else(|| layout.name.clone());

    Some(LanguageReading {
        text,
        name: layout.name,
    })
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let codes = module.text("codes").filter(|value| !value.is_empty());
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(move |host| update(codes.as_deref(), host));

    Some(root)
}

fn update(codes: Option<&str>, host: &ModuleHost) {
    let callback_host = host.clone();
    let codes = codes.map(str::to_owned);

    host.run_command(COMMAND, move |text| {
        let Some(reading) = text
            .as_deref()
            .and_then(|text| reading(text, codes.as_deref()))
        else {
            callback_host.set_text("");

            return;
        };
        let values = [
            ("text", reading.text.as_str()),
            ("name", reading.name.as_str()),
            ("index", ""),
        ];

        callback_host.present("{text}", &values);
    });
}

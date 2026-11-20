// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The active keyboard layout, ported from `modules/language.c`.
//!
//! niri's plain-text listing marks the active layout with an asterisk and gives
//! it by display name.  `#:codes` names the XKB codes in the same order, so a
//! bar can show `US` rather than `English (US)`.
//!
//! The reading is also the way back.  A click asks niri for the next layout,
//! and the right button opens every layout niri reported, one click away from
//! any of them.  Both gestures belong to the module, so a configuration that
//! names `#:on-click` or `#:on-click-right` keeps its own commands instead.

use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::{self, ModuleHost, WeakModuleHost};
use crate::command;
use crate::style;

pub const COMMAND: &str = "niri msg keyboard-layouts";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 3;
const NEXT_COMMAND: &str = "niri msg action switch-layout next";
const DEFAULT_FORMAT: &str = "{text}";
const DEFAULT_TOOLTIP: &str =
    "Keyboard layout: {name}\nClick: next layout · Right-click: all layouts";

/// How long the bar waits before re-reading after a layout change.  niri
/// applies the switch on its own event loop; asking in the same turn would
/// draw the layout the user just left.
const SWITCH_SETTLE: Duration = Duration::from_millis(150);

const MENU_CSS: &str = "
.whelk-language-popover {
  padding: 6px;
}
.whelk-language-list {
  padding: 2px;
}
.whelk-language-row {
  padding: 4px 10px;
  border-radius: 5px;
}
.whelk-language-row.active {
  font-weight: 600;
}
";

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

/// Every layout in niri's listing, in niri's order.  The `Keyboard layouts:`
/// header line has no leading number, so it is not a layout.
pub fn layouts(text: &str) -> Vec<Layout> {
    let mut layouts = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        let line = line
            .strip_prefix('*')
            .map_or(line, |rest| rest.trim_start());
        let Some((index, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(index) = index.parse::<usize>() else {
            continue;
        };
        let name = name.trim();

        if !name.is_empty() {
            layouts.push(Layout {
                index,
                name: name.to_owned(),
            });
        }
    }

    layouts
}

/// The index niri marks with an asterisk, which is the layout in use.
pub fn active_index(text: &str) -> Option<usize> {
    parse(text).map(|layout| layout.index)
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

/// The click that moves to the next layout niri knows.
pub fn next_command() -> &'static str {
    NEXT_COMMAND
}

/// The click that switches to one layout by the index niri printed for it.
/// Zero means "no layout in use", which a row never offers.
pub fn switch_command(index: usize) -> String {
    format!("niri msg action switch-layout {index}")
}

/// How one layout is written in the list: `#:codes` when the configuration
/// named them, the display name otherwise.
fn layout_label(layout: &Layout, codes: Option<&str>) -> String {
    code(codes, layout.index).unwrap_or_else(|| layout.name.clone())
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

    if host.settings().actions.primary.is_none() {
        attach_switch(&root, &host);
    }

    if host.settings().actions.secondary.is_none() {
        let codes = module.text("codes").filter(|value| !value.is_empty());

        attach_picker(&root, &host, &context.bar, codes);
    }

    Some(root)
}

/// A click asks niri for the next layout and re-reads once niri has applied
/// it.  Nothing is painted optimistically: the reading stays the compositor's.
fn attach_switch(root: &gtk::Box, host: &ModuleHost) {
    let gesture = gtk::GestureClick::new();
    let host = host.downgrade();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |_, _, _, _| {
        command::spawn(next_command());

        if let Some(host) = host.upgrade() {
            refresh_later(&host);
        }
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

/// The list of every layout, opened by the right button on the bar's own
/// monitor.  The rows are read fresh each time, so a layout added to the niri
/// configuration appears without a shell reload.
fn attach_picker(root: &gtk::Box, host: &ModuleHost, bar: &BarSettings, codes: Option<String>) {
    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    popover.set_has_arrow(false);
    popover.add_css_class("whelk-language-popover");
    menu.add_css_class("whelk-language-list");
    popover.set_child(Some(&menu));
    popover.set_parent(root);
    barmodule::watch_popover(&popover);
    attach_escape(&popover);

    let popover_weak = popover.downgrade();

    root.connect_destroy(move |_| {
        if let Some(popover) = popover_weak.upgrade() {
            popover.popdown();
            popover.unparent();
        }
    });

    if let Some(provider) = install_css(bar) {
        root.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    let gesture = gtk::GestureClick::new();
    let host = host.downgrade();
    let popover_clone = popover.clone();
    let menu_clone = menu.clone();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        barmodule::close_popovers(Some(&popover_clone));

        let popover = popover_clone.clone();
        let menu = menu_clone.clone();
        let host = host.clone();
        let codes = codes.clone();

        command::run(COMMAND, move |text| {
            if fill_menu(&menu, &popover, host, codes.as_deref(), text.as_deref()) {
                popover.popup();
            }
        });
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

/// Paint one row per layout.  Returns whether there was anything to show: a
/// bar whose niri answered without a layout stays closed rather than opening
/// an empty card.
fn fill_menu(
    menu: &gtk::Box,
    popover: &gtk::Popover,
    host: WeakModuleHost,
    codes: Option<&str>,
    text: Option<&str>,
) -> bool {
    while let Some(child) = menu.first_child() {
        menu.remove(&child);
    }

    let layouts = text.map(layouts).unwrap_or_default();
    let active = text.and_then(active_index);

    if layouts.is_empty() {
        return false;
    }

    for layout in &layouts {
        let current = active == Some(layout.index);
        let label = layout_label(layout, codes);
        let row = row_button(&label, current);

        if current {
            row.set_tooltip_text(Some("This layout is in use"));
        }

        let command_line = switch_command(layout.index);
        let popover_weak = popover.downgrade();
        let host = host.clone();

        row.connect_clicked(move |_| {
            command::spawn(&command_line);

            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }

            if let Some(host) = host.upgrade() {
                refresh_later(&host);
            }
        });
        menu.append(&row);
    }

    true
}

fn row_button(label: &str, active: bool) -> gtk::Button {
    let text = if active {
        format!("✓ {label}")
    } else {
        format!("   {label}")
    };
    let button = gtk::Button::with_label(&text);

    button.add_css_class("flat");
    button.add_css_class("whelk-language-row");
    button.set_halign(gtk::Align::Fill);

    if active {
        button.add_css_class("active");
    }

    button
}

fn refresh_later(host: &ModuleHost) {
    let host = host.downgrade();

    gtk::glib::timeout_add_local_once(SWITCH_SETTLE, move || {
        if let Some(host) = host.upgrade() {
            host.refresh();
        }
    });
}

fn update(codes: Option<&str>, host: &ModuleHost) {
    let callback_host = host.clone();
    let codes = codes.map(str::to_owned);

    host.run_command(COMMAND, move |text| {
        let Some(reading) = text
            .as_deref()
            .and_then(|text| reading(text, codes.as_deref()))
        else {
            // No niri, no compositor answer: a blank box on the bar says
            // nothing, so the reading steps out of the row entirely.
            callback_host.set_visible(false);
            callback_host.set_text("");

            return;
        };
        let values = [
            ("text", reading.text.as_str()),
            ("name", reading.name.as_str()),
        ];

        callback_host.set_visible(true);
        callback_host.present_default(DEFAULT_FORMAT, "", DEFAULT_TOOLTIP, &values);
    });
}

/// Escape closes the list, so a right click by accident costs a key rather
/// than a pointer trip back to the reading.
fn attach_escape(popover: &gtk::Popover) {
    let keys = gtk::EventControllerKey::new();
    let weak = popover.downgrade();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        if key != gtk::gdk::Key::Escape {
            return gtk::glib::Propagation::Proceed;
        }

        if let Some(popover) = weak.upgrade() {
            popover.popdown();
        }

        gtk::glib::Propagation::Stop
    });
    popover.add_controller(keys);
}

fn install_css(bar: &BarSettings) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let mut css = MENU_CSS.to_owned();

    if let Some(foreground) = bar
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(
            ".whelk-language-popover {{ color: {foreground}; }}\n"
        ));
    }

    if let Some(background) = style::solid(bar.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-language-popover {{ background-color: {background}; }}\n"
        ));
    }

    let provider = gtk::CssProvider::new();

    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(provider)
}

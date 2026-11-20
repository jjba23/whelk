// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The active power-profiles-daemon profile, ported from
//! `modules/power_profile.c`.
//!
//! A machine without the daemon answers nothing and the module hides itself:
//! an indicator for a profile that cannot be chosen is decoration.
//!
//! The reading is also the control.  A click asks the daemon for the next
//! profile and the right button opens every profile `powerprofilesctl list`
//! reports, with the one in use marked, one click from any of them.  Both
//! gestures belong to the module, so a configuration that names `#:on-click` or
//! `#:on-click-right` keeps its own commands.
//!
//! The default glyph is a leaf, not the bolt this file used to wear: the
//! battery module spends the bolt on its charging reading, and two bolts a few
//! pixels apart read as one broken thing.

use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::{self, ModuleHost, WeakModuleHost};
use crate::command;
use crate::format;
use crate::style;

pub const COMMAND: &str = "powerprofilesctl get";
pub const LIST_COMMAND: &str = "powerprofilesctl list";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;

const CYCLE_COMMAND: &str = "powerprofilesctl cycle";
const DEFAULT_ICON: &str = "\u{f06c}";
const DEFAULT_FORMAT: &str = "{icon}";
const DEFAULT_TOOLTIP: &str =
    "Power profile: {profile}\nClick: next profile · Right-click: all profiles";

/// The profiles a daemon older than `powerprofilesctl list` can still be
/// asked for.
const KNOWN_PROFILES: [&str; 3] = ["performance", "balanced", "power-saver"];

/// The daemon applies a change on its own event loop; asking in the same turn
/// would draw the profile the user just left.
const SETTLE: Duration = Duration::from_millis(250);

const MENU_CSS: &str = "
.whelk-profile-popover {
  padding: 6px;
}
.whelk-profile-list {
  padding: 2px;
}
.whelk-profile-row {
  padding: 4px 10px;
  border-radius: 5px;
}
.whelk-profile-row.active {
  font-weight: 600;
}
";

/// The profile `powerprofilesctl get` printed.
pub fn reading(text: &str) -> Option<String> {
    let profile = format::first_line(text).trim();

    (!profile.is_empty()).then(|| profile.to_owned())
}

/// Every profile `powerprofilesctl list` prints, in the daemon's order.
///
/// The listing indents a profile by two spaces and its details by four, and
/// marks the profile in use with an asterisk, so only a short indented word
/// ending in a colon is a profile.
pub fn profiles(text: &str) -> Vec<String> {
    let mut profiles = Vec::new();

    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let rest = line.trim_start();
        let (active, rest) = match rest.strip_prefix('*') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, rest),
        };

        if !active && indent > 2 {
            continue;
        }

        let Some(name) = rest.strip_suffix(':') else {
            continue;
        };
        let name = name.trim();

        if !name.is_empty() && !name.contains(char::is_whitespace) {
            profiles.push(name.to_owned());
        }
    }

    profiles
}

/// The profile `powerprofilesctl list` marks with an asterisk, which is the one
/// in use.  The plain `get` answer has no marker.
pub fn active(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix('*')?.trim_start();
        let name = rest.strip_suffix(':').unwrap_or(rest).trim();

        (!name.is_empty()).then(|| name.to_owned())
    })
}

/// The profiles the menu offers.
///
/// The daemon's own listing wins when it answered: a profile this build has
/// never heard of is still one the machine can be in.  `#:profiles` then trims
/// that list, and a filter matching nothing leaves the daemon's list whole
/// rather than opening an empty menu.  Without a listing, `#:profiles` is the
/// list, and the three names every daemon knows are the fallback.
pub fn offered(listing: Option<&str>, configured: Option<&str>) -> Vec<String> {
    let wanted: Option<Vec<String>> = configured
        .map(|list| {
            list.split(',')
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect::<Vec<String>>()
        })
        .filter(|names| !names.is_empty());
    let from_listing = listing.map(profiles).unwrap_or_default();

    if !from_listing.is_empty() {
        let chosen = match wanted.as_ref() {
            Some(wanted) => from_listing
                .iter()
                .filter(|profile| wanted.contains(&profile.to_ascii_lowercase()))
                .cloned()
                .collect::<Vec<String>>(),
            None => from_listing.clone(),
        };

        return if chosen.is_empty() {
            from_listing
        } else {
            chosen
        };
    }

    match wanted {
        Some(wanted) => wanted,
        None => KNOWN_PROFILES
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
    }
}

/// The click that moves to the next profile the daemon offers.
pub fn cycle_command() -> &'static str {
    CYCLE_COMMAND
}

/// The click that switches to one profile by the name `powerprofilesctl list`
/// printed.
pub fn set_command(profile: &str) -> String {
    format!("powerprofilesctl set {profile}")
}

/// The CSS class a profile wears, so a theme can give the loud one its own
/// ink.  Everything that is not a letter or a digit becomes a dash, the way
/// `style::widget_id` treats a surface name.
pub fn profile_class(profile: &str) -> String {
    let mut class = String::from("profile-");

    for character in profile.chars() {
        class.push(if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '-'
        });
    }

    class
}

/// How a profile is written in the menu: the daemon's own word, in sentence
/// case, because `power-saver` is a command line and "Power saver" is a menu.
fn label(profile: &str) -> String {
    let words = profile.replace('-', " ");
    let mut characters = words.chars();

    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(update);

    if host.settings().actions.primary.is_none() {
        attach_cycle(&root, &host);
    }

    if host.settings().actions.secondary.is_none() {
        let configured = module.text("profiles").filter(|list| !list.is_empty());

        attach_picker(&root, &host, &context.bar, configured);
    }

    Some(root)
}

/// A click asks the daemon for the next profile and re-reads once it has
/// applied it.  Nothing is painted optimistically: the reading stays the
/// daemon's.
fn attach_cycle(root: &gtk::Box, host: &ModuleHost) {
    let gesture = gtk::GestureClick::new();
    let host = host.downgrade();

    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_pressed(move |_, _, _, _| {
        command::spawn(cycle_command());

        if let Some(host) = host.upgrade() {
            refresh_later(&host);
        }
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

/// The list of every profile, opened by the right button.  It is read fresh
/// each time, so a profile the daemon gains appears without a shell reload.
fn attach_picker(
    root: &gtk::Box,
    host: &ModuleHost,
    bar: &BarSettings,
    configured: Option<String>,
) {
    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    popover.set_has_arrow(false);
    popover.add_css_class("whelk-profile-popover");
    menu.add_css_class("whelk-profile-list");
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
        let configured = configured.clone();

        command::run(LIST_COMMAND, move |text| {
            if fill_menu(
                &menu,
                &popover,
                host,
                configured.as_deref(),
                text.as_deref(),
            ) {
                popover.popup();
            }
        });
    });
    root.add_controller(gesture);
    root.set_cursor_from_name(Some("pointer"));
}

/// Paint one row per profile.  Returns whether there was anything to show: a
/// daemon that answered without a profile stays closed rather than opening an
/// empty card.
fn fill_menu(
    menu: &gtk::Box,
    popover: &gtk::Popover,
    host: WeakModuleHost,
    configured: Option<&str>,
    text: Option<&str>,
) -> bool {
    while let Some(child) = menu.first_child() {
        menu.remove(&child);
    }

    let profiles = offered(text, configured);
    let active = text.and_then(active);

    if profiles.is_empty() {
        return false;
    }

    for profile in &profiles {
        let current = active.as_deref() == Some(profile.as_str());
        let row = row_button(&label(profile), current);

        if current {
            row.set_tooltip_text(Some("This profile is in use"));
        }

        let command_line = set_command(profile);
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
    button.add_css_class("whelk-profile-row");
    button.set_halign(gtk::Align::Fill);

    if active {
        button.add_css_class("active");
    }

    button
}

fn refresh_later(host: &ModuleHost) {
    let host = host.downgrade();

    gtk::glib::timeout_add_local_once(SETTLE, move || {
        if let Some(host) = host.upgrade() {
            host.refresh();
        }
    });
}

/// Drop every profile class the previous reading added, so a bar that moves
/// from `power-saver` to `performance` does not wear both.
fn clear_profile_classes(host: &ModuleHost) {
    let Some(root) = host.root() else {
        return;
    };

    for class in root.css_classes() {
        if class.starts_with("profile-") {
            root.remove_css_class(&class);
        }
    }
}

fn update(host: &ModuleHost) {
    let callback_host = host.clone();

    host.run_command(COMMAND, move |text| {
        let Some(profile) = text.as_deref().and_then(reading) else {
            callback_host.set_visible(false);
            callback_host.set_text("");
            clear_profile_classes(&callback_host);

            return;
        };
        let values = [("profile", profile.as_str())];

        clear_profile_classes(&callback_host);
        callback_host.add_class(&profile_class(&profile));
        callback_host.set_visible(true);
        callback_host.present_default(DEFAULT_FORMAT, DEFAULT_ICON, DEFAULT_TOOLTIP, &values);
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
            ".whelk-profile-popover {{ color: {foreground}; }}\n"
        ));
    }

    if let Some(background) = style::solid(bar.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-profile-popover {{ background-color: {background}; }}\n"
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

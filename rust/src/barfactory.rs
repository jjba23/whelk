// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Rust bar's module factory.
//!
//! This is the one table that turns a configuration kind into a widget, the
//! same role the C `factories[]` table plays.  An unknown kind is logged and
//! left out; it does not stop the rest of the bar from being built.
//!
//! A factory receives the settings it needs in [`ModuleContext`], so a module
//! does not reach back into the global configuration.

use gtk4 as gtk;

use crate::audiobar;
use crate::backlightbar;
use crate::bar::{BarSettings, ModuleSpec};
use crate::batterybar;
use crate::bluetoothbar;
use crate::clockbar;
use crate::cpubar;
use crate::custombar;
use crate::dndbar;
use crate::inhibitbar;
use crate::keyboardbar;
use crate::languagebar;
use crate::memorybar;
use crate::notificationsbar;
use crate::powerprofilebar;
use crate::session::Session;
use crate::wifibar;
use crate::windowbar;
use crate::workspacebar;

/// The session and bar facts a module factory may need.  Keep this small: a
/// factory should receive settings, not a second configuration authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleContext {
    pub session: Session,
    pub spacing: i32,
    pub icon_font: Option<String>,
}

impl ModuleContext {
    pub fn from_bar(bar: &BarSettings, session: Session) -> Self {
        Self {
            session,
            spacing: bar.spacing,
            icon_font: bar.icon_font.clone(),
        }
    }
}

pub type ModuleFactory = fn(&ModuleSpec, &ModuleContext) -> Option<gtk::Box>;

const FACTORIES: &[(&str, ModuleFactory)] = &[
    ("audio", audiobar::module),
    ("backlight", backlightbar::module),
    ("battery", batterybar::module),
    ("bluetooth", bluetoothbar::module),
    ("clock", clockbar::module),
    ("cpu", cpubar::module),
    ("custom", custombar::module),
    ("dnd", dndbar::module),
    ("inhibit", inhibitbar::module),
    ("keyboard", keyboardbar::module),
    ("language", languagebar::module),
    ("memory", memorybar::module),
    ("notifications", notificationsbar::module),
    ("power-profile", powerprofilebar::module),
    ("wifi", wifibar::module),
    ("window", windowbar::module),
    ("workspaces", workspacebar::module),
];

/// The constructor for `kind`, without building a widget.  The UI uses this
/// table through [`build`]; tests can ask whether a kind is supported without a
/// display.
pub fn factory(kind: &str) -> Option<ModuleFactory> {
    FACTORIES
        .iter()
        .find_map(|(candidate, factory)| (*candidate == kind).then_some(*factory))
}

pub fn supports(kind: &str) -> bool {
    factory(kind).is_some()
}

/// Build one configured module, or leave it out with a diagnostic.
pub fn build(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let Some(factory) = factory(&module.kind) else {
        eprintln!(
            "whelk-rs: module: unknown kind '{}': it is left out of the bar",
            module.kind
        );

        return None;
    };

    factory(module, context)
}

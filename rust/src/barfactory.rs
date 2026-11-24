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

use std::rc::Rc;

use gtk4 as gtk;

use crate::audiobar;
use crate::backlightbar;
use crate::bar::{BarSettings, ModuleSpec};
use crate::batterybar;
use crate::bluetoothbar;
use crate::characterbar;
use crate::clipboardbar;
use crate::clockbar;
use crate::cpubar;
use crate::custombar;
use crate::dndbar;
use crate::globalmenu;
use crate::inhibitbar;
use crate::keyboardbar;
use crate::languagebar;
use crate::memorybar;
use crate::menuapps::MenuSources;
use crate::mprisbar;
use crate::notificationsbar;
use crate::powerprofilebar;
use crate::schemebar;
use crate::session::Session;
use crate::startbar;
use crate::startmenu::MenuEntry;
use crate::taskbar;
use crate::traybar;
use crate::wallpaperbar;
use crate::wifibar;
use crate::windowbar;
use crate::workspacebar;

/// The session, bar material and loaded shell menu a factory may need.  A
/// factory receives settings, not a second configuration authority; the menu is
/// included because the start module renders the same value the reload did.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleContext {
    pub session: Session,
    pub spacing: i32,
    pub icon_font: Option<String>,
    pub bar: BarSettings,
    pub shell_menu: Rc<Vec<MenuEntry>>,
    /// The dictionary and Orbit corpus the global menu reads.  It is shared,
    /// immutable data, not a second configuration authority.
    pub menu_sources: Rc<MenuSources>,
}

impl ModuleContext {
    pub fn from_bar(bar: &BarSettings, session: Session) -> Self {
        Self {
            session,
            spacing: bar.spacing,
            icon_font: bar.icon_font.clone(),
            bar: bar.clone(),
            shell_menu: Rc::new(Vec::new()),
            menu_sources: Rc::new(MenuSources::default()),
        }
    }
}

pub type ModuleFactory = fn(&ModuleSpec, &ModuleContext) -> Option<gtk::Box>;

const FACTORIES: &[(&str, ModuleFactory)] = &[
    ("audio", audiobar::module),
    ("backlight", backlightbar::module),
    ("battery", batterybar::module),
    ("bluetooth", bluetoothbar::module),
    ("character", characterbar::module),
    ("clock", clockbar::module),
    ("clipboard", clipboardbar::module),
    ("cpu", cpubar::module),
    ("custom", custombar::module),
    ("dnd", dndbar::module),
    ("inhibit", inhibitbar::module),
    ("keyboard", keyboardbar::module),
    ("language", languagebar::module),
    ("media-controls", mprisbar::controls_module),
    ("memory", memorybar::module),
    ("menu", globalmenu::module),
    ("mpris", mprisbar::module),
    ("notifications", notificationsbar::module),
    ("power-profile", powerprofilebar::module),
    ("scheme", schemebar::module),
    ("start", startbar::module),
    ("tasks", taskbar::module),
    ("tray", traybar::module),
    ("wallpaper", wallpaperbar::module),
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

/// Every kind the table answers to, in the table's order.  The status report
/// walks this so a module it has never heard of cannot pass unmentioned.
pub fn kinds() -> impl Iterator<Item = &'static str> {
    FACTORIES.iter().map(|(kind, _)| *kind)
}

/// Build one configured module, or leave it out with a diagnostic.
pub fn build(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let Some(factory) = factory(&module.kind) else {
        eprintln!(
            "whelk: module: unknown kind '{}': it is left out of the bar",
            module.kind
        );

        return None;
    };

    factory(module, context)
}

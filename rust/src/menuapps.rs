// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The application-specific fallback producers for the global menu.
//!
//! A Wayland shell cannot ask a GTK application for its menus unless the
//! compositor implements the GTK shell protocol.  Most do not.  After
//! `org.gtk.Menus`, DBusMenu and AT-SPI have found nothing, whelk can still
//! describe an application from three sources:
//!
//! * the curated Scheme dictionary, in `(whelk menudict)`;
//! * the Orbit shortcut corpus, in `(whelk orbit)`;
//! * the focused application's desktop entry, whose actions and `Exec` line
//!   make a real File menu.
//!
//! This module owns the display-free data and the rules that merge them.  The
//! Scheme walk lives in [`crate::guile`]; the GTK menu bar only receives the
//! resulting [`Menu`] values.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::chord::{self, Chord, ChordSequence, ChordStyle};
use crate::launcherdata;
use crate::menubar::{self, Menu, MenuAction, MenuEntry};
use crate::session::WindowActions;
use crate::settings::SchemeValue;

/// One application in the curated Scheme dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryApp {
    pub name: String,
    pub ids: Vec<String>,
    pub menus: Vec<DictionaryMenu>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryMenu {
    pub label: String,
    pub items: Vec<DictionaryItem>,
}

/// An item as the dictionary writes it.  `accel` is the text its author wrote;
/// `sequence` is that chord once the shell has understood it.  A sequence that
/// could not be read still keeps its printed accelerator and stays disabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryItem {
    pub label: String,
    pub accel: String,
    pub sequence: Option<ChordSequence>,
}

impl DictionaryItem {
    fn menu_entry(&self, tool: &str, style: ChordStyle) -> MenuEntry {
        let accel = self
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.format(style))
            .or_else(|| {
                let displayed = chord::display(&self.accel, style);

                (!displayed.is_empty()).then_some(displayed)
            });
        let command = self
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.command(tool));

        MenuEntry::Item {
            label: self.label.clone(),
            enabled: command.is_some(),
            accel,
            action: command.map(MenuAction::Command),
            submenu: Vec::new(),
        }
    }
}

/// One application in the Orbit corpus.  The corpus is Scheme data, but the
/// menu bar does not need to keep a Guile reference alive to read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrbitApp {
    pub name: String,
    pub ids: Vec<String>,
    pub blank: bool,
    pub menus: Vec<OrbitMenu>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrbitMenu {
    pub label: String,
    pub entries: Vec<OrbitEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrbitEntry {
    Separator,
    Item {
        label: String,
        shortcut: Option<String>,
        action: Option<String>,
    },
    Submenu {
        label: String,
        entries: Vec<OrbitEntry>,
    },
}

/// The two Scheme-owned corpora, as Rust data.  One copy is shared by every bar
/// and every module context; a focus change does not re-read Scheme.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuSources {
    pub dictionary: Vec<DictionaryApp>,
    pub orbit: Vec<OrbitApp>,
}

impl MenuSources {
    pub fn dictionary_for(&self, app_id: &str) -> Option<&DictionaryApp> {
        self.dictionary
            .iter()
            .find(|app| app.ids.iter().any(|id| id_matches(app_id, id)))
    }

    pub fn orbit_for(&self, app_id: &str) -> Option<&OrbitApp> {
        self.orbit
            .iter()
            .find(|app| app.ids.iter().any(|id| id_matches(app_id, id)))
    }
}

/// The case-insensitive identifier comparison shared by the dictionary, Orbit
/// and desktop entries.  A trailing `.desktop` is ignored, and a dotted suffix
/// matches in either direction: `firefox`, `org.mozilla.firefox` and
/// `Firefox` all name the same thing.
pub fn id_matches(app_id: &str, id: &str) -> bool {
    let app = normalize_id(app_id);
    let id = normalize_id(id);

    if app.is_empty() || id.is_empty() {
        return false;
    }

    if app == id {
        return true;
    }

    app.strip_suffix(&id)
        .is_some_and(|prefix| prefix.ends_with('.'))
        || id
            .strip_suffix(&app)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn normalize_id(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .trim_end_matches(".desktop")
        .to_owned()
}

// ---------------------------------------------------------------------------
// Reading Scheme values
// ---------------------------------------------------------------------------

fn alist_value<'a>(entries: &'a [(String, SchemeValue)], key: &str) -> Option<&'a SchemeValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

fn text_value(value: Option<&SchemeValue>) -> Option<String> {
    match value {
        Some(SchemeValue::Text(value) | SchemeValue::Symbol(value)) => Some(value.clone()),
        _ => None,
    }
}

fn text_list(value: Option<&SchemeValue>) -> Vec<String> {
    let Some(SchemeValue::List(values)) = value else {
        return Vec::new();
    };

    values
        .iter()
        .filter_map(|value| match value {
            SchemeValue::Text(value) | SchemeValue::Symbol(value) if !value.is_empty() => {
                Some(value.clone())
            }
            _ => None,
        })
        .collect()
}

fn is_truthy(value: Option<&SchemeValue>) -> bool {
    !matches!(value, Some(SchemeValue::Bool(false)) | None)
}

/// Read `(whelk menudict)`'s `menudict->alists` value.
pub fn dictionary_from_scheme(value: &SchemeValue) -> Vec<DictionaryApp> {
    let SchemeValue::List(apps) = value else {
        return Vec::new();
    };

    apps.iter()
        .filter_map(|app| {
            let SchemeValue::Alist(entries) = app else {
                return None;
            };
            let name = text_value(alist_value(entries, "name"))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "unnamed".to_owned());
            let ids = text_list(alist_value(entries, "ids"));

            if ids.is_empty() {
                return None;
            }

            let menus = match alist_value(entries, "menus") {
                Some(SchemeValue::List(menus)) => menus
                    .iter()
                    .filter_map(dictionary_menu_from_scheme)
                    .collect(),
                _ => Vec::new(),
            };

            Some(DictionaryApp { name, ids, menus })
        })
        .collect()
}

fn dictionary_menu_from_scheme(value: &SchemeValue) -> Option<DictionaryMenu> {
    let SchemeValue::List(menu) = value else {
        return None;
    };
    let label = text_value(menu.first())?.trim().to_owned();

    if label.is_empty() {
        return None;
    }

    let items = menu[1..]
        .iter()
        .filter_map(dictionary_item_from_scheme)
        .collect::<Vec<_>>();

    (!items.is_empty()).then_some(DictionaryMenu { label, items })
}

fn dictionary_item_from_scheme(value: &SchemeValue) -> Option<DictionaryItem> {
    let SchemeValue::List(item) = value else {
        return None;
    };
    let label = text_value(item.first())?.trim().to_owned();

    if label.is_empty() {
        return None;
    }

    if item.len() == 2 {
        let written = text_value(item.get(1))?.trim().to_owned();

        return Some(DictionaryItem {
            label,
            accel: written.clone(),
            sequence: ChordSequence::parse(&written),
        });
    }

    // The long form: (label accel modifiers key), with #f for a bare key.
    let accel = text_value(item.get(1))
        .unwrap_or_default()
        .trim()
        .to_owned();
    let modifiers = text_value(item.get(2));
    let key = text_value(item.get(3)).map(|key| key.trim().to_owned());
    let sequence = key
        .as_deref()
        .and_then(|key| sequence_from_parts(modifiers.as_deref(), key));

    Some(DictionaryItem {
        label,
        accel,
        sequence,
    })
}

fn sequence_from_parts(modifiers: Option<&str>, key: &str) -> Option<ChordSequence> {
    if key.is_empty() {
        return None;
    }

    let modifiers = modifiers.unwrap_or_default().trim();
    let written = if modifiers.is_empty() {
        key.to_owned()
    } else {
        format!(
            "{}+{key}",
            modifiers.split_whitespace().collect::<Vec<_>>().join("+")
        )
    };

    Chord::parse(&written).map(|chord| ChordSequence {
        chords: vec![chord],
    })
}

/// Read `(whelk orbit)`'s `orbit-apps` value.
pub fn orbit_from_scheme(value: &SchemeValue) -> Vec<OrbitApp> {
    // The generic Scheme walker classifies a list whose first element is a
    // symbol as an alist.  `orbit-apps` starts with `(app ...)`, so it arrives
    // as `Alist([("app", rest) ...])`; the menu units inside it do not.  Accept
    // both shapes here rather than teaching the generic walker about one
    // module's tags.
    let apps = match value {
        SchemeValue::List(apps) => apps.iter().collect::<Vec<_>>(),
        SchemeValue::Alist(entries) => entries
            .iter()
            .filter(|(tag, _)| tag == "app")
            .map(|(_, value)| value)
            .collect(),
        _ => return Vec::new(),
    };

    apps.into_iter().filter_map(orbit_app_from_scheme).collect()
}

fn orbit_app_from_scheme(app: &SchemeValue) -> Option<OrbitApp> {
    let SchemeValue::List(app) = app else {
        return None;
    };

    // A normal list carries the `app` tag; the alist shape above has already
    // stripped it.
    let start = if text_value(app.first()).as_deref() == Some("app") {
        1
    } else {
        0
    };

    if app.len() < start + 3 {
        return None;
    }

    let name = text_value(app.get(start))?.trim().to_owned();

    if name.is_empty() {
        return None;
    }

    let ids = match app.get(start + 1) {
        Some(SchemeValue::List(ids)) => ids
            .iter()
            .filter_map(|id| match id {
                SchemeValue::Text(value) | SchemeValue::Symbol(value) if !value.is_empty() => {
                    Some(value.clone())
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    if ids.is_empty() {
        return None;
    }

    let blank = is_truthy(app.get(start + 2));
    let menus = app[start + 3..]
        .iter()
        .filter_map(orbit_menu_from_scheme)
        .collect();

    Some(OrbitApp {
        name,
        ids,
        blank,
        menus,
    })
}

fn orbit_menu_from_scheme(value: &SchemeValue) -> Option<OrbitMenu> {
    let SchemeValue::List(menu) = value else {
        return None;
    };

    if text_value(menu.first()).as_deref() != Some("menu") || menu.len() < 2 {
        return None;
    }

    let label = text_value(menu.get(1))?.trim().to_owned();

    if label.is_empty() {
        return None;
    }

    let mut entries = Vec::new();

    for unit in &menu[2..] {
        append_orbit_unit(&mut entries, unit, 0);
    }

    trim_orbit_separators(&mut entries);

    (!entries.is_empty()).then_some(OrbitMenu { label, entries })
}

fn append_orbit_unit(entries: &mut Vec<OrbitEntry>, value: &SchemeValue, depth: usize) {
    if depth > 16 {
        return;
    }

    let SchemeValue::List(unit) = value else {
        return;
    };
    let Some(tag) = text_value(unit.first()) else {
        return;
    };

    match tag.as_str() {
        "separator" => {
            if entries
                .last()
                .is_some_and(|entry| *entry != OrbitEntry::Separator)
            {
                entries.push(OrbitEntry::Separator);
            }
        }
        "item" => {
            let Some(label) = text_value(unit.get(1)).map(|label| label.trim().to_owned()) else {
                return;
            };

            if label.is_empty() {
                return;
            }

            let shortcut = text_value(unit.get(2)).filter(|shortcut| !shortcut.is_empty());
            let action = text_value(unit.get(3)).filter(|action| !action.is_empty());

            entries.push(OrbitEntry::Item {
                label,
                shortcut,
                action,
            });
        }
        "submenu" => {
            let Some(label) = text_value(unit.get(1)).map(|label| label.trim().to_owned()) else {
                return;
            };

            if label.is_empty() {
                return;
            }

            let mut nested = Vec::new();

            for child in &unit[2..] {
                append_orbit_unit(&mut nested, child, depth + 1);
            }

            trim_orbit_separators(&mut nested);

            if !nested.is_empty() {
                entries.push(OrbitEntry::Submenu {
                    label,
                    entries: nested,
                });
            }
        }
        _ => {}
    }
}

fn trim_orbit_separators(entries: &mut Vec<OrbitEntry>) {
    while entries
        .last()
        .is_some_and(|entry| *entry == OrbitEntry::Separator)
    {
        entries.pop();
    }
}

// ---------------------------------------------------------------------------
// Desktop entries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub actions: Vec<DesktopAction>,
}

impl DesktopEntry {
    /// The command that starts another ordinary window: the entry's `Exec`
    /// with its file, URL and icon field codes removed.
    pub fn new_window_command(&self) -> Option<String> {
        strip_field_codes(&self.exec)
    }
}

impl DesktopAction {
    pub fn command(&self) -> Option<String> {
        strip_field_codes(&self.exec)
    }
}

/// Find the focused application's desktop entry.  The exact filename is tried
/// first; if the compositor reported a bare name, the application directories
/// are then searched for the id containing it, the same conservative rule the
/// C desktop helper uses.
pub fn desktop_entry_for_app_id(app_id: &str) -> Option<DesktopEntry> {
    desktop_entry_in_dirs(app_id, &launcherdata::desktop_dirs())
}

pub fn desktop_entry_in_dirs(app_id: &str, directories: &[PathBuf]) -> Option<DesktopEntry> {
    // A desktop id is a file name, not a path.  Reject a compositor-supplied
    // id that would escape the application directories.
    if app_id.is_empty() || app_id.contains('/') {
        return None;
    }

    let direct = if app_id.ends_with(".desktop") {
        vec![app_id.to_owned()]
    } else {
        vec![format!("{app_id}.desktop"), app_id.to_owned()]
    };

    for name in direct {
        for directory in directories {
            let path = directory.join(&name);

            if let Ok(text) = fs::read_to_string(&path)
                && let Some(entry) = parse_desktop_entry(&text, &name)
            {
                return Some(entry);
            }
        }
    }

    let needle = normalize_id(app_id);

    if needle.is_empty() {
        return None;
    }

    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut files = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name.to_string_lossy().ends_with(".desktop"))
            .collect::<Vec<_>>();

        files.sort();

        for name in files {
            let name = name.to_string_lossy().into_owned();

            if !name.to_ascii_lowercase().contains(&needle) {
                continue;
            }

            let path = directory.join(&name);
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };

            if let Some(entry) = parse_desktop_entry(&text, &name) {
                return Some(entry);
            }
        }
    }

    None
}

/// Parse a `.desktop` file's `[Desktop Entry]` and its action groups.
pub fn parse_desktop_entry(text: &str, filename: &str) -> Option<DesktopEntry> {
    let sections = desktop_sections(text);
    let entry = sections.get("Desktop Entry")?;

    if entry.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }

    let name = entry
        .get("Name")
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())?
        .to_owned();
    let exec = entry.get("Exec").cloned().unwrap_or_default();
    let mut actions = Vec::new();
    let mut seen = HashSet::new();

    for id in entry
        .get("Actions")
        .map(String::as_str)
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if !seen.insert(id.to_owned()) {
            continue;
        }

        let section = format!("Desktop Action {id}");
        let Some(values) = sections.get(&section) else {
            continue;
        };
        let action_name = values
            .get("Name")
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(id)
            .to_owned();

        actions.push(DesktopAction {
            id: id.to_owned(),
            name: action_name,
            exec: values.get("Exec").cloned().unwrap_or_default(),
        });
    }

    Some(DesktopEntry {
        id: filename.trim_end_matches(".desktop").to_owned(),
        name,
        exec,
        actions,
    })
}

fn desktop_sections(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut current = String::new();

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].to_owned();
            sections.entry(current.clone()).or_default();

            continue;
        }

        if current.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            sections.entry(current.clone()).or_default().insert(
                key.trim().to_owned(),
                launcherdata::desktop_unescape(value.trim()),
            );
        }
    }

    sections
}

/// Remove desktop `Exec` field codes.  A menu row has no file, URL or icon to
/// supply, so the row that opens a plain window drops them: `firefox %u`
/// becomes `firefox`, while `%%` remains a literal percent sign.
pub fn strip_field_codes(exec: &str) -> Option<String> {
    let mut command = String::with_capacity(exec.len());
    let mut characters = exec.chars();

    while let Some(character) = characters.next() {
        if character != '%' {
            command.push(character);

            continue;
        }

        match characters.next() {
            Some('%') => command.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k') => {}
            Some(_) | None => {}
        }
    }

    let command = command.trim().to_owned();

    (!command.is_empty()).then_some(command)
}

/// The name a menu bar puts before the application's menus when the dictionary
/// has no name for it.  The desktop entry's `Name` wins; otherwise the last
/// component of the id is read as words, as the C helper does.
pub fn app_name(app_id: &str, desktop: Option<&DesktopEntry>) -> String {
    if let Some(name) = desktop
        .map(|entry| entry.name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_owned();
    }

    if app_id.is_empty() {
        return "whelk".to_owned();
    }

    let mut copy = app_id.trim_end_matches(".desktop").to_owned();

    if let Some(dot) = copy.rfind('.') {
        copy = copy[dot + 1..].to_owned();
    }

    if copy.is_empty() {
        return "Application".to_owned();
    }

    let mut name = String::with_capacity(copy.len());
    let mut start = true;

    for mut character in copy.chars() {
        if character == '-' || character == '_' {
            name.push(' ');
            start = true;
        } else {
            if start {
                character = character.to_ascii_uppercase();
                start = false;
            }

            name.push(character);
        }
    }

    name
}

// ---------------------------------------------------------------------------
// Building the application's menus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltMenus {
    pub menus: Vec<Menu>,
    pub app_name: String,
    /// `Some(name)` when the curated dictionary answered for this application.
    pub dictionary_name: Option<String>,
}

/// Build the application-specific fallback, in the same order as the C shell:
/// desktop actions and New Window, the dictionary (or shared Edit/View), the
/// compositor's Window verbs, then Orbit if the dictionary did not answer.
#[allow(clippy::too_many_arguments)]
pub fn build_menus(
    sources: &MenuSources,
    app_id: &str,
    desktop: Option<&DesktopEntry>,
    window_actions: &WindowActions,
    shortcut_tool: Option<&str>,
    show_shortcuts: bool,
    style: ChordStyle,
) -> BuiltMenus {
    let tool = shortcut_tool
        .filter(|tool| !tool.is_empty())
        .filter(|_| show_shortcuts);
    let can_type = tool.is_some();
    // The dictionary still names an application when no typing tool exists,
    // but its shortcut rows are offered only when one does.
    let known_dictionary = sources.dictionary_for(app_id);
    let dictionary = can_type.then_some(known_dictionary).flatten();
    let mut bar = Vec::new();
    let mut file = Vec::new();
    let new_window = desktop.and_then(DesktopEntry::new_window_command);

    if let Some(command) = new_window.as_deref() {
        file.push(MenuEntry::command("New Window", command));
    }

    if let Some(desktop) = desktop {
        let mut action_rows = false;

        for action in &desktop.actions {
            if new_window.is_some() && action.id == "new-window" {
                continue;
            }

            if !action_rows && !file.is_empty() {
                push_separator(&mut file);
            }

            action_rows = true;

            let command = action.command();

            file.push(MenuEntry::Item {
                label: action.name.clone(),
                enabled: command.is_some(),
                accel: None,
                action: command.map(|command| MenuAction::DesktopAction {
                    desktop_file: format!("{}.desktop", desktop.id),
                    action: action.id.clone(),
                    command,
                }),
                submenu: Vec::new(),
            });
        }
    }

    if let Some(dictionary) = dictionary
        && let Some(dictionary_file) = dictionary
            .menus
            .iter()
            .find(|menu| menu.label.eq_ignore_ascii_case("File"))
    {
        if !file.is_empty() {
            push_separator(&mut file);
        }

        append_dictionary_items(
            &mut file,
            &dictionary_file.items,
            tool.unwrap_or_default(),
            style,
        );
    }

    if let Some(close) = window_actions
        .close
        .as_deref()
        .filter(|close| !close.is_empty())
    {
        add_file_close(&mut file, close);
    }

    if MenuEntry::has_items(&file) {
        bar.push(Menu::with_entries("File", file));
    }

    if let Some(dictionary) = dictionary {
        for menu in &dictionary.menus {
            if menu.label.eq_ignore_ascii_case("File") {
                continue;
            }

            let entries = menu
                .items
                .iter()
                .map(|item| item.menu_entry(tool.unwrap_or_default(), style))
                .collect::<Vec<_>>();

            if MenuEntry::has_items(&entries) {
                bar.push(Menu::with_entries(menu.label.clone(), entries));
            }
        }
    } else if can_type {
        bar.extend(menubar::shared_menus(tool.unwrap_or_default(), style));
    }

    let window = menubar::window_entries(window_actions);

    if MenuEntry::has_items(&window) {
        bar.push(Menu::with_entries("Window", window));
    }

    // Orbit is a gap-filler: a curated dictionary wins for the application it
    // knows, and Orbit's menus replace the synthesized menus with the same
    // label while keeping Window (which the corpus does not describe).
    if dictionary.is_none()
        && can_type
        && !app_id.is_empty()
        && let Some(orbit) = sources.orbit_for(app_id)
        && !orbit.blank
    {
        let imported = orbit_menus(orbit, tool.unwrap_or_default(), style);

        if !imported.is_empty() {
            bar = merge_imported(bar, imported);
        }
    }

    let dictionary_name = known_dictionary.map(|app| app.name.clone());
    let app_name = dictionary_name
        .clone()
        .unwrap_or_else(|| app_name(app_id, desktop));

    BuiltMenus {
        menus: bar,
        app_name,
        dictionary_name,
    }
}

fn append_dictionary_items(
    file: &mut Vec<MenuEntry>,
    items: &[DictionaryItem],
    tool: &str,
    style: ChordStyle,
) {
    for item in items {
        if has_label(file, &item.label) {
            continue;
        }

        file.push(item.menu_entry(tool, style));
    }
}

fn add_file_close(file: &mut Vec<MenuEntry>, command: &str) {
    if has_label(file, "Close Window") {
        return;
    }

    let quit_at = file.iter().position(|entry| {
        matches!(
            entry,
            MenuEntry::Item { label, .. } if label.starts_with("Quit")
        )
    });
    let close = MenuEntry::command("Close Window", command);

    let Some(quit_at) = quit_at else {
        if !file.is_empty() && !matches!(file.last(), Some(MenuEntry::Separator)) {
            file.push(MenuEntry::Separator);
        }

        file.push(close);

        return;
    };

    let needs_separator =
        quit_at > 0 && !matches!(file.get(quit_at - 1), Some(MenuEntry::Separator));

    if needs_separator {
        file.insert(quit_at, MenuEntry::Separator);
        file.insert(quit_at + 1, close);
    } else {
        file.insert(quit_at, close);
    }
}

fn has_label(entries: &[MenuEntry], label: &str) -> bool {
    entries.iter().any(|entry| {
        matches!(entry, MenuEntry::Item { label: candidate, .. } if candidate.eq_ignore_ascii_case(label))
    })
}

fn push_separator(entries: &mut Vec<MenuEntry>) {
    if !entries.is_empty() && !matches!(entries.last(), Some(MenuEntry::Separator)) {
        entries.push(MenuEntry::Separator);
    }
}

fn orbit_menus(app: &OrbitApp, tool: &str, style: ChordStyle) -> Vec<Menu> {
    app.menus
        .iter()
        .filter_map(|menu| {
            let entries = orbit_entries(&menu.entries, tool, style);

            (!entries.is_empty()).then(|| Menu::with_entries(menu.label.clone(), entries))
        })
        .collect()
}

fn orbit_entries(entries: &[OrbitEntry], tool: &str, style: ChordStyle) -> Vec<MenuEntry> {
    let mut result = Vec::new();

    for entry in entries {
        match entry {
            OrbitEntry::Separator => {
                if !result.is_empty() && !matches!(result.last(), Some(MenuEntry::Separator)) {
                    result.push(MenuEntry::Separator);
                }
            }
            OrbitEntry::Item {
                label,
                shortcut,
                action,
            } => {
                if let Some(shortcut) = shortcut.as_deref().filter(|shortcut| !shortcut.is_empty())
                {
                    if let Some(sequence) = ChordSequence::parse_gtk(shortcut) {
                        let command = sequence.command(tool);

                        result.push(MenuEntry::Item {
                            label: label.clone(),
                            enabled: command.is_some(),
                            accel: sequence.format(style),
                            action: command.map(MenuAction::Command),
                            submenu: Vec::new(),
                        });
                    } else {
                        result.push(MenuEntry::Item {
                            label: label.clone(),
                            enabled: false,
                            accel: Some(shortcut.to_owned()),
                            action: None,
                            submenu: Vec::new(),
                        });
                    }
                } else if let Some(action) = action.as_deref().filter(|action| !action.is_empty()) {
                    result.push(MenuEntry::Item {
                        label: label.clone(),
                        enabled: true,
                        accel: None,
                        action: Some(MenuAction::GtkActionName(action.to_owned())),
                        submenu: Vec::new(),
                    });
                } else {
                    result.push(MenuEntry::Item {
                        label: label.clone(),
                        enabled: false,
                        accel: None,
                        action: None,
                        submenu: Vec::new(),
                    });
                }
            }
            OrbitEntry::Submenu { label, entries } => {
                let submenu = orbit_entries(entries, tool, style);

                if !submenu.is_empty() {
                    result.push(MenuEntry::Item {
                        label: label.clone(),
                        enabled: true,
                        accel: None,
                        action: None,
                        submenu,
                    });
                }
            }
        }
    }

    while matches!(result.last(), Some(MenuEntry::Separator)) {
        result.pop();
    }

    result
}

fn merge_imported(fallback: Vec<Menu>, imported: Vec<Menu>) -> Vec<Menu> {
    let mut merged = imported;

    for menu in fallback {
        if !merged.iter().any(|present| present.label == menu.label) {
            merged.push(menu);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(values: Vec<SchemeValue>) -> SchemeValue {
        SchemeValue::List(values)
    }

    fn text(value: &str) -> SchemeValue {
        SchemeValue::Text(value.to_owned())
    }

    fn app(name: &str, ids: &[&str], blank: bool, menus: Vec<SchemeValue>) -> SchemeValue {
        let mut values = vec![
            SchemeValue::Symbol("app".to_owned()),
            text(name),
            list(ids.iter().map(|id| text(id)).collect()),
            SchemeValue::Bool(blank),
        ];

        values.extend(menus);

        list(values)
    }

    #[test]
    fn identifiers_ignore_case_suffix_and_dotted_direction() {
        assert!(id_matches("org.mozilla.firefox", "firefox"));
        assert!(id_matches("Firefox", "firefox"));
        assert!(id_matches("firefox.desktop", "org.mozilla.firefox"));
        assert!(id_matches("foo", "org.example.Foo"));
        assert!(!id_matches("org.example.Nothing", "firefox"));
        assert!(!id_matches("", "firefox"));
    }

    #[test]
    fn dictionary_reads_short_and_long_chords() {
        let item = |values: Vec<SchemeValue>| list(values);
        let menu = list(vec![
            text("Edit"),
            item(vec![text("Copy"), text("C-c")]),
            item(vec![text("Paste"), text("Ctrl+V"), text("ctrl"), text("v")]),
        ]);
        let value = list(vec![SchemeValue::Alist(vec![
            ("name".to_owned(), text("Example")),
            ("ids".to_owned(), list(vec![text("example")])),
            ("menus".to_owned(), list(vec![menu])),
        ])]);
        let dictionaries = dictionary_from_scheme(&value);
        let app = &dictionaries[0];

        assert_eq!(app.name, "Example");
        assert_eq!(
            app.menus[0].items[0]
                .sequence
                .as_ref()
                .unwrap()
                .format(ChordStyle::Cua),
            Some("Ctrl+C".to_owned())
        );
        assert_eq!(
            app.menus[0].items[1]
                .sequence
                .as_ref()
                .unwrap()
                .format(ChordStyle::Cua),
            Some("Ctrl+V".to_owned())
        );
    }

    #[test]
    fn orbit_accepts_the_generic_walkers_alist_shape() {
        // `scheme_value` sees the top-level list start with `(app ...)` and
        // calls it an alist, stripping the tag from every entry.  The real
        // corpus arrives this way.
        let file = list(vec![
            text("menu"),
            text("File"),
            list(vec![text("item"), text("Quit"), text("<Ctrl>Q")]),
        ]);
        let value = SchemeValue::Alist(vec![(
            "app".to_owned(),
            list(vec![
                text("Example"),
                list(vec![text("example")]),
                SchemeValue::Bool(false),
                file,
            ]),
        )]);
        let apps = orbit_from_scheme(&value);

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Example");
        assert_eq!(apps[0].ids, vec!["example"]);
        assert_eq!(apps[0].menus[0].label, "File");
    }

    #[test]
    fn orbit_reads_nested_submenus_and_blank_apps() {
        let file = list(vec![
            text("menu"),
            text("File"),
            list(vec![text("item"), text("Quit"), text("<Ctrl>Q")]),
            list(vec![text("separator")]),
            list(vec![
                text("submenu"),
                text("Recent"),
                list(vec![text("item"), text("One"), text("<Ctrl>1")]),
            ]),
        ]);
        let value = list(vec![
            app("Example", &["example"], false, vec![file]),
            app("Blank", &["blank"], true, vec![]),
        ]);
        let apps = orbit_from_scheme(&value);

        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].menus[0].entries.len(), 3);
        assert!(matches!(
            &apps[0].menus[0].entries[2],
            OrbitEntry::Submenu { label, entries } if label == "Recent" && entries.len() == 1
        ));
        assert!(apps[1].blank);
    }

    #[test]
    fn orbit_entries_type_shortcuts_and_keep_actions() {
        let app = OrbitApp {
            name: "Example".to_owned(),
            ids: vec!["example".to_owned()],
            blank: false,
            menus: vec![OrbitMenu {
                label: "File".to_owned(),
                entries: vec![
                    OrbitEntry::Item {
                        label: "Quit".to_owned(),
                        shortcut: Some("<Ctrl>Q".to_owned()),
                        action: None,
                    },
                    OrbitEntry::Item {
                        label: "Preferences".to_owned(),
                        shortcut: None,
                        action: Some("preferences".to_owned()),
                    },
                    OrbitEntry::Item {
                        label: "Placeholder".to_owned(),
                        shortcut: None,
                        action: None,
                    },
                    OrbitEntry::Separator,
                ],
            }],
        };
        let menus = orbit_menus(&app, "wtype", ChordStyle::Cua);

        assert_eq!(menus[0].entries.len(), 3);
        assert!(matches!(
            &menus[0].entries[0],
            MenuEntry::Item { label, enabled: true, accel: Some(accel), action: Some(MenuAction::Command(_)), .. }
                if label == "Quit" && accel == "Ctrl+Q"
        ));
        assert!(matches!(
            &menus[0].entries[1],
            MenuEntry::Item { action: Some(MenuAction::GtkActionName(name)), .. }
                if name == "preferences"
        ));
        assert!(matches!(
            &menus[0].entries[2],
            MenuEntry::Item { label, enabled: false, .. } if label == "Placeholder"
        ));
    }

    #[test]
    fn desktop_parser_reads_actions_and_strips_field_codes() {
        let entry = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=Example\nExec=/usr/bin/example %u\nActions=New;Edit\n\n[Desktop Action New]\nName=New Window\nExec=/usr/bin/example --new %%file\n\n[Desktop Action Edit]\nName=Edit\nExec=/usr/bin/example --edit %F\n",
            "example.desktop",
        )
        .expect("desktop entry");

        assert_eq!(entry.name, "Example");
        assert_eq!(
            entry.new_window_command().as_deref(),
            Some("/usr/bin/example")
        );
        assert_eq!(entry.actions.len(), 2);
        assert_eq!(entry.actions[0].name, "New Window");
        assert_eq!(
            entry.actions[0].command().as_deref(),
            Some("/usr/bin/example --new %file")
        );
        assert_eq!(
            entry.actions[1].command().as_deref(),
            Some("/usr/bin/example --edit")
        );
    }

    #[test]
    fn desktop_lookup_tries_exact_then_id_fragment() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("org.example.Foo.desktop");

        std::fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Foo\nExec=foo\n",
        )
        .expect("write desktop file");

        let entry =
            desktop_entry_in_dirs("Foo", &[directory.path().to_path_buf()]).expect("lookup");
        assert_eq!(entry.name, "Foo");
        assert_eq!(entry.id, "org.example.Foo");
        assert_eq!(app_name("com.example.toolbox", None), "Toolbox");
        assert_eq!(app_name("google-chrome", None), "Google Chrome");
    }

    #[test]
    fn build_merges_dictionary_file_actions_and_window() {
        let actions = WindowActions {
            close: Some("close".to_owned()),
            fullscreen: Some("fullscreen".to_owned()),
            ..WindowActions::default()
        };
        let sources = MenuSources {
            dictionary: vec![DictionaryApp {
                name: "Example".to_owned(),
                ids: vec!["example".to_owned()],
                menus: vec![
                    DictionaryMenu {
                        label: "File".to_owned(),
                        items: vec![DictionaryItem {
                            label: "Quit".to_owned(),
                            accel: "Ctrl+Q".to_owned(),
                            sequence: ChordSequence::parse("Ctrl+Q"),
                        }],
                    },
                    DictionaryMenu {
                        label: "Edit".to_owned(),
                        items: vec![DictionaryItem {
                            label: "Copy".to_owned(),
                            accel: "Ctrl+C".to_owned(),
                            sequence: ChordSequence::parse("Ctrl+C"),
                        }],
                    },
                ],
            }],
            ..MenuSources::default()
        };
        let desktop = DesktopEntry {
            id: "example".to_owned(),
            name: "Example".to_owned(),
            exec: "example %u".to_owned(),
            actions: vec![DesktopAction {
                id: "new-window".to_owned(),
                name: "New Window".to_owned(),
                exec: "example --new".to_owned(),
            }],
        };
        let built = build_menus(
            &sources,
            "example",
            Some(&desktop),
            &actions,
            Some("wtype"),
            true,
            ChordStyle::Cua,
        );

        assert_eq!(built.app_name, "Example");
        assert_eq!(
            built
                .menus
                .iter()
                .map(|menu| menu.label.as_str())
                .collect::<Vec<_>>(),
            vec!["File", "Edit", "Window"]
        );
        assert!(matches!(
            &built.menus[0].entries[0],
            MenuEntry::Item { label, .. } if label == "New Window"
        ));
        assert!(matches!(&built.menus[0].entries[1], MenuEntry::Separator));
        assert!(matches!(
            &built.menus[0].entries[2],
            MenuEntry::Item { label, .. } if label == "Close Window"
        ));
        assert!(matches!(
            &built.menus[0].entries[3],
            MenuEntry::Item { label, .. } if label == "Quit"
        ));
    }

    #[test]
    fn no_typing_tool_keeps_the_dictionary_name_but_not_its_rows() {
        let actions = WindowActions {
            close: Some("close".to_owned()),
            ..WindowActions::default()
        };
        let sources = MenuSources {
            dictionary: vec![DictionaryApp {
                name: "Dictionary Name".to_owned(),
                ids: vec!["example".to_owned()],
                menus: vec![DictionaryMenu {
                    label: "File".to_owned(),
                    items: vec![DictionaryItem {
                        label: "Quit".to_owned(),
                        accel: "Ctrl+Q".to_owned(),
                        sequence: ChordSequence::parse("Ctrl+Q"),
                    }],
                }],
            }],
            ..MenuSources::default()
        };
        let desktop = DesktopEntry {
            id: "example".to_owned(),
            name: "Desktop Name".to_owned(),
            exec: "example".to_owned(),
            actions: Vec::new(),
        };
        let built = build_menus(
            &sources,
            "example",
            Some(&desktop),
            &actions,
            None,
            true,
            ChordStyle::Cua,
        );

        assert_eq!(built.app_name, "Dictionary Name");
        assert!(built.menus.iter().all(|menu| menu.label != "Edit"));
        assert!(built.menus.iter().all(|menu| menu.label != "View"));
        assert!(
            !built.menus[0]
                .entries
                .iter()
                .any(|entry| { matches!(entry, MenuEntry::Item { label, .. } if label == "Quit") })
        );
    }

    #[test]
    fn blank_orbit_does_not_replace_the_shared_fallback() {
        let actions = WindowActions {
            close: Some("close".to_owned()),
            fullscreen: Some("fullscreen".to_owned()),
            ..WindowActions::default()
        };
        let sources = MenuSources {
            orbit: vec![OrbitApp {
                name: "Blank".to_owned(),
                ids: vec!["blank".to_owned()],
                blank: true,
                menus: Vec::new(),
            }],
            ..MenuSources::default()
        };
        let built = build_menus(
            &sources,
            "blank",
            None,
            &actions,
            Some("wtype"),
            true,
            ChordStyle::Cua,
        );

        assert!(built.menus.iter().any(|menu| menu.label == "Edit"));
        assert!(built.menus.iter().any(|menu| menu.label == "Window"));
    }

    #[test]
    fn orbit_replaces_shared_menus_but_keeps_window() {
        let actions = WindowActions {
            close: Some("close".to_owned()),
            fullscreen: Some("fullscreen".to_owned()),
            ..WindowActions::default()
        };
        let sources = MenuSources {
            orbit: vec![OrbitApp {
                name: "Orbit".to_owned(),
                ids: vec!["orbit".to_owned()],
                blank: false,
                menus: vec![OrbitMenu {
                    label: "Edit".to_owned(),
                    entries: vec![OrbitEntry::Item {
                        label: "Copy".to_owned(),
                        shortcut: Some("<Ctrl>C".to_owned()),
                        action: None,
                    }],
                }],
            }],
            ..MenuSources::default()
        };
        let built = build_menus(
            &sources,
            "orbit",
            None,
            &actions,
            Some("wtype"),
            true,
            ChordStyle::Cua,
        );

        assert!(built.menus.iter().any(|menu| menu.label == "Edit"));
        assert!(built.menus.iter().any(|menu| menu.label == "Window"));
        // Orbit replaces the shared menus it describes; a shared menu it does
        // not describe (View here) remains the gap-filler, exactly as the C
        // merge does.
        assert!(built.menus.iter().any(|menu| menu.label == "View"));
    }
}

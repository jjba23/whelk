// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The global menu's data model, ported from the menubar half of
//! `util/menutree.c`.
//!
//! A menubar is the focused application's top-level menus, each with entries
//! and, below them, submenus.  The model here is deliberately display-free:
//! labels, accelerators, enabled state and an owned [`MenuAction`].  A parser
//! never hands a widget a pointer into a `GVariant`, and malformed D-Bus data
//! becomes an empty menu rather than a panic.
//!
//! Three producers feed it:
//!
//! * `org.gtk.Menus.Start`, whose `a(uuaa{sv})` answer names actions by string;
//! * `com.canonical.dbusmenu.GetLayout`, whose rows are ids on the publishing
//!   object; and
//! * a configuration's `#:menu-command`, whose lines are
//!   `Menu<TAB>Item<TAB>Chord<TAB>Command`.
//!
//! The fallback menubar (File, Edit, View, Window) is built from the focused
//! window's compositor verbs and the shortcut tool, exactly as the C module
//! builds it when no application exports a menu.

use std::collections::{HashMap, HashSet};

use gtk::glib;
use gtk::glib::VariantDict;
use gtk4 as gtk;

use crate::chord::{self, Chord, ChordStyle};
use crate::session::WindowActions;
use crate::traymenu::{self, TrayMenuEntry};

/// The largest menu tree a producer may describe.  A malformed or hostile
/// `org.gtk.Menus` answer can point a section back at itself; the depth and
/// path guards below turn that into a truncated menu.
pub const MAX_DEPTH: usize = 32;

/// The largest number of rows a single `org.gtk.Menus` answer may expand into.
/// The depth guard alone does not stop a wide DAG that reuses the same section
/// from many parents; this bounds the total work as well as the path length.
pub const MAX_NODES: usize = 4096;

/// What choosing a row does.  The action is data, so the same entry can be
/// rendered by the popover or selected by a future menu search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// A shell command, already shell-escaped where it needs to be.
    Command(String),
    /// `Event(id, "clicked", …)` on the object that published a DBusMenu.
    DBusMenu {
        service: String,
        path: String,
        id: i32,
    },
    /// `org.gtk.Actions.Activate` on the focused application.  `path` is the
    /// object the action lives on: the application object, or its first
    /// window for a `win.*` action.
    GtkAction {
        service: String,
        path: String,
        name: String,
        target: Option<String>,
    },
    /// An `org.gtk.Actions` name whose service and object path are resolved
    /// against the focused window when it is chosen.  The Orbit corpus names
    /// actions this way; it has no bus connection to resolve them at read time.
    GtkActionName(String),
    /// An action in a desktop entry, launched by its `Exec` line.  The desktop
    /// file and action id are kept for the log and for a future GIO launch.
    DesktopAction {
        desktop_file: String,
        action: String,
        command: String,
    },
    /// `org.a11y.atspi.Action.DoAction` on an accessibility-bus object.  The
    /// object lives on the accessibility bus, not the session bus.
    Atspi {
        service: String,
        path: String,
        action: i32,
    },
}

/// One row or submenu in a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Separator,
    Item {
        label: String,
        enabled: bool,
        accel: Option<String>,
        action: Option<MenuAction>,
        submenu: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    /// A clickable row with no accelerator or submenu.
    pub fn item(label: impl Into<String>, action: Option<MenuAction>) -> Self {
        Self::Item {
            label: label.into(),
            enabled: true,
            accel: None,
            action,
            submenu: Vec::new(),
        }
    }

    /// A row whose action is a shell command.
    pub fn command(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self::item(label, Some(MenuAction::Command(command.into())))
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn has_items(entries: &[MenuEntry]) -> bool {
        entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Item { .. }))
    }
}

/// One top-level menu in the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub label: String,
    pub entries: Vec<MenuEntry>,
}

impl Menu {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            entries: Vec::new(),
        }
    }

    pub fn with_entries(label: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self {
            label: label.into(),
            entries,
        }
    }
}

// ---------------------------------------------------------------------------
// The configuration's own menu program
// ---------------------------------------------------------------------------

/// Read the `Menu<TAB>Item<TAB>Chord<TAB>Command` protocol used by
/// `#:menu-command`.  A `>` in the first field nests a submenu.  A row with no
/// command is shown disabled, the same visible answer a failed shortcut gets.
pub fn from_text(text: &str, style: ChordStyle) -> Vec<Menu> {
    let mut bar: Vec<Menu> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.splitn(4, '\t').collect();

        if fields.len() < 2 || fields[0].is_empty() {
            continue;
        }

        let Some(entries) = text_menu_entries(&mut bar, fields[0]) else {
            continue;
        };

        if fields[1] == "-" {
            entries.push(MenuEntry::Separator);

            continue;
        }

        let accel = fields
            .get(2)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| chord::display(value, style));
        let command = fields
            .get(3)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        let enabled = command.is_some();

        entries.push(MenuEntry::Item {
            label: fields[1].to_owned(),
            enabled,
            accel,
            action: command.map(|value| MenuAction::Command(value.to_owned())),
            submenu: Vec::new(),
        });
    }

    bar.retain(|menu| !menu.entries.is_empty());
    bar
}

fn text_menu_entries<'a>(bar: &'a mut Vec<Menu>, path: &str) -> Option<&'a mut Vec<MenuEntry>> {
    let mut segments = path.split('>').map(str::trim);
    let first = segments.next().unwrap_or_default();
    let position = bar
        .iter()
        .position(|menu| menu.label == first)
        .unwrap_or_else(|| {
            bar.push(Menu::new(first));

            bar.len().saturating_sub(1)
        });
    let entries = &mut bar.get_mut(position)?.entries;
    let rest: Vec<&str> = segments.filter(|segment| !segment.is_empty()).collect();

    descend_menu(entries, &rest)
}

/// Walk the remaining path segments.  Recursing (rather than rebinding a
/// mutable borrow in a loop) lets the final borrow be returned directly, and
/// an unexpected shape returns `None` instead of panicking.
fn descend_menu<'a>(
    entries: &'a mut Vec<MenuEntry>,
    segments: &[&str],
) -> Option<&'a mut Vec<MenuEntry>> {
    let Some((segment, rest)) = segments.split_first() else {
        return Some(entries);
    };
    let position = ensure_submenu(entries, segment);
    let Some(MenuEntry::Item { submenu, .. }) = entries.get_mut(position) else {
        return None;
    };

    descend_menu(submenu, rest)
}

fn ensure_submenu(entries: &mut Vec<MenuEntry>, label: &str) -> usize {
    entries
        .iter()
        .position(|entry| {
            matches!(
                entry,
                MenuEntry::Item {
                    label: entry_label,
                    submenu,
                    ..
                } if entry_label == label && !submenu.is_empty()
            )
        })
        .unwrap_or_else(|| {
            entries.push(MenuEntry::Item {
                label: label.to_owned(),
                enabled: true,
                accel: None,
                action: None,
                submenu: Vec::new(),
            });

            entries.len().saturating_sub(1)
        })
}

// ---------------------------------------------------------------------------
// org.gtk.Menus
// ---------------------------------------------------------------------------

/// Read the `a(uuaa{sv})` answer to `org.gtk.Menus.Start`.
///
/// The root is menu `(0, 0)`, as in the C parser.  A top-level `:section` is
/// flattened into the bar; a top-level item with no `:submenu` becomes a
/// one-row menu, which is how GTK exports an application-wide action.
pub fn from_gtk(
    reply: &glib::Variant,
    service: &str,
    app_path: Option<&str>,
    style: ChordStyle,
) -> Vec<Menu> {
    if reply.type_().as_str() != "a(uuaa{sv})" {
        return Vec::new();
    }

    let mut menus: HashMap<(u32, u32), Vec<glib::Variant>> = HashMap::new();

    for index in 0..reply.n_children() {
        let Some(entry) = reply.try_child_value(index) else {
            continue;
        };

        if entry.type_().as_str() != "(uuaa{sv})" || entry.n_children() != 3 {
            continue;
        }

        let Some(group) = entry
            .try_child_value(0)
            .and_then(|value| value.get::<u32>())
        else {
            continue;
        };
        let Some(menu) = entry
            .try_child_value(1)
            .and_then(|value| value.get::<u32>())
        else {
            continue;
        };
        let Some(items) = entry.try_child_value(2) else {
            continue;
        };

        if items.type_().as_str() != "aa{sv}" {
            continue;
        }

        let rows = menus.entry((group, menu)).or_default();

        for item in 0..items.n_children() {
            if let Some(attrs) = items.try_child_value(item) {
                rows.push(attrs);
            }
        }
    }

    let Some(root) = menus.get(&(0, 0)) else {
        return Vec::new();
    };
    let mut bar = Vec::new();
    let mut stack = HashSet::new();
    let mut budget = MAX_NODES;

    collect_top_level(
        &mut bar,
        root,
        &menus,
        service,
        app_path,
        style,
        0,
        &mut stack,
        &mut budget,
    );

    bar.retain(|menu| !menu.entries.is_empty());
    bar
}

#[allow(clippy::too_many_arguments)]
fn collect_top_level(
    bar: &mut Vec<Menu>,
    items: &[glib::Variant],
    menus: &HashMap<(u32, u32), Vec<glib::Variant>>,
    service: &str,
    app_path: Option<&str>,
    style: ChordStyle,
    depth: usize,
    stack: &mut HashSet<(u32, u32)>,
    budget: &mut usize,
) {
    if depth > MAX_DEPTH {
        return;
    }

    for attrs in items {
        if *budget == 0 {
            return;
        }

        *budget -= 1;

        let Some(dict) = attrs_dict(attrs) else {
            continue;
        };

        if let Some(target) = link_target(&dict, ":section") {
            if let Some(section) = menus.get(&target) {
                if !stack.insert(target) {
                    continue;
                }

                collect_top_level(
                    bar,
                    section,
                    menus,
                    service,
                    app_path,
                    style,
                    depth + 1,
                    stack,
                    budget,
                );
                stack.remove(&target);
            }

            continue;
        }

        let label = dict
            .lookup::<String>("label")
            .ok()
            .flatten()
            .unwrap_or_default();

        if label.is_empty() {
            continue;
        }

        let mut entries = Vec::new();
        let submenu_target = link_target(&dict, ":submenu");

        if let Some(target) = submenu_target
            && let Some(submenu) = menus.get(&target)
            && stack.insert(target)
        {
            append_items(
                &mut entries,
                submenu,
                menus,
                service,
                app_path,
                style,
                depth + 1,
                stack,
                budget,
            );
            stack.remove(&target);
        } else if submenu_target.is_none() {
            entries.push(MenuEntry::Item {
                label: label.clone(),
                enabled: true,
                accel: None,
                action: gtk_action(&dict, service, app_path),
                submenu: Vec::new(),
            });
        }

        if !entries.is_empty() {
            bar.push(Menu::with_entries(label, entries));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn append_items(
    entries: &mut Vec<MenuEntry>,
    items: &[glib::Variant],
    menus: &HashMap<(u32, u32), Vec<glib::Variant>>,
    service: &str,
    app_path: Option<&str>,
    style: ChordStyle,
    depth: usize,
    stack: &mut HashSet<(u32, u32)>,
    budget: &mut usize,
) {
    if depth > MAX_DEPTH {
        return;
    }

    for attrs in items {
        if *budget == 0 {
            return;
        }

        *budget -= 1;

        let Some(dict) = attrs_dict(attrs) else {
            continue;
        };

        if let Some(target) = link_target(&dict, ":section") {
            if let Some(section) = menus.get(&target) {
                if !stack.insert(target) {
                    continue;
                }

                if !entries.is_empty() {
                    entries.push(MenuEntry::Separator);
                }

                append_items(
                    entries,
                    section,
                    menus,
                    service,
                    app_path,
                    style,
                    depth + 1,
                    stack,
                    budget,
                );
                stack.remove(&target);
            }

            continue;
        }

        let label = dict
            .lookup::<String>("label")
            .ok()
            .flatten()
            .unwrap_or_default();
        let submenu_target = link_target(&dict, ":submenu");
        let action = gtk_action(&dict, service, app_path);

        if label.is_empty() && submenu_target.is_none() {
            continue;
        }

        let accel = dict
            .lookup::<String>("accel")
            .ok()
            .flatten()
            .filter(|value| !value.is_empty())
            .map(|value| chord::display(&value, style));
        let enabled = dict
            .lookup::<bool>("enabled")
            .ok()
            .flatten()
            .unwrap_or(true);
        let mut submenu = Vec::new();

        if let Some(target) = submenu_target
            && let Some(items) = menus.get(&target)
            && stack.insert(target)
        {
            append_items(
                &mut submenu,
                items,
                menus,
                service,
                app_path,
                style,
                depth + 1,
                stack,
                budget,
            );
            stack.remove(&target);
        }

        entries.push(MenuEntry::Item {
            label,
            enabled,
            accel,
            action,
            submenu,
        });
    }
}

fn attrs_dict(attrs: &glib::Variant) -> Option<VariantDict> {
    (attrs.type_().as_str() == "a{sv}").then(|| VariantDict::new(Some(attrs)))
}

fn link_target(dict: &VariantDict, key: &str) -> Option<(u32, u32)> {
    dict.lookup::<(u32, u32)>(key).ok().flatten()
}

fn gtk_action(dict: &VariantDict, service: &str, app_path: Option<&str>) -> Option<MenuAction> {
    let app_path = app_path.filter(|path| !path.is_empty())?;
    let action = dict.lookup::<String>("action").ok().flatten()?;
    let (prefix, name) = action
        .split_once('.')
        .filter(|(_, name)| !name.is_empty())
        .unwrap_or((&action, &action));
    let path = if prefix == "win" {
        format!("{app_path}/windows/0")
    } else {
        app_path.to_owned()
    };
    let target = dict.lookup::<String>("target").ok().flatten();

    Some(MenuAction::GtkAction {
        service: service.to_owned(),
        path,
        name: name.to_owned(),
        target,
    })
}

// ---------------------------------------------------------------------------
// com.canonical.dbusmenu
// ---------------------------------------------------------------------------

/// Read a menubar from a `GetLayout` reply.  The root's children are the
/// top-level menus; each of their children is a row.  The node reader itself
/// lives in [`crate::traymenu`] and is shared with tray context menus.
pub fn from_dbusmenu(
    reply: &glib::Variant,
    service: &str,
    path: &str,
    style: ChordStyle,
) -> Vec<Menu> {
    traymenu::parse_menubar(reply, service, path, style)
        .into_iter()
        .filter_map(|(label, entries)| {
            let entries = entries
                .into_iter()
                .map(menu_entry_from_tray)
                .collect::<Vec<_>>();

            (!label.is_empty() && !entries.is_empty()).then(|| Menu::with_entries(label, entries))
        })
        .collect()
}

fn menu_entry_from_tray(entry: TrayMenuEntry) -> MenuEntry {
    match entry {
        TrayMenuEntry::Separator => MenuEntry::Separator,
        TrayMenuEntry::Item {
            label,
            enabled,
            accel,
            action,
            submenu,
        } => MenuEntry::Item {
            label,
            enabled,
            accel,
            action: Some(MenuAction::DBusMenu {
                service: action.service,
                path: action.path,
                id: action.id,
            }),
            submenu: submenu.into_iter().map(menu_entry_from_tray).collect(),
        },
    }
}

// ---------------------------------------------------------------------------
// The synthesized menubar
// ---------------------------------------------------------------------------

/// The menus a focused application gets when it publishes none: its window's
/// compositor verbs, the shortcuts every application shares (when a typing tool
/// exists), and Close Window in File.
pub fn fallback_menus(
    actions: &WindowActions,
    shortcut_tool: Option<&str>,
    show_shortcuts: bool,
    style: ChordStyle,
) -> Vec<Menu> {
    let mut bar = Vec::new();
    let mut file = Vec::new();

    if let Some(close) = actions.close.as_deref().filter(|value| !value.is_empty()) {
        file.push(MenuEntry::command("Close Window", close));
    }

    if !file.is_empty() {
        bar.push(Menu::with_entries("File", file));
    }

    if show_shortcuts && let Some(tool) = shortcut_tool.filter(|tool| !tool.is_empty()) {
        let edit = shared_edit_menu(tool, style);

        if MenuEntry::has_items(&edit) {
            bar.push(Menu::with_entries("Edit", edit));
        }

        let view = shared_view_menu(tool, style);

        if MenuEntry::has_items(&view) {
            bar.push(Menu::with_entries("View", view));
        }
    }

    let window = window_menu(actions);

    if MenuEntry::has_items(&window) {
        bar.push(Menu::with_entries("Window", window));
    }

    bar
}

/// The arranging verbs for the focused window.  Kept public so the desktop
/// fallback can put the same rows under its own Window menu.
pub fn window_entries(actions: &WindowActions) -> Vec<MenuEntry> {
    window_menu(actions)
}

/// The shared Edit and View menus.  A caller that has an application menu of
/// its own does not use these; the dictionary and Orbit producers replace them.
pub fn shared_menus(tool: &str, style: ChordStyle) -> Vec<Menu> {
    let mut menus = Vec::new();
    let edit = shared_edit_menu(tool, style);

    if MenuEntry::has_items(&edit) {
        menus.push(Menu::with_entries("Edit", edit));
    }

    let view = shared_view_menu(tool, style);

    if MenuEntry::has_items(&view) {
        menus.push(Menu::with_entries("View", view));
    }

    menus
}

fn window_menu(actions: &WindowActions) -> Vec<MenuEntry> {
    let mut entries = Vec::new();

    push_command(
        &mut entries,
        "Maximize / Restore",
        actions.maximize.as_deref(),
    );
    push_command(
        &mut entries,
        "Toggle Floating",
        actions.float_window.as_deref(),
    );

    let moves = [
        ("Move Left", actions.move_left.as_deref()),
        ("Move Right", actions.move_right.as_deref()),
        ("Move Up", actions.move_up.as_deref()),
        ("Move Down", actions.move_down.as_deref()),
    ];

    if moves.iter().any(|(_, command)| command.is_some()) && MenuEntry::has_items(&entries) {
        entries.push(MenuEntry::Separator);
    }

    for (label, command) in moves {
        push_command(&mut entries, label, command);
    }

    if let Some(fullscreen) = actions
        .fullscreen
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if MenuEntry::has_items(&entries) {
            entries.push(MenuEntry::Separator);
        }

        entries.push(MenuEntry::command("Full Screen", fullscreen));
    }

    entries
}

fn push_command(entries: &mut Vec<MenuEntry>, label: &str, command: Option<&str>) {
    if let Some(command) = command.filter(|command| !command.is_empty()) {
        entries.push(MenuEntry::command(label, command));
    }
}

fn shared_edit_menu(tool: &str, style: ChordStyle) -> Vec<MenuEntry> {
    let mut entries = Vec::new();

    shortcut_entry(&mut entries, "Undo", "Ctrl+Z", tool, style);
    shortcut_entry(&mut entries, "Redo", "Ctrl+Shift+Z", tool, style);
    entries.push(MenuEntry::Separator);
    shortcut_entry(&mut entries, "Cut", "Ctrl+X", tool, style);
    shortcut_entry(&mut entries, "Copy", "Ctrl+C", tool, style);
    shortcut_entry(&mut entries, "Paste", "Ctrl+V", tool, style);
    shortcut_entry(
        &mut entries,
        "Paste and Match Style",
        "Ctrl+Shift+V",
        tool,
        style,
    );
    shortcut_entry(&mut entries, "Select All", "Ctrl+A", tool, style);
    entries.push(MenuEntry::Separator);
    shortcut_entry(&mut entries, "Find", "Ctrl+F", tool, style);
    shortcut_entry(&mut entries, "Find Next", "F3", tool, style);
    shortcut_entry(&mut entries, "Find Previous", "Shift+F3", tool, style);
    shortcut_entry(&mut entries, "Replace", "Ctrl+H", tool, style);

    entries
}

fn shared_view_menu(tool: &str, style: ChordStyle) -> Vec<MenuEntry> {
    let mut entries = Vec::new();

    shortcut_entry(&mut entries, "Reload", "Ctrl+R", tool, style);
    entries.push(MenuEntry::Separator);
    shortcut_entry(&mut entries, "Zoom In", "Ctrl++", tool, style);
    shortcut_entry(&mut entries, "Zoom Out", "Ctrl+-", tool, style);
    shortcut_entry(&mut entries, "Actual Size", "Ctrl+0", tool, style);
    entries.push(MenuEntry::Separator);
    shortcut_entry(&mut entries, "Full Screen", "F11", tool, style);

    entries
}

fn shortcut_entry(
    entries: &mut Vec<MenuEntry>,
    label: &str,
    accel: &str,
    tool: &str,
    style: ChordStyle,
) {
    let command = Chord::parse(accel).and_then(|chord| chord.command(tool));

    entries.push(MenuEntry::Item {
        label: label.to_owned(),
        enabled: command.is_some(),
        accel: Some(chord::display(accel, style)),
        action: command.map(MenuAction::Command),
        submenu: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib::prelude::*;

    fn attrs(entries: &[(&str, glib::Variant)]) -> glib::Variant {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<HashMap<_, _>>()
            .to_variant()
    }

    fn gtk_bar(menus: Vec<(u32, u32, Vec<glib::Variant>)>) -> glib::Variant {
        menus
            .into_iter()
            .map(|(group, menu, items)| {
                let items = items
                    .iter()
                    .filter_map(|item| item.get::<HashMap<String, glib::Variant>>())
                    .collect::<Vec<_>>();

                (group, menu, items)
            })
            .collect::<Vec<_>>()
            .to_variant()
    }

    #[test]
    fn text_menus_nest_submenus_and_keep_disabled_rows() {
        let bar = from_text(
            "File\tOpen\tCtrl+O\txdg-open .\n\
             File>Recent\tOne\t\txdg-open one\n\
             File\tQuit\tCtrl+Q\t\n",
            ChordStyle::Cua,
        );

        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].label, "File");
        assert!(matches!(
            &bar[0].entries[0],
            MenuEntry::Item {
                label,
                accel: Some(accel),
                action: Some(MenuAction::Command(command)),
                ..
            } if label == "Open" && accel == "Ctrl+O" && command == "xdg-open ."
        ));
        assert!(matches!(
            &bar[0].entries[1],
            MenuEntry::Item { label, submenu, .. }
                if label == "Recent"
                    && matches!(&submenu[0], MenuEntry::Item { label, .. } if label == "One")
        ));
        assert!(matches!(
            &bar[0].entries[2],
            MenuEntry::Item {
                label,
                enabled: false,
                ..
            } if label == "Quit"
        ));
    }

    #[test]
    fn gtk_sections_are_flattened_and_submenus_kept() {
        let reply = gtk_bar(vec![
            (
                0,
                0,
                vec![attrs(&[(":section", (0u32, 1u32).to_variant())])],
            ),
            (
                0,
                1,
                vec![
                    attrs(&[
                        ("label", "File".to_variant()),
                        (":submenu", (0u32, 2u32).to_variant()),
                    ]),
                    attrs(&[
                        ("label", "Edit".to_variant()),
                        (":submenu", (0u32, 3u32).to_variant()),
                    ]),
                ],
            ),
            (
                0,
                2,
                vec![
                    attrs(&[
                        ("label", "Open".to_variant()),
                        ("action", "app.open".to_variant()),
                    ]),
                    attrs(&[
                        ("label", "Quit".to_variant()),
                        ("enabled", false.to_variant()),
                    ]),
                ],
            ),
            (
                0,
                3,
                vec![attrs(&[
                    ("label", "Copy".to_variant()),
                    ("action", "win.copy".to_variant()),
                    ("accel", "<Primary>c".to_variant()),
                    ("target", "selection".to_variant()),
                ])],
            ),
        ]);
        assert_eq!(reply.type_().as_str(), "a(uuaa{sv})");
        let bar = from_gtk(
            &reply,
            "org.example.App",
            Some("/org/example/App"),
            ChordStyle::Cua,
        );

        assert_eq!(bar.len(), 2);
        assert_eq!(bar[0].label, "File");
        assert!(matches!(
            &bar[0].entries[0],
            MenuEntry::Item {
                label,
                action: Some(MenuAction::GtkAction {
                    service,
                    path,
                    name,
                    ..
                }),
                ..
            } if label == "Open"
                && service == "org.example.App"
                && path == "/org/example/App"
                && name == "open"
        ));
        assert!(matches!(
            &bar[0].entries[1],
            MenuEntry::Item {
                label,
                enabled: false,
                ..
            } if label == "Quit"
        ));
        assert!(matches!(
            &bar[1].entries[0],
            MenuEntry::Item {
                label,
                accel: Some(accel),
                action: Some(MenuAction::GtkAction {
                    path,
                    name,
                    target: Some(target),
                    ..
                }),
                ..
            } if label == "Copy"
                && accel == "Ctrl+C"
                && path == "/org/example/App/windows/0"
                && name == "copy"
                && target == "selection"
        ));
    }

    #[test]
    fn gtk_sections_separate_entries_and_cycles_are_cut() {
        let reply = gtk_bar(vec![
            (
                0,
                0,
                vec![attrs(&[(":section", (0u32, 1u32).to_variant())])],
            ),
            (
                0,
                1,
                vec![
                    attrs(&[("label", "First".to_variant())]),
                    attrs(&[(":section", (0u32, 0u32).to_variant())]),
                ],
            ),
        ]);
        let bar = from_gtk(&reply, "app", Some("/app"), ChordStyle::Cua);

        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].entries.len(), 1);
    }

    #[test]
    fn dbusmenu_groups_top_level_nodes_into_menus() {
        let child = |id: i32, props: &[(&str, glib::Variant)]| {
            (
                id,
                props
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect::<HashMap<_, _>>(),
                Vec::<glib::Variant>::new(),
            )
                .to_variant()
        };
        let file = (
            10i32,
            [("label".to_owned(), "File".to_variant())]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            vec![child(11, &[("label", "Quit".to_variant())])],
        );
        let reply = (
            1u32,
            (
                0i32,
                HashMap::<String, glib::Variant>::new(),
                vec![file.to_variant()],
            ),
        )
            .to_variant();

        assert_eq!(reply.type_().as_str(), "(u(ia{sv}av))");
        let bar = from_dbusmenu(&reply, "app", "/MenuBar/1", ChordStyle::Cua);

        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].label, "File");
        assert!(matches!(
            &bar[0].entries[0],
            MenuEntry::Item {
                label,
                action: Some(MenuAction::DBusMenu { service, path, id }),
                ..
            } if label == "Quit"
                && service == "app"
                && path == "/MenuBar/1"
                && *id == 11
        ));
    }

    #[test]
    fn fallback_menus_only_offer_actions_that_work() {
        let actions = WindowActions {
            close: Some("close".to_owned()),
            fullscreen: Some("fullscreen".to_owned()),
            maximize: None,
            ..WindowActions::default()
        };
        let menus = fallback_menus(&actions, Some("wtype"), true, ChordStyle::Cua);

        assert_eq!(
            menus
                .iter()
                .map(|menu| menu.label.as_str())
                .collect::<Vec<_>>(),
            vec!["File", "Edit", "View", "Window"]
        );
        assert!(matches!(
            &menus[0].entries[0],
            MenuEntry::Item { label, action: Some(MenuAction::Command(command)), .. }
                if label == "Close Window" && command == "close"
        ));
        assert!(matches!(
            &menus[1].entries[0],
            MenuEntry::Item { label, enabled: true, .. } if label == "Undo"
        ));
        assert!(matches!(
            &menus[3].entries[0],
            MenuEntry::Item { label, .. } if label == "Full Screen"
        ));

        let without_tool = fallback_menus(&actions, None, true, ChordStyle::Cua);

        assert_eq!(
            without_tool
                .iter()
                .map(|menu| menu.label.as_str())
                .collect::<Vec<_>>(),
            vec!["File", "Window"]
        );

        let without_shortcuts = fallback_menus(&actions, Some("wtype"), false, ChordStyle::Cua);

        assert_eq!(
            without_shortcuts
                .iter()
                .map(|menu| menu.label.as_str())
                .collect::<Vec<_>>(),
            vec!["File", "Window"]
        );
    }
}

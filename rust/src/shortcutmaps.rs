// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! User-installed Orbit shortcut maps, ported from `util/shortcutmaps.c`.
//!
//! Orbit ships its corpus as one JSON file per application.  The built-in
//! Scheme corpus is the common case, but a user can put a reviewed correction
//! under `global-menu/shortcuts/apps` (or point `WHELK_SHORTCUT_MAPS` at one
//! file or directory) without rebuilding the shell.  The identifier index is
//! read once with the configuration; the matching file's menus are parsed only
//! when an application asks for them.
//!
//! The JSON format is deliberately small:
//!
//! ```json
//! {"identifiers": ["org.example.Demo", "demo"],
//!  "blank": false,
//!  "menus": [{"label": "File", "items": [
//!    {"label": "New", "shortcut": "<Ctrl>N"},
//!    {"label": "Recent", "items": [{"label": "Open", "shortcut": "<Ctrl>O"}]}
//!  ]}]}
//! ```
//!
//! A shortcut becomes a command typed by the configured helper.  A row with an
//! action and no integer target is an `org.gtk.Actions` name; a targeted action
//! is left disabled because the menu model carries only strings.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::chord::{ChordSequence, ChordStyle};
use crate::json::Json;
use crate::menuapps::id_matches;
use crate::menubar::{Menu, MenuAction, MenuEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShortcutMapping {
    path: PathBuf,
    ids: Vec<String>,
    blank: bool,
}

/// The user-installed mapping index.  It is configuration data, shared by the
/// bars; it is not a running service and does not watch the directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutMaps {
    mappings: Vec<ShortcutMapping>,
}

impl ShortcutMaps {
    /// Read the standard locations, or `WHELK_SHORTCUT_MAPS` when it is set.
    /// An unreadable directory or malformed file is skipped: a fallback that
    /// cannot be read must not take the menu down with it.
    pub fn load() -> Self {
        let mut paths = Vec::new();

        match env::var_os("WHELK_SHORTCUT_MAPS").filter(|path| !path.is_empty()) {
            Some(override_path) => add_search_path(&mut paths, Path::new(&override_path)),
            None => {
                add_search_path(
                    &mut paths,
                    &xdg_dir("XDG_CONFIG_HOME", ".config").join("global-menu/shortcuts/apps"),
                );
                add_search_path(
                    &mut paths,
                    &xdg_dir("XDG_DATA_HOME", ".local/share").join("global-menu/shortcuts/apps"),
                );
                add_search_path(
                    &mut paths,
                    Path::new("/usr/share/global-menu/shortcuts/apps"),
                );
            }
        }

        Self::from_paths(paths)
    }

    /// Build an index from explicit files and directories.  This is the
    /// testable half of [`Self::load`]; paths are sorted, so two directories
    /// that provide the same identifier have a deterministic winner.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        let mut files = Vec::new();

        for path in &paths {
            if path.is_dir() {
                collect_json_files(path, &mut files);
            } else if path.is_file() {
                files.push(path.clone());
            }
        }

        files.sort();
        files.dedup();

        let mappings = files.iter().filter_map(|path| read_mapping(path)).collect();

        Self { mappings }
    }

    /// The first mapping whose identifier matches `app_id`, as a menu tree.
    /// `None` means no mapping matched; `Some(empty)` means a matching mapping
    /// deliberately describes no menus (or could not be parsed), which is still
    /// an answer and keeps the built-in corpus from overriding a local choice.
    pub fn menus_for(&self, app_id: &str, tool: &str, style: ChordStyle) -> Option<Vec<Menu>> {
        if app_id.is_empty() || tool.is_empty() {
            return None;
        }

        let mapping = self
            .mappings
            .iter()
            .find(|mapping| mapping.ids.iter().any(|id| id_matches(app_id, id)))?;

        if mapping.blank {
            return Some(Vec::new());
        }

        let Ok(text) = fs::read_to_string(&mapping.path) else {
            return Some(Vec::new());
        };
        let Ok(root) = Json::parse(&text) else {
            return Some(Vec::new());
        };

        Some(menus_from_json(&root, tool, style))
    }
}

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    if let Some(value) = env::var_os(variable).filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }

    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(fallback)
}

fn add_search_path(paths: &mut Vec<PathBuf>, path: &Path) {
    if !path.as_os_str().is_empty() {
        paths.push(path.to_path_buf());
    }
}

fn collect_json_files(directory: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();

        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
}

fn read_mapping(path: &Path) -> Option<ShortcutMapping> {
    let text = fs::read_to_string(path).ok()?;
    let root = Json::parse(&text).ok()?;
    let identifiers = root.get("identifiers")?.array()?;
    let ids = identifiers
        .iter()
        .filter_map(Json::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    if ids.is_empty() {
        return None;
    }

    Some(ShortcutMapping {
        path: path.to_path_buf(),
        ids,
        blank: root.get_bool("blank", false),
    })
}

fn menus_from_json(root: &Json, tool: &str, style: ChordStyle) -> Vec<Menu> {
    let Some(entries) = root.get("menus").and_then(Json::array) else {
        return Vec::new();
    };

    entries
        .iter()
        .filter_map(|menu| {
            let label = menu.get_str("label")?.trim();

            if label.is_empty() {
                return None;
            }

            let items = menu.get("items")?.array()?;
            let entries = entries_from_json(items, tool, style);

            (!entries.is_empty()).then(|| Menu::with_entries(label.to_owned(), entries))
        })
        .collect()
}

fn entries_from_json(items: &[Json], tool: &str, style: ChordStyle) -> Vec<MenuEntry> {
    let mut entries = Vec::new();

    for item in items {
        if item.get_bool("separator", false) {
            if entries
                .last()
                .is_some_and(|entry| !matches!(entry, MenuEntry::Separator))
            {
                entries.push(MenuEntry::Separator);
            }

            continue;
        }

        let Some(label) = item.get_str("label").map(str::trim) else {
            continue;
        };

        if label.is_empty() {
            continue;
        }

        if let Some(children) = item.get("items").and_then(Json::array) {
            let submenu = entries_from_json(children, tool, style);

            if !submenu.is_empty() {
                entries.push(MenuEntry::Item {
                    label: label.to_owned(),
                    enabled: true,
                    accel: None,
                    action: None,
                    submenu,
                });
            }

            continue;
        }

        let shortcut = item.get_str("shortcut").filter(|value| !value.is_empty());
        let action = item.get_str("action").filter(|value| !value.is_empty());

        let entry = if let Some(shortcut) = shortcut {
            match ChordSequence::parse_gtk(shortcut) {
                Some(sequence) => {
                    let command = sequence.command(tool);

                    MenuEntry::Item {
                        label: label.to_owned(),
                        enabled: command.is_some(),
                        accel: sequence.format(style),
                        action: command.map(MenuAction::Command),
                        submenu: Vec::new(),
                    }
                }
                None => MenuEntry::Item {
                    label: label.to_owned(),
                    // Keep the text its author wrote: a mapping problem to see,
                    // not an invitation to invent a different chord.
                    enabled: false,
                    accel: Some(shortcut.to_owned()),
                    action: None,
                    submenu: Vec::new(),
                },
            }
        } else if let Some(action) = action {
            MenuEntry::Item {
                label: label.to_owned(),
                enabled: item.get("target").is_none(),
                accel: None,
                action: (item.get("target").is_none())
                    .then(|| MenuAction::GtkActionName(action.to_owned())),
                submenu: Vec::new(),
            }
        } else {
            // Orbit's placeholders are deliberate: show the row, but do not
            // pretend a mouse gesture or a menu of its own can be activated.
            MenuEntry::Item {
                label: label.to_owned(),
                enabled: false,
                accel: None,
                action: None,
                submenu: Vec::new(),
            }
        };

        entries.push(entry);
    }

    while matches!(entries.last(), Some(MenuEntry::Separator)) {
        entries.pop();
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/shortcutmaps/demo.json");

    fn fixture_maps() -> (tempfile::TempDir, ShortcutMaps) {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("demo.json"), FIXTURE).expect("write fixture");
        let maps = ShortcutMaps::from_paths(vec![directory.path().to_path_buf()]);

        (directory, maps)
    }

    #[test]
    fn imports_menus_nested_submenus_and_sequences() {
        let (_directory, maps) = fixture_maps();
        let menus = maps
            .menus_for("org.example.Demo", "wtype", ChordStyle::Cua)
            .expect("fixture mapping");

        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].label, "File");
        assert_eq!(menus[0].entries.len(), 7);
        assert!(matches!(
            &menus[0].entries[0],
            MenuEntry::Item { label, enabled: true, accel: Some(accel), action: Some(MenuAction::Command(command)), .. }
                if label == "New"
                    && accel == "Ctrl+N"
                    && command == "wtype -M ctrl -k n -m ctrl"
        ));

        let recent = &menus[0].entries[1];

        assert!(matches!(
            recent,
            MenuEntry::Item { label, submenu, .. }
                if label == "Recent" && submenu.len() == 2
        ));
        assert!(matches!(&menus[0].entries[2], MenuEntry::Separator));
        assert!(matches!(
            &menus[0].entries[3],
            MenuEntry::Item { label, enabled: true, action: Some(MenuAction::GtkActionName(action)), .. }
                if label == "Needs a menu" && action == "app.open-menu"
        ));
        assert!(matches!(
            &menus[0].entries[4],
            MenuEntry::Item { label, enabled: true, action: Some(MenuAction::GtkActionName(action)), .. }
                if label == "Plain action" && action == "file-open"
        ));
        assert!(matches!(
            &menus[0].entries[5],
            MenuEntry::Item { label, enabled: false, action: None, .. }
                if label == "Target action"
        ));
        assert!(matches!(
            &menus[0].entries[6],
            MenuEntry::Item { label, enabled: false, action: None, .. }
                if label == "Unavailable"
        ));

        let edit = &menus[1];

        assert_eq!(edit.label, "Edit");
        assert!(matches!(
            &edit.entries[0],
            MenuEntry::Item { label, accel: Some(accel), action: Some(MenuAction::Command(command)), .. }
                if label == "Bracket"
                    && accel == "Ctrl+]"
                    && command == "wtype -M ctrl -k bracketright -m ctrl"
        ));
        assert!(matches!(
            &edit.entries[1],
            MenuEntry::Item { label, accel: Some(accel), .. } if label == "Colon" && accel == ":"
        ));
        assert!(matches!(
            &edit.entries[2],
            MenuEntry::Item { label, accel: Some(accel), .. } if label == "Less" && accel == "<"
        ));
    }

    #[test]
    fn unknown_apps_no_tool_and_blank_maps_have_the_expected_answer() {
        let (_directory, maps) = fixture_maps();

        assert!(
            maps.menus_for("org.example.Unknown", "wtype", ChordStyle::Cua)
                .is_none()
        );
        assert!(
            maps.menus_for("org.example.Demo", "", ChordStyle::Cua)
                .is_none()
        );

        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(
            directory.path().join("blank.json"),
            r#"{"identifiers":["blank"],"blank":true,"menus":[{"label":"File","items":[{"label":"Quit","shortcut":"<Ctrl>Q"}]}]}"#,
        )
        .expect("write blank fixture");
        let blank = ShortcutMaps::from_paths(vec![directory.path().to_path_buf()]);

        assert_eq!(
            blank
                .menus_for("blank", "wtype", ChordStyle::Cua)
                .expect("blank mapping matches")
                .len(),
            0
        );
    }

    #[test]
    fn desktop_suffix_and_explicit_file_paths_match() {
        let (directory, maps) = fixture_maps();
        let menus = maps
            .menus_for("org.example.Demo.desktop", "xdotool", ChordStyle::Cua)
            .expect("desktop suffix");

        assert_eq!(menus.len(), 2);
        assert!(matches!(
            &menus[0].entries[0],
            MenuEntry::Item { action: Some(MenuAction::Command(command)), .. }
                if command == "xdotool key ctrl+n"
        ));

        let from_file = ShortcutMaps::from_paths(vec![directory.path().join("demo.json")]);

        assert_eq!(
            from_file
                .menus_for("demo", "wtype", ChordStyle::Cua)
                .expect("file path")
                .len(),
            2
        );
    }

    #[test]
    fn malformed_mappings_are_skipped_and_duplicate_paths_are_deduped() {
        let directory = tempfile::tempdir().expect("tempdir");

        fs::write(
            directory.path().join("bad.json"),
            "{\"identifiers\": [\"bad\"]",
        )
        .expect("write bad fixture");
        fs::write(directory.path().join("no-ids.json"), "{\"menus\": []}")
            .expect("write missing ids");
        fs::write(directory.path().join("demo.json"), FIXTURE).expect("write fixture");

        let maps = ShortcutMaps::from_paths(vec![
            directory.path().to_path_buf(),
            directory.path().join("demo.json"),
        ]);

        assert!(maps.menus_for("bad", "wtype", ChordStyle::Cua).is_none());
        assert!(maps.menus_for("no-ids", "wtype", ChordStyle::Cua).is_none());
        assert_eq!(
            maps.menus_for("demo", "wtype", ChordStyle::Cua)
                .expect("valid mapping")
                .len(),
            2
        );
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use whelk_rs::launcherdata::{
    ItemKind, file_items, parse_niri_windows, rank_indices, scan_command_dirs, scan_desktop_dirs,
    session_items,
};

fn write_desktop(directory: &PathBuf, name: &str, body: &str) {
    fs::create_dir_all(directory).expect("desktop directory");
    fs::write(directory.join(name), body).expect("desktop file");
}

#[test]
fn a_user_desktop_file_shadows_the_system_file() {
    let root = tempfile::tempdir().expect("desktop root");
    let user = root.path().join("user/applications");
    let system = root.path().join("system/applications");

    write_desktop(
        &user,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=User Editor\nExec=user-editor\n",
    );
    write_desktop(
        &system,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=System Editor\nExec=system-editor\n",
    );
    write_desktop(
        &system,
        "files.desktop",
        "[Desktop Entry]\nType=Application\nName=Files\nIcon=folder\nExec=files\n",
    );
    write_desktop(
        &system,
        "hidden.desktop",
        "[Desktop Entry]\nType=Application\nName=Hidden\nHidden=true\n",
    );
    write_desktop(
        &system,
        "folder.desktop",
        "[Desktop Entry]\nType=Directory\nName=Folder\n",
    );
    fs::write(system.join("notes.txt"), "not a desktop entry").expect("non-desktop file");

    let items = scan_desktop_dirs(&[user, system]);
    let names = items
        .iter()
        .map(|item| item.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names, ["User Editor", "Files"]);
    assert_eq!(items[0].kind, ItemKind::App);
    assert!(items[0].action.ends_with("editor.desktop"));
}

#[test]
fn an_unloadable_user_entry_does_not_hide_the_system_entry() {
    let root = tempfile::tempdir().expect("desktop root");
    let user = root.path().join("user/applications");
    let system = root.path().join("system/applications");

    write_desktop(
        &user,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Hidden Editor\nHidden=true\n",
    );
    write_desktop(
        &system,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=System Editor\nExec=system-editor\n",
    );

    let items = scan_desktop_dirs(&[user, system]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "System Editor");
}

#[test]
fn path_commands_are_executable_files_and_keep_path_order() {
    let root = tempfile::tempdir().expect("command root");
    let first = root.path().join("first");
    let second = root.path().join("second");

    fs::create_dir_all(&first).expect("first directory");
    fs::create_dir_all(&second).expect("second directory");
    fs::write(first.join("tool"), "#!/bin/sh\n").expect("tool");
    fs::set_permissions(first.join("tool"), fs::Permissions::from_mode(0o755))
        .expect("executable bit");
    fs::write(second.join("tool"), "#!/bin/sh\n").expect("duplicate tool");
    fs::set_permissions(second.join("tool"), fs::Permissions::from_mode(0o755))
        .expect("executable bit");
    fs::write(first.join("data"), "not executable").expect("data");
    fs::set_permissions(first.join("data"), fs::Permissions::from_mode(0o644)).expect("file mode");
    fs::write(second.join("data"), "#!/bin/sh\n").expect("later data");
    fs::set_permissions(second.join("data"), fs::Permissions::from_mode(0o755))
        .expect("later executable bit");
    fs::create_dir_all(second.join("directory")).expect("directory");

    let items = scan_command_dirs(&[first.clone(), second.clone()]);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].kind, ItemKind::Command);
    assert_eq!(items[0].name, "tool");
    assert_eq!(items[0].detail, first.to_string_lossy());
    assert_eq!(items[1].name, "data");
    assert_eq!(items[1].detail, second.to_string_lossy());
}

#[test]
fn niri_windows_require_an_id_and_title() {
    let items = parse_niri_windows(
        r#"[
            {"id": 7, "title": "Editor", "app_id": "org.example.Editor"},
            {"id": 0, "title": "No id", "app_id": "ignored"},
            {"id": 9, "title": "", "app_id": "ignored"},
            {"id": 11, "title": "Terminal"}
        ]"#,
        false,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Editor");
    assert_eq!(items[0].detail, "org.example.Editor");
    assert_eq!(items[0].action, "7");
    assert_eq!(items[1].name, "Terminal");
    assert_eq!(items[1].detail, "window");
}

#[test]
fn ranking_puts_word_starts_first_and_keeps_empty_queries() {
    let items = vec![
        whelk_rs::launcherdata::LauncherItem::new(ItemKind::App, "Firefox", ""),
        whelk_rs::launcherdata::LauncherItem::new(ItemKind::App, "File Manager", ""),
        whelk_rs::launcherdata::LauncherItem::new(ItemKind::App, "Terminal", ""),
    ];

    assert_eq!(rank_indices(&items, "fire", 10), [0]);
    assert_eq!(rank_indices(&items, "", 2), [0, 1]);
}

#[test]
fn file_rows_come_from_the_ranked_walk() {
    let items = file_items(
        "report",
        "d\t/home/me/reports\nf\t/home/me/report.txt\nf\t/home/me/archive/report.md\n",
        3,
    );

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].name, "report.txt");
    assert_eq!(items[1].name, "report.md");
    assert_eq!(items[2].name, "reports/");
    assert_eq!(items[2].action, "/home/me/reports");
}

#[test]
fn the_session_menu_is_built_in() {
    let items = session_items();

    assert_eq!(items.len(), 6);
    assert_eq!(items[0].name, "Lock Screen");
    assert_eq!(items[0].kind, ItemKind::Session);
    assert!(items.iter().any(|item| item.name == "Shut Down"));
    assert!(
        items
            .iter()
            .all(|item| !item.action.is_empty() && item.icon.is_some())
    );
}

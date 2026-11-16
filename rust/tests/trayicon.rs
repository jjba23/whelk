// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// JetBrains Toolbox publishes its own colour icon under IconThemePath, while a
// desktop icon theme may offer a monochrome icon with the same name.  The app's
// directory has to be searched first.

use std::fs;
use std::path::Path;

use whelk_rs::trayicon;

fn text(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write fixture");
}

#[test]
fn finds_app_icon() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().to_string_lossy().into_owned();
    let png = directory.path().join("toolbox-tray-color.png");
    let svg = directory.path().join("toolbox-tray-color.svg");

    text(&png, "png");
    text(&svg, "svg");

    assert_eq!(
        trayicon::tray_icon_file(&root, "toolbox-tray-color"),
        Some(png.to_string_lossy().into_owned())
    );

    fs::remove_file(&png).expect("remove png");
    assert_eq!(
        trayicon::tray_icon_file(&root, "toolbox-tray-color"),
        Some(svg.to_string_lossy().into_owned())
    );

    fs::remove_file(&svg).expect("remove svg");
    assert_eq!(trayicon::tray_icon_file(&root, "toolbox-tray-color"), None);
}

#[test]
fn an_exact_name_comes_before_an_extension() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().to_string_lossy().into_owned();
    let exactly = directory.path().join("icon");
    let png = directory.path().join("icon.png");

    text(&exactly, "exact");
    text(&png, "png");

    assert_eq!(
        trayicon::tray_icon_file(&root, "icon"),
        Some(exactly.to_string_lossy().into_owned())
    );
}

#[test]
fn rejects_relative_and_escaping_names() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().to_string_lossy().into_owned();

    assert_eq!(
        trayicon::tray_icon_file(&root, "../toolbox-tray-color"),
        None
    );
    assert_eq!(
        trayicon::tray_icon_file(&root, "/tmp/toolbox-tray-color"),
        None
    );
    assert_eq!(trayicon::tray_icon_file(&root, "dir\\icon"), None);
    assert_eq!(
        trayicon::tray_icon_file("relative-path", "toolbox-tray-color"),
        None
    );
    assert_eq!(trayicon::tray_icon_file(&root, ""), None);
    assert_eq!(trayicon::tray_icon_file("", "toolbox-tray-color"), None);
}

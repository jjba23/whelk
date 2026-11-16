// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::fs;
use std::path::{Path, PathBuf};
use std::slice::from_ref;

use whelk_rs::collection::collect;
use whelk_rs::wallpaper::Kind;

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }

    fs::write(path, b"test").expect("write test file");
}

fn paths(items: &[whelk_rs::collection::WallpaperItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.path.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_walk_finds_recursive_images_and_videos_but_not_noise() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");

    touch(&root.join("b.png"));
    touch(&root.join("a.JPG"));
    touch(&root.join("clip.mp4"));
    touch(&root.join("notes.txt"));
    touch(&root.join(".hidden.png"));
    touch(&root.join("sub/c.webp"));
    touch(&root.join(".hidden-dir/d.png"));
    touch(&root.join("sub/also.webm"));

    let items = collect(from_ref(&root), None, true);
    let found = paths(&items);

    assert_eq!(
        found,
        vec![
            root.join("a.JPG").to_string_lossy().into_owned(),
            root.join("b.png").to_string_lossy().into_owned(),
            root.join("clip.mp4").to_string_lossy().into_owned(),
            root.join("sub/also.webm").to_string_lossy().into_owned(),
            root.join("sub/c.webp").to_string_lossy().into_owned(),
        ]
    );
    assert_eq!(items[0].kind, Kind::Image);
    assert_eq!(items[2].kind, Kind::Video);
}

#[test]
fn video_off_removes_videos_but_keeps_the_stills() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");

    touch(&root.join("still.png"));
    touch(&root.join("movie.mp4"));

    let items = collect(from_ref(&root), None, false);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, root.join("still.png"));
    assert_eq!(items[0].kind, Kind::Image);
}

#[test]
fn a_single_file_root_is_a_collection_of_one() {
    let directory = tempfile::tempdir().expect("tempdir");
    let file = directory.path().join("only.avif");
    touch(&file);

    let items = collect(from_ref(&file), None, true);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, file);
}

#[test]
fn the_current_file_is_included_once_even_outside_the_roots() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");
    let current = directory.path().join("outside/start.png");

    touch(&root.join("inside.png"));
    touch(&current);

    let items = collect(&[root.clone(), root.clone()], Some(&current), true);
    let found = paths(&items);

    assert_eq!(
        found,
        vec![
            current.to_string_lossy().into_owned(),
            root.join("inside.png").to_string_lossy().into_owned(),
        ]
    );
}

#[test]
fn missing_and_unsupported_current_files_are_ignored() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");
    let unsupported = directory.path().join("current.txt");

    touch(&root.join("inside.png"));
    touch(&unsupported);

    let missing = directory.path().join("missing.png");
    let items = collect(from_ref(&root), Some(&missing), true);
    assert_eq!(items.len(), 1);

    let items = collect(&[root], Some(&unsupported), true);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, directory.path().join("walls/inside.png"));
}

#[test]
fn directory_symlinks_are_not_followed_but_file_symlinks_are_kept() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");
    let outside = directory.path().join("outside");
    let file = directory.path().join("real.png");

    touch(&root.join("inside.png"));
    touch(&outside.join("not-in-the-walk.png"));
    touch(&file);

    symlink(&outside, root.join("linked-dir")).expect("directory symlink");
    symlink(&file, root.join("linked-file.png")).expect("file symlink");

    let items = collect(from_ref(&root), None, true);
    let found = paths(&items);

    assert_eq!(
        found,
        vec![
            root.join("inside.png").to_string_lossy().into_owned(),
            root.join("linked-file.png").to_string_lossy().into_owned(),
        ]
    );
}

#[test]
fn the_walk_stops_below_the_c_depth_limit() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().join("walls");
    let mut deep = root.clone();

    for level in 0..14 {
        deep = deep.join(format!("level-{level}"));
    }

    touch(&root.join("shallow.png"));
    touch(&deep.join("too-deep.png"));

    let found = paths(&collect(from_ref(&root), None, true));

    assert_eq!(
        found,
        vec![root.join("shallow.png").to_string_lossy().into_owned()]
    );
}

#[test]
fn a_missing_root_is_simply_empty() {
    let directory = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = directory.path().join("does-not-exist");

    assert!(collect(&[root], None, true).is_empty());
}

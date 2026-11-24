// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::fs;
use std::path::{Path, PathBuf};

use whelk_rs::collection::{WallpaperItem, is_usable};
use whelk_rs::rotation::{Playlist, step_path};
use whelk_rs::wallpaper::{Kind, kind_for_path};

fn item(path: PathBuf) -> WallpaperItem {
    WallpaperItem {
        kind: kind_for_path(&path),
        path,
    }
}

fn files(directory: &Path, names: &[&str]) -> Vec<WallpaperItem> {
    names
        .iter()
        .map(|name| {
            let path = directory.join(name);
            fs::write(&path, b"test").expect("write test file");
            item(path)
        })
        .collect()
}

#[test]
fn an_existing_selection_wins_over_the_configured_start() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png"]);
    let mut playlist = Playlist::new(items.clone(), true, 7);

    playlist.select(&items[1].path);
    playlist.resolve(Some(&items[0].path));

    assert_eq!(playlist.selected(), Some(items[1].path.as_path()));
}

#[test]
fn a_missing_selection_falls_back_to_current_then_the_first_item() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png"]);
    let mut playlist = Playlist::new(items.clone(), true, 7);

    playlist.resolve(Some(&items[1].path));
    assert_eq!(playlist.selected(), Some(items[1].path.as_path()));

    let mut empty = Playlist::new(items.clone(), true, 7);
    empty.resolve(None);
    assert_eq!(empty.selected(), Some(items[0].path.as_path()));

    let mut without_items = Playlist::new(Vec::new(), true, 7);
    without_items.resolve(None);
    assert_eq!(without_items.selected(), None);
}

#[test]
fn an_explicit_selection_may_live_outside_the_collection() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["inside.png"]);
    let outside = directory.path().join("outside.png");
    fs::write(&outside, b"test").expect("write outside");

    let mut playlist = Playlist::new(items.clone(), true, 7);
    playlist.resolve(Some(&outside));

    assert_eq!(playlist.selected(), Some(outside.as_path()));

    // The C engine starts the lookup at index zero when the selected file is
    // outside the collection; it does not invent a new order.
    assert_eq!(playlist.advance(false), Some(items[0].path.as_path()));
}

#[test]
fn rotation_wraps_in_order() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png", "c.png"]);
    let mut playlist = Playlist::new(items.clone(), true, 7);

    playlist.resolve(Some(&items[0].path));

    assert_eq!(playlist.advance(false), Some(items[1].path.as_path()));
    assert_eq!(playlist.advance(false), Some(items[2].path.as_path()));
    assert_eq!(playlist.advance(false), Some(items[0].path.as_path()));
}

#[test]
fn shuffled_rotation_never_repeats_and_is_reproducible() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(
        directory.path(),
        &["a.png", "b.png", "c.png", "d.png", "e.png"],
    );

    let mut first = Playlist::new(items.clone(), true, 42);
    let mut second = Playlist::new(items.clone(), true, 42);
    first.resolve(Some(&items[0].path));
    second.resolve(Some(&items[0].path));

    let mut previous = items[0].path.clone();

    for _ in 0..40 {
        let next = first.advance(true).expect("next item").to_owned();
        let other = second.advance(true).expect("same next item").to_owned();

        assert_ne!(next, previous);
        assert_eq!(next, other);
        previous = next;
    }
}

#[test]
fn rotation_needs_two_items_a_clock_and_a_selection() {
    let directory = tempfile::tempdir().expect("tempdir");
    let one = files(directory.path(), &["a.png"]);
    let two = files(directory.path(), &["a.png", "b.png"]);

    let one = {
        let mut playlist = Playlist::new(one, true, 1);
        playlist.resolve(None);
        playlist
    };
    let two = {
        let mut playlist = Playlist::new(two, true, 1);
        playlist.resolve(None);
        playlist
    };

    assert!(!one.should_rotate(true, 900));
    assert!(!two.should_rotate(false, 900));
    assert!(!two.should_rotate(true, 0));
    assert!(two.should_rotate(true, 900));
    assert!(two.should_rotate(true, 1));
}

#[test]
fn a_deleted_current_file_does_not_stop_rotation() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png"]);
    let mut playlist = Playlist::new(items.clone(), true, 7);

    playlist.resolve(Some(&items[0].path));
    fs::remove_file(&items[0].path).expect("delete current");

    // The collection still contains the old item, as it does in C until the
    // next rescan; the clock must advance rather than stop on the missing file.
    assert!(playlist.should_rotate(true, 900));
    assert_eq!(playlist.advance(false), Some(items[1].path.as_path()));
    assert!(playlist.should_rotate(true, 900));
}

#[test]
fn selection_rejects_files_the_video_policy_cannot_show() {
    let directory = tempfile::tempdir().expect("tempdir");
    let image = directory.path().join("still.png");
    let video = directory.path().join("movie.mp4");
    let unsupported = directory.path().join("notes.txt");
    fs::write(&image, b"test").expect("image");
    fs::write(&video, b"test").expect("video");
    fs::write(&unsupported, b"test").expect("notes");

    let mut playlist = Playlist::new(Vec::new(), false, 1);

    assert!(is_usable(&image, false));
    assert!(playlist.select(&image));
    assert!(!playlist.select(&video));
    assert!(!playlist.select(&unsupported));
    assert_eq!(playlist.selected(), Some(image.as_path()));

    let mut with_video = Playlist::new(Vec::new(), true, 1);
    assert!(with_video.select(&video));
    assert_eq!(with_video.selected(), Some(video.as_path()));
}

#[test]
fn a_menu_row_steps_the_collection_in_both_directions() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png", "c.png"]);

    assert_eq!(
        step_path(&items, None, 1, false, 1),
        Some(items[1].path.clone())
    );
    assert_eq!(
        step_path(&items, Some(&items[1].path), 1, false, 1),
        Some(items[2].path.clone())
    );
    assert_eq!(
        step_path(&items, Some(&items[1].path), -1, false, 1),
        Some(items[0].path.clone()),
        "previous is a signed step, not a second playlist"
    );
    assert_eq!(
        step_path(&items, Some(&items[0].path), -1, false, 1),
        Some(items[2].path.clone()),
        "the first picture's previous is the last"
    );
    assert_eq!(
        step_path(&items, Some(&items[2].path), 1, false, 1),
        Some(items[0].path.clone())
    );
    assert_eq!(step_path(&[], None, 1, false, 1), None);
}

#[test]
fn a_shuffle_step_is_a_different_picture() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.png", "c.png", "d.png"]);

    for seed in 0..32 {
        let next = step_path(&items, Some(&items[1].path), 1, true, seed).expect("a picture");

        assert_ne!(
            next, items[1].path,
            "shuffle must move away from the current"
        );
        assert!(items.iter().any(|item| item.path == next));
    }
}

#[test]
fn items_keep_their_kind() {
    let directory = tempfile::tempdir().expect("tempdir");
    let items = files(directory.path(), &["a.png", "b.mp4"]);
    let playlist = Playlist::new(items.clone(), true, 1);

    assert_eq!(playlist.items()[0].kind, Kind::Image);
    assert_eq!(playlist.items()[1].kind, Kind::Video);
}

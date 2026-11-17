// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The MPRIS reading, without a bus: what a player's metadata says, which
//! player a bar follows, and how its time is printed.

use gtk4::glib;
use whelk_rs::mpris::{
    Playback, choose, format_time, parse_metadata, player_name, progress_percent,
};

fn variant(variant_type: &str, text: &str) -> glib::Variant {
    let variant_type = glib::VariantTy::new(variant_type).expect("known variant type");

    glib::Variant::parse(Some(variant_type), text).expect("test variant parses")
}

fn players(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_track_is_read_from_the_metadata_players_publish() {
    let metadata = variant(
        "a{sv}",
        "{'mpris:trackid': <objectpath '/org/mpris/track/7'>,
          'mpris:length': <int64 180000000>,
          'mpris:artUrl': <'file:///tmp/cover.png'>,
          'xesam:title': <'Nightcall'>,
          'xesam:artist': <['Kavinsky', 'Lovefoxxx']>,
          'xesam:album': <'OutRun'>}",
    );
    let track = parse_metadata(&metadata);

    assert_eq!(track.title, "Nightcall");
    assert_eq!(track.artist, "Kavinsky, Lovefoxxx");
    assert_eq!(track.album, "OutRun");
    assert_eq!(track.art_url.as_deref(), Some("file:///tmp/cover.png"));
    assert_eq!(track.length_us, 180_000_000);
    assert_eq!(track.id, "/org/mpris/track/7");
}

#[test]
fn a_player_that_says_little_still_says_something() {
    // A browser video: a title, no album, no length, no cover.
    let metadata = variant("a{sv}", "{'xesam:title': <'A video'>}");
    let track = parse_metadata(&metadata);

    assert_eq!(track.title, "A video");
    assert_eq!(track.artist, "");
    assert_eq!(track.album, "");
    assert_eq!(track.art_url, None);
    assert_eq!(track.length_us, 0);
}

#[test]
fn one_artist_published_as_a_string_is_read_as_one_artist() {
    let metadata = variant("a{sv}", "{'xesam:artist': <'One Artist'>}");

    assert_eq!(parse_metadata(&metadata).artist, "One Artist");
}

#[test]
fn empty_artists_and_odd_length_types_do_not_break_the_reading() {
    let metadata = variant(
        "a{sv}",
        "{'xesam:artist': <['', 'Solo']>, 'mpris:length': <uint64 42000000>}",
    );
    let track = parse_metadata(&metadata);

    assert_eq!(track.artist, "Solo");
    assert_eq!(track.length_us, 42_000_000);
}

#[test]
fn a_metadata_that_is_not_a_dictionary_is_an_empty_track() {
    let metadata = variant("s", "'not metadata'");

    assert_eq!(parse_metadata(&metadata).title, "");
}

#[test]
fn a_bus_name_is_reduced_to_the_player() {
    assert_eq!(player_name("org.mpris.MediaPlayer2.firefox"), "firefox");
    assert_eq!(
        player_name("org.mpris.MediaPlayer2.plasma-browser-integration"),
        "plasma-browser-integration"
    );
    assert_eq!(player_name("org.example.Other"), "org.example.Other");
}

#[test]
fn auto_follows_the_player_that_last_played() {
    let bus = players(&[
        "org.mpris.MediaPlayer2.firefox",
        "org.mpris.MediaPlayer2.spotify",
    ]);

    // Nothing has played yet: the first player answers.
    assert_eq!(
        choose(&bus, None, None).map(String::as_str),
        Some("org.mpris.MediaPlayer2.firefox")
    );

    // The browser played, then the music did: the music wins.
    assert_eq!(
        choose(&bus, Some("org.mpris.MediaPlayer2.spotify"), Some("auto")).map(String::as_str),
        Some("org.mpris.MediaPlayer2.spotify")
    );

    // A remembered player that is gone falls back to the first.
    assert_eq!(
        choose(&bus, Some("org.mpris.MediaPlayer2.vlc"), None).map(String::as_str),
        Some("org.mpris.MediaPlayer2.firefox")
    );
    assert_eq!(choose(&[], None, None), None);
}

#[test]
fn a_named_player_is_the_one_followed() {
    let bus = players(&[
        "org.mpris.MediaPlayer2.firefox",
        "org.mpris.MediaPlayer2.spotify",
    ]);

    assert_eq!(
        choose(&bus, None, Some("spotify")).map(String::as_str),
        Some("org.mpris.MediaPlayer2.spotify")
    );
    assert_eq!(
        choose(&bus, None, Some("org.mpris.MediaPlayer2.firefox")).map(String::as_str),
        Some("org.mpris.MediaPlayer2.firefox")
    );
    // A name that is not on the bus is not silently replaced by another
    // player: a bar that says it shows VLC must not show Spotify.
    assert_eq!(choose(&bus, None, Some("vlc")), None);
}

#[test]
fn a_players_time_reads_like_a_counter() {
    assert_eq!(format_time(0), "0:00");
    assert_eq!(format_time(5_000_000), "0:05");
    assert_eq!(format_time(65_000_000), "1:05");
    assert_eq!(format_time(3_725_000_000), "1:02:05");
    assert_eq!(format_time(-1), "0:00");
}

#[test]
fn progress_is_a_whole_percent_of_the_track() {
    assert_eq!(progress_percent(0, 0), 0);
    assert_eq!(progress_percent(1_000_000, 0), 0);
    assert_eq!(progress_percent(30_000_000, 120_000_000), 25);
    assert_eq!(progress_percent(60_000_000, 60_000_000), 100);
    // A seek past the end is still a bar, not 140%.
    assert_eq!(progress_percent(84_000_000, 60_000_000), 100);
}

#[test]
fn the_playback_state_decides_the_word_and_the_glyph() {
    assert_eq!(Playback::parse("Playing"), Playback::Playing);
    assert_eq!(Playback::parse("Paused"), Playback::Paused);
    assert_eq!(Playback::parse("Stopped"), Playback::Stopped);
    assert_eq!(Playback::parse("Buffering"), Playback::Stopped);

    assert_eq!(Playback::Playing.name(), "playing");
    assert_eq!(Playback::Paused.name(), "paused");
    assert!(Playback::Playing.is_playing());
    assert_ne!(Playback::Playing.icon(), Playback::Paused.icon());
}

#[test]
fn only_a_local_cover_becomes_a_picture() {
    assert_eq!(
        whelk_rs::mpris::art_path("file:///tmp/cover.png"),
        Some(std::path::PathBuf::from("/tmp/cover.png"))
    );
    assert_eq!(whelk_rs::mpris::art_path("https://example.com/a.png"), None);
    assert_eq!(whelk_rs::mpris::art_path(""), None);
}

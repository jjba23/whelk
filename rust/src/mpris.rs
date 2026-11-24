// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! MPRIS, natively: the player a bar shows and the three buttons it needs.
//!
//! The default now-playing tile is a `custom` module that shells out to
//! `playerctl` every two seconds.  This is the same reading without the
//! subprocess: the session bus already publishes every player as
//! `org.mpris.MediaPlayer2.<name>`, so whelk watches the names, keeps one
//! `GDBusProxy` per player with its properties cached, and calls the player's
//! own `PlayPause`, `Next` and `Previous` methods when a module is clicked.
//!
//! Two halves, deliberately: [`parse_metadata`], [`choose`], [`format_time`] and
//! the rest are pure and are what the tests pin; [`Mpris`] is the live half that
//! owns the proxies and tells the bar when a reading moved.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

/// Every player owns one bus name under this prefix.
pub const PLAYER_PREFIX: &str = "org.mpris.MediaPlayer2.";
/// The object every player exports its player interface on.
pub const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
/// The interface with the metadata, the state and the controls.
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
/// The interface `Position` has to be asked for through.
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
pub const DBUS_NAME: &str = "org.freedesktop.DBus";
pub const DBUS_PATH: &str = "/org/freedesktop/DBus";
/// A player that does not answer within this many milliseconds is silent, which
/// is a bar that keeps working rather than a bar that waits.
const CALL_TIMEOUT_MS: i32 = 2_000;
/// `#:player "auto"`: follow whichever player last reported it was playing.
pub const AUTO: &str = "auto";

const PLAY_ICON: &str = "\u{f04b}";
const PAUSE_ICON: &str = "\u{f04c}";
const STOP_ICON: &str = "\u{f04d}";

/// The three states MPRIS defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Playback {
    /// `PlaybackStatus` as a value, with anything unexpected read as stopped:
    /// a player that says something else is not playing.
    pub fn parse(text: &str) -> Self {
        match text {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }

    /// The word a format can show as `{state}`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    /// The glyph a format shows as `{icon}` when the configuration named none.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Playing => PLAY_ICON,
            Self::Paused => PAUSE_ICON,
            Self::Stopped => STOP_ICON,
        }
    }

    pub fn is_playing(self) -> bool {
        self == Self::Playing
    }
}

/// What one player says about the track it is on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    /// `mpris:trackid`, as text: the identity a player keeps across updates.
    pub id: String,
    pub title: String,
    /// Every artist, joined with a comma: MPRIS publishes a list.
    pub artist: String,
    pub album: String,
    /// `mpris:artUrl`.  A local `file://` URL is what the art picture can
    /// show; a remote one is left for a tile that wants to fetch it.
    pub art_url: Option<String>,
    /// `mpris:length` in microseconds; zero when the player does not say.
    pub length_us: i64,
}

/// The reading a bar module paints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    /// The player being followed, without the bus-name prefix.
    pub player: Option<String>,
    pub track: Track,
    pub playback: Playback,
    /// Where the player is, in microseconds.  Zero before the first answer.
    pub position_us: i64,
    /// How many players are on the bus, so a tooltip can say so.
    pub players: usize,
}

impl Reading {
    /// A bar hides a module with nothing to say; this is how it asks.
    pub fn available(&self) -> bool {
        self.player.is_some()
    }
}

/// The controls a module offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Previous,
    PlayPause,
    Next,
    Stop,
}

impl Control {
    /// The MPRIS method behind the control.
    pub fn method(self) -> &'static str {
        match self {
            Self::Previous => "Previous",
            Self::PlayPause => "PlayPause",
            Self::Next => "Next",
            Self::Stop => "Stop",
        }
    }
}

/// `org.mpris.MediaPlayer2.firefox` as `firefox`.
pub fn player_name(bus_name: &str) -> &str {
    bus_name.strip_prefix(PLAYER_PREFIX).unwrap_or(bus_name)
}

/// Read `Metadata` (`a{sv}`) into a [`Track`].
///
/// Every field is optional in practice: a browser video has a title and no
/// album, a stream has no length, and a player that just started has neither.
/// A missing or mistyped field is empty rather than an error, because a bar with
/// a title and no artist is still a bar.
pub fn parse_metadata(metadata: &glib::Variant) -> Track {
    let mut track = Track::default();

    let Some(values) = metadata.get::<HashMap<String, glib::Variant>>() else {
        return track;
    };

    if let Some(value) = values.get("xesam:title") {
        track.title = text(value);
    }

    if let Some(value) = values.get("xesam:artist") {
        // `xesam:artist` is `as`.  A player that publishes one string instead
        // is read as one artist rather than dropped.
        track.artist = match value.get::<Vec<String>>() {
            Some(artists) => artists
                .into_iter()
                .filter(|artist| !artist.trim().is_empty())
                .collect::<Vec<String>>()
                .join(", "),
            None => text(value),
        };
    }

    if let Some(value) = values.get("xesam:album") {
        track.album = text(value);
    }

    if let Some(value) = values.get("mpris:artUrl") {
        let url = text(value);

        if !url.is_empty() {
            track.art_url = Some(url);
        }
    }

    if let Some(value) = values.get("mpris:trackid") {
        // `mpris:trackid` is an object path, which is not a string variant: its
        // printed form is the path a caller can compare.
        track.id = value
            .get::<String>()
            .unwrap_or_else(|| value.print(false).to_string());
    }

    if let Some(value) = values.get("mpris:length") {
        // The spec says `x`; a player that answers `u` is read anyway.
        track.length_us = value
            .get::<i64>()
            .or_else(|| value.get::<u64>().map(|length| length as i64))
            .unwrap_or(0)
            .max(0);
    }

    track
}

fn text(value: &glib::Variant) -> String {
    value.get::<String>().unwrap_or_default()
}

/// Which player the module follows.
///
/// `wanted` is `#:player`: an exact player name, or `None`/`"auto"` for the
/// player that last reported it was playing, remembered in `remembered`.  When
/// nothing is playing (or the remembered player is gone), the first player on
/// the bus answers, so a paused player still shows its track.
pub fn choose<'a>(
    players: &'a [String],
    remembered: Option<&str>,
    wanted: Option<&str>,
) -> Option<&'a String> {
    let wanted = wanted
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != AUTO);

    if let Some(wanted) = wanted {
        return players
            .iter()
            .find(|name| name.as_str() == wanted || player_name(name) == wanted);
    }

    if let Some(remembered) = remembered
        && let Some(found) = players.iter().find(|name| name.as_str() == remembered)
    {
        return Some(found);
    }

    players.first()
}

/// `{position}` and `{length}`: `3:07`, or `1:02:09` once there is an hour.
pub fn format_time(microseconds: i64) -> String {
    let seconds = microseconds.max(0) / 1_000_000;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// `{progress}`: whole percent of the track, zero when the length is unknown.
pub fn progress_percent(position_us: i64, length_us: i64) -> u32 {
    if length_us <= 0 {
        return 0;
    }

    let fraction = position_us.max(0) as f64 / length_us as f64;

    (fraction * 100.0).clamp(0.0, 100.0).round() as u32
}

// ---------------------------------------------------------------------------
// The live half
// ---------------------------------------------------------------------------

type Change = dyn Fn(&Reading) + 'static;

struct Player {
    /// The well-known bus name, prefix included.
    name: String,
    proxy: gio::DBusProxy,
}

struct Inner {
    /// `#:player`, already reduced to `None` for automatic.
    wanted: Option<String>,
    /// The player that last said it was playing.
    remembered: Option<String>,
    /// The last position read, and the player it was read from.
    position: Option<(String, i64)>,
    players: Vec<Player>,
    /// Kept so dropping the last reference stops the name watch.
    subscription: Option<gio::SignalSubscription>,
    on_change: Rc<Change>,
}

/// A live view of the session bus's players.
///
/// One of these per module: it owns the proxies, so it lives exactly as long as
/// the widget that draws the reading, and it calls `on_change` whenever
/// anything it shows may have moved.
pub struct Mpris {
    inner: Rc<RefCell<Inner>>,
}

impl Mpris {
    /// Follow the bus, and call `on_change` with the current reading whenever
    /// it moves.
    pub fn start<F>(wanted: Option<String>, on_change: F) -> Rc<Self>
    where
        F: Fn(&Reading) + 'static,
    {
        let wanted = wanted
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty() && name != AUTO);
        let inner = Rc::new(RefCell::new(Inner {
            wanted,
            remembered: None,
            position: None,
            players: Vec::new(),
            subscription: None,
            on_change: Rc::new(on_change),
        }));
        let mpris = Rc::new(Self {
            inner: Rc::clone(&inner),
        });
        let weak = Rc::downgrade(&inner);

        gio::bus_get(
            gio::BusType::Session,
            gio::Cancellable::NONE,
            move |result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };

                match result {
                    Ok(bus) => watch_names(&inner, &bus),
                    Err(error) => {
                        eprintln!("whelk: mpris: no session bus, so no player: {error}");
                    }
                }
            },
        );

        mpris
    }

    /// The reading as it stands, without asking the player anything.
    pub fn reading(&self) -> Reading {
        reading_of(&self.inner.borrow())
    }

    /// Ask the active player where it is.
    ///
    /// `Position` is the one property players do not announce: it changes
    /// without a `PropertiesChanged`, so a bar that wants `{position}` has to
    /// ask.  Every other part of the reading arrives as a signal.
    pub fn poll_position(&self) {
        let Some((name, proxy)) = active(&self.inner.borrow()) else {
            return;
        };
        let weak = Rc::downgrade(&self.inner);

        proxy.call(
            PROPERTIES_INTERFACE,
            Some(&(PLAYER_INTERFACE, "Position").to_variant()),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
            move |result| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };

                if let Ok(reply) = result
                    && let Some((value,)) = reply.get::<(glib::Variant,)>()
                    && let Some(position) = value.get::<i64>()
                {
                    inner.borrow_mut().position = Some((name.clone(), position));
                }

                notify(&inner);
            },
        );
    }

    /// Call the active player's control.  A failed call is a player that went
    /// away between the click and the method; the watch will drop it.
    pub fn control(&self, control: Control) {
        let Some((_, proxy)) = active(&self.inner.borrow()) else {
            return;
        };

        proxy.call(
            control.method(),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
            |_result| {},
        );
    }

    /// Move the active player to `position_us`.
    ///
    /// MPRIS has two ways to do this.  A player that publishes a track id takes
    /// `SetPosition`, which says exactly where to go; one that does not takes a
    /// relative `Seek`, so the delta is measured from the last position read.
    /// The local position moves immediately so a slider under the user's finger
    /// does not snap back while the player answers, and the next poll corrects
    /// it if the player refused.
    pub fn seek_to(&self, position_us: i64) {
        let position_us = position_us.max(0);
        let Some((name, proxy, track_id, current_us)) = ({
            let inner = self.inner.borrow();

            active(&inner).map(|(name, proxy)| {
                let track_id = proxy
                    .cached_property("Metadata")
                    .map(|metadata| parse_metadata(&metadata).id)
                    .unwrap_or_default();

                (name, proxy, track_id, reading_of(&inner).position_us)
            })
        }) else {
            return;
        };

        let (method, parameters) =
            match glib::Variant::parse(Some(glib::VariantTy::OBJECT_PATH), &track_id) {
                Ok(track) => ("SetPosition", (track, position_us).to_variant()),
                Err(_) => ("Seek", (position_us - current_us).to_variant()),
            };

        self.inner.borrow_mut().position = Some((name, position_us));
        notify(&self.inner);

        proxy.call(
            method,
            Some(&parameters),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
            |_result| {},
        );
    }

    /// Let the active player's own time line catch up after a pause or a seek.
    pub fn refresh(&self) {
        notify(&self.inner);
    }

    /// The active player's bus name, prefix included.
    pub fn active_name(&self) -> Option<String> {
        active(&self.inner.borrow()).map(|(name, _proxy)| name)
    }
}

fn reading_of(inner: &Inner) -> Reading {
    let mut reading = Reading {
        players: inner.players.len(),
        ..Reading::default()
    };
    let Some((name, proxy)) = active(inner) else {
        return reading;
    };

    reading.player = Some(player_name(&name).to_owned());

    if let Some((from, position)) = inner.position.as_ref()
        && from == &name
    {
        reading.position_us = *position;
    }

    if let Some(metadata) = proxy.cached_property("Metadata") {
        reading.track = parse_metadata(&metadata);
    }

    if let Some(status) = proxy.cached_property("PlaybackStatus") {
        reading.playback = Playback::parse(&status.get::<String>().unwrap_or_default());
    }

    reading
}

fn active(inner: &Inner) -> Option<(String, gio::DBusProxy)> {
    let names: Vec<String> = inner
        .players
        .iter()
        .map(|player| player.name.clone())
        .collect();
    let chosen = choose(&names, inner.remembered.as_deref(), inner.wanted.as_deref())?.clone();

    inner
        .players
        .iter()
        .find(|player| player.name == chosen)
        .map(|player| (player.name.clone(), player.proxy.clone()))
}

fn notify(inner: &Rc<RefCell<Inner>>) {
    // The callback repaints, so it must not run with the borrow held: a module
    // is free to ask the same view for its reading while it paints.
    let (reading, on_change) = {
        let inner = inner.borrow();

        (reading_of(&inner), Rc::clone(&inner.on_change))
    };

    on_change(&reading);
}

fn watch_names(inner: &Rc<RefCell<Inner>>, bus: &gio::DBusConnection) {
    let weak = Rc::downgrade(inner);
    let signal_bus = bus.clone();
    let subscription = bus.subscribe_to_signal(
        Some(DBUS_NAME),
        Some(DBUS_NAME),
        Some("NameOwnerChanged"),
        Some(DBUS_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let Some((name, _old, new)) = signal.parameters.get::<(String, String, String)>()
            else {
                return;
            };

            if !name.starts_with(PLAYER_PREFIX) {
                return;
            }

            if new.is_empty() {
                remove_player(&inner, &name);
                notify(&inner);
            } else {
                add_player(&inner, &signal_bus, &name);
            }
        },
    );

    inner.borrow_mut().subscription = Some(subscription);

    // The players that were already running are not announced again: one
    // `ListNames` at startup is what lets a bar started after the music see it.
    let weak = Rc::downgrade(inner);
    let list_bus = bus.clone();

    bus.call(
        Some(DBUS_NAME),
        DBUS_PATH,
        DBUS_NAME,
        "ListNames",
        None,
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT_MS,
        gio::Cancellable::NONE,
        move |result: Result<glib::Variant, glib::Error>| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let Ok(reply) = result else {
                return;
            };
            let Some((names,)) = reply.get::<(Vec<String>,)>() else {
                return;
            };

            for name in names
                .into_iter()
                .filter(|name| name.starts_with(PLAYER_PREFIX))
            {
                add_player(&inner, &list_bus, &name);
            }
        },
    );
}

fn add_player(inner: &Rc<RefCell<Inner>>, bus: &gio::DBusConnection, name: &str) {
    if inner
        .borrow()
        .players
        .iter()
        .any(|player| player.name == name)
    {
        return;
    }

    let weak = Rc::downgrade(inner);
    let bus_name = name.to_owned();

    gio::DBusProxy::new(
        bus,
        gio::DBusProxyFlags::NONE,
        None,
        Some(name),
        PLAYER_PATH,
        PLAYER_INTERFACE,
        gio::Cancellable::NONE,
        move |result| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let proxy = match result {
                Ok(proxy) => proxy,
                Err(error) => {
                    eprintln!("whelk: mpris: {bus_name} could not be watched: {error}");

                    return;
                }
            };

            // The handler holds a weak reference to the proxy it belongs to:
            // the proxy owns the handler, so a strong one would keep the player
            // alive past the bar that asked for it.
            let weak_proxy: glib::object::WeakRef<gio::DBusProxy> = proxy.downgrade();
            let weak_inner = Rc::downgrade(&inner);
            let handler_name = bus_name.clone();

            proxy.connect_local("g-properties-changed", false, move |_| {
                if let Some(inner) = weak_inner.upgrade() {
                    if let Some(proxy) = weak_proxy.upgrade()
                        && let Some(status) = proxy.cached_property("PlaybackStatus")
                        && Playback::parse(&status.get::<String>().unwrap_or_default()).is_playing()
                    {
                        inner.borrow_mut().remembered = Some(handler_name.clone());
                    }

                    notify(&inner);
                }

                None
            });

            {
                let mut inner = inner.borrow_mut();

                if inner.players.iter().any(|player| player.name == bus_name) {
                    return;
                }

                inner.players.push(Player {
                    name: bus_name,
                    proxy,
                });
            }

            notify(&inner);
        },
    );
}

fn remove_player(inner: &Rc<RefCell<Inner>>, name: &str) {
    let mut inner = inner.borrow_mut();

    inner.players.retain(|player| player.name != name);

    if inner.remembered.as_deref() == Some(name) {
        inner.remembered = None;
    }

    if inner
        .position
        .as_ref()
        .is_some_and(|(from, _position)| from == name)
    {
        inner.position = None;
    }
}

/// `file://` art as a path; anything else is a URL a picture cannot open.
pub fn art_path(url: &str) -> Option<std::path::PathBuf> {
    let file = gio::File::for_uri(url);

    file.path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_name_loses_its_prefix() {
        assert_eq!(player_name("org.mpris.MediaPlayer2.spotify"), "spotify");
        assert_eq!(player_name("something-else"), "something-else");
    }

    #[test]
    fn positions_read_like_a_player_counter() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(65_000_000), "1:05");
        assert_eq!(format_time(3_725_000_000), "1:02:05");
        assert_eq!(format_time(-5), "0:00");
        assert_eq!(progress_percent(0, 0), 0);
        assert_eq!(progress_percent(30_000_000, 120_000_000), 25);
        assert_eq!(progress_percent(999_000_000, 120_000_000), 100);
    }

    #[test]
    fn the_unknown_playback_state_is_stopped() {
        assert_eq!(Playback::parse("Playing"), Playback::Playing);
        assert_eq!(Playback::parse("Paused"), Playback::Paused);
        assert_eq!(Playback::parse("Stopped"), Playback::Stopped);
        assert_eq!(Playback::parse("Buffering"), Playback::Stopped);
        assert!(!Playback::parse("Buffering").is_playing());
    }
}

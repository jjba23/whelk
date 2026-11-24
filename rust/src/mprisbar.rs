// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The now-playing bar module.
//!
//! A tile that says what is playing and moves when you click it, backed by the
//! player's own MPRIS interface: no `playerctl` subprocess, no polling of one
//! process per player, and controls that go to the player rather than to a
//! command that has to find it again.
//!
//! `#:player "auto"` follows whichever player last reported it was playing, so
//! a bar switches from a paused video to the music that starts behind it.
//! Clicking the tile is `PlayPause`; the middle button is next and the right
//! button is previous, with the wheel doing the same two.  A configuration that
//! named `#:on-click`, `#:on-click-middle` or `#:on-click-right` keeps its own
//! command for that button.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::format;
use crate::mpris::{self, Control, Mpris, Reading};

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
pub const DEFAULT_FORMAT: &str = "{icon} {title}";
pub const DEFAULT_MAX_LENGTH: i32 = 40;
pub const DEFAULT_ART_SIZE: i32 = 16;
const DEFAULT_TOOLTIP: &str =
    "{title}\n{artist} — {album}\n{position} / {length}  ({progress})\nplayer: {player}";

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let spec = module.clone();
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    // The cover goes before the reading, so it is appended before the host
    // adds the label.
    let art = art_picture(module);

    if let (Some(picture), Some(module_root)) = (art.as_ref(), host.root()) {
        module_root.append(picture);
    }

    if let Some(label) = host.add_label() {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_xalign(0.0);
    }

    let last_art: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let wanted = module.text("player");
    let weak_host = host.downgrade();
    let paint_spec = spec.clone();
    let paint_art = art.clone();
    let paint_last_art = Rc::clone(&last_art);

    let mpris = Mpris::start(wanted, move |reading| {
        let Some(host) = weak_host.upgrade() else {
            return;
        };

        paint(&paint_spec, &host, &paint_art, &paint_last_art, reading);
    });

    // Paint the empty reading now: the bus answers asynchronously, and a module
    // that never painted would sit on the bar as a blank gap.
    paint(&spec, &host, &art, &last_art, &mpris.reading());

    attach_controls(&root, &host, &mpris);

    let weak_mpris = Rc::downgrade(&mpris);
    host.start(move |_host| {
        // The proxies tell the bar about every change that is announced; the
        // position is the one thing that has to be asked for.
        if let Some(mpris) = weak_mpris.upgrade() {
            mpris.poll_position();
        }
    });

    // The module owns the view; the view owns the proxies.  Dropping the root
    // must therefore drop the view, which is what unsubscribes from the bus.
    let view = Rc::new(RefCell::new(Some(mpris)));

    root.connect_destroy(move |_| drop(view.borrow_mut().take()));

    Some(root)
}

/// A compact transport applet: previous, play/pause and next as three flat
/// buttons in the bar.
///
/// It is the same [`Mpris`] engine as the now-playing tile, so it follows the
/// configured player and changes its play mark when the player does.  With no
/// player the buttons stay in place but are insensitive, which keeps the bar's
/// layout still while a media player starts or stops.
pub fn controls_module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );
    let previous = gtk::Button::with_label("⏮");
    let play = gtk::Button::with_label("▶");
    let next = gtk::Button::with_label("⏭");

    root.add_css_class("whelk-media-controls");

    for (button, tooltip) in [
        (&previous, "Previous track"),
        (&play, "Play or pause"),
        (&next, "Next track"),
    ] {
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tooltip));
        root.append(button);
    }

    root.set_sensitive(false);

    let weak_host = host.downgrade();
    let weak_play = play.downgrade();
    let mpris = Mpris::start(module.text("player"), move |reading| {
        if let Some(play) = weak_play.upgrade() {
            play.set_label(if reading.playback.is_playing() {
                "⏸"
            } else {
                "▶"
            });
        }

        let Some(host) = weak_host.upgrade() else {
            return;
        };
        let Some(root) = host.root() else {
            return;
        };

        root.set_sensitive(reading.available());

        let title = reading.track.title.trim();
        let artist = reading.track.artist.trim();
        let tooltip = if !reading.available() {
            "No media player".to_owned()
        } else if artist.is_empty() {
            title.to_owned()
        } else if title.is_empty() {
            artist.to_owned()
        } else {
            format!("{title}\n{artist}")
        };

        root.set_tooltip_text(Some(&tooltip));
    });

    for (button, control) in [
        (&previous, Control::Previous),
        (&play, Control::PlayPause),
        (&next, Control::Next),
    ] {
        let mpris = Rc::clone(&mpris);

        button.connect_clicked(move |_| mpris.control(control));
    }

    let weak_mpris = Rc::downgrade(&mpris);
    host.start(move |_host| {
        if let Some(mpris) = weak_mpris.upgrade() {
            mpris.poll_position();
        }
    });

    let view = Rc::new(RefCell::new(Some(mpris)));

    root.connect_destroy(move |_| drop(view.borrow_mut().take()));

    Some(root)
}

fn art_picture(module: &ModuleSpec) -> Option<gtk::Picture> {
    if !module.boolean("art?").unwrap_or(false) {
        return None;
    }

    let size = module
        .integer("art-size")
        .unwrap_or(i64::from(DEFAULT_ART_SIZE))
        .clamp(8, 128) as i32;
    let picture = gtk::Picture::new();

    picture.add_css_class("whelk-art");
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    picture.set_size_request(size, size);
    picture.set_valign(gtk::Align::Center);
    picture.set_visible(false);

    Some(picture)
}

fn paint(
    module: &ModuleSpec,
    host: &ModuleHost,
    art: &Option<gtk::Picture>,
    last_art: &Rc<RefCell<Option<PathBuf>>>,
    reading: &Reading,
) {
    let show_off = module.boolean("show-off?").unwrap_or(false);

    if !reading.available() && !show_off {
        host.set_visible(false);

        if let Some(picture) = art {
            picture.set_visible(false);
        }

        return;
    }

    host.set_visible(true);

    let max_length = module
        .integer("max-length")
        .unwrap_or(i64::from(DEFAULT_MAX_LENGTH))
        .clamp(0, i64::from(i32::MAX)) as i32;
    let title = if max_length > 0 {
        format::truncate_utf8(&reading.track.title, max_length)
    } else {
        reading.track.title.clone()
    };
    let player = reading.player.clone().unwrap_or_default();
    let art_url = reading.track.art_url.clone().unwrap_or_default();
    let position = mpris::format_time(reading.position_us);
    let length = mpris::format_time(reading.track.length_us);
    let progress = format!(
        "{}%",
        mpris::progress_percent(reading.position_us, reading.track.length_us)
    );
    let values = [
        ("player", player.as_str()),
        ("title", title.as_str()),
        ("artist", reading.track.artist.as_str()),
        ("album", reading.track.album.as_str()),
        ("state", reading.playback.name()),
        ("position", position.as_str()),
        ("length", length.as_str()),
        ("progress", progress.as_str()),
        ("art", art_url.as_str()),
    ];

    let icon = module
        .text("icon")
        .unwrap_or_else(|| reading.playback.icon().to_owned());
    host.present_with(DEFAULT_FORMAT, &icon, Some(DEFAULT_TOOLTIP), &values);

    if let Some(picture) = art {
        paint_art(picture, last_art, reading.track.art_url.as_deref());
    }
}

fn paint_art(picture: &gtk::Picture, last_art: &Rc<RefCell<Option<PathBuf>>>, url: Option<&str>) {
    let path = url.and_then(mpris::art_path);

    if path.is_none() {
        picture.set_visible(false);

        return;
    }

    if last_art.borrow().as_ref() != path.as_ref() {
        if let Some(path) = path.as_ref() {
            picture.set_filename(Some(path));
        }

        *last_art.borrow_mut() = path;
    }

    picture.set_visible(true);
}

fn attach_controls(root: &gtk::Box, host: &ModuleHost, mpris: &Rc<Mpris>) {
    let actions = &host.settings().actions;
    let mut pointer = false;

    for (button, control, configured) in [
        (
            gtk::gdk::BUTTON_PRIMARY,
            Control::PlayPause,
            actions.primary.is_some(),
        ),
        (
            gtk::gdk::BUTTON_MIDDLE,
            Control::Next,
            actions.middle.is_some(),
        ),
        (
            gtk::gdk::BUTTON_SECONDARY,
            Control::Previous,
            actions.secondary.is_some(),
        ),
    ] {
        if configured {
            // The configuration's own command already runs on this button; a
            // second gesture would play music *and* run it.
            continue;
        }

        attach_press(root, button, mpris, control);
        pointer = true;
    }

    if !actions.has_scroll() {
        attach_wheel(root, mpris);
        pointer = true;
    }

    if pointer {
        root.set_cursor_from_name(Some("pointer"));
    }
}

fn attach_press(root: &gtk::Box, button: u32, mpris: &Rc<Mpris>, control: Control) {
    let gesture = gtk::GestureClick::new();
    let mpris = Rc::clone(mpris);

    gesture.set_button(button);
    gesture.connect_pressed(move |_, _, _, _| {
        mpris.control(control);
    });
    root.add_controller(gesture);
}

fn attach_wheel(root: &gtk::Box, mpris: &Rc<Mpris>) {
    let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let mpris = Rc::clone(mpris);

    controller.connect_scroll(move |_, _, dy| {
        if dy < 0.0 {
            mpris.control(Control::Next);
        } else if dy > 0.0 {
            mpris.control(Control::Previous);
        }

        gtk::glib::Propagation::Stop
    });
    root.add_controller(controller);
}

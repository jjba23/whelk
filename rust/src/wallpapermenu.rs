// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper menu, ported from the collection half of `modules/wallpaper.c`.
//!
//! One card of thumbnails, opened where the user asked for it: a right click on
//! an empty desktop, or the bar's wallpaper reading.  A click on a picture
//! applies it through the same preference and repaint the picker uses, and
//! *Browse all* hands a collection too big for the menu to
//! [`crate::wallpaperpicker`].
//!
//! The card is its own layer surface, cut to the card, for the same reason the
//! launcher is: a compositor that blurs a surface blurs its geometry, so a
//! fullscreen overlay would blur the whole output every time a thumbnail
//! arrives.  It is placed with layer-shell margins -- at the pointer for a
//! desktop click, under the reading for the bar -- and `placement_near_point`
//! and `placement_under` are the pure half of that, tested without a display.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::collection;
use crate::settings::WallpaperSettings;
use crate::style;
use crate::wallpaper::Kind;
use crate::wallpaperpicker::{self, ThumbRequest, Thumbs};

/// The pixels between the pointer (or the bar) and the card.
const GAP: i32 = 6;
/// The card keeps four pictures to a row at this size: a menu, not the picker.
const COLUMNS: usize = 4;
const THUMB_WIDTH: i32 = 112;
const THUMB_HEIGHT: i32 = 63;
const COLUMN_GAP: i32 = 6;
/// Three rows fit a screen without turning the menu into the picker; anything
/// past that is behind *Browse all*.
const MAX_ROWS: usize = 3;

const MENU_CSS: &str = "
.whelk-wallpaper-menu {
  padding: 10px 12px 8px 12px;
  border-radius: 14px;
}
.whelk-wallpaper-menu-title {
  font-weight: 800;
}
.whelk-wallpaper-menu-count {
  opacity: 0.55;
  font-size: 0.85em;
}
.whelk-wallpaper-menu-hint {
  opacity: 0.55;
  font-size: 0.84em;
}
.whelk-wallpaper-menu-warning {
  opacity: 0.72;
  font-size: 0.84em;
}
.whelk-wallpaper-menu-empty {
  opacity: 0.7;
  padding: 10px 2px;
}
.whelk-wallpaper-menu-tile {
  padding: 0;
  min-width: 0;
  min-height: 0;
  border-radius: 8px;
  border: 1px solid alpha(currentColor, 0.14);
}
.whelk-wallpaper-menu-tile:hover {
  border-color: alpha(currentColor, 0.42);
  background-color: alpha(currentColor, 0.08);
}
.whelk-wallpaper-menu-tile.current {
  border-color: alpha(currentColor, 0.72);
  box-shadow: 0 0 0 2px alpha(currentColor, 0.18);
}
.whelk-wallpaper-menu-thumb {
  border-radius: 7px;
  background-color: alpha(black, 0.22);
}
.whelk-wallpaper-menu-video {
  opacity: 0.85;
  font-size: 1.2em;
}
";

thread_local! {
    static CURRENT: RefCell<Option<WallpaperMenu>> = const { RefCell::new(None) };
}

/// How many pictures the menu shows before the rest are behind *Browse all*.
pub fn capacity() -> usize {
    COLUMNS * MAX_ROWS
}

/// Whether a collection is too big for the menu.
pub fn needs_browser(items: usize) -> bool {
    items > capacity()
}

/// Keep a card inside the output: `(left, top)` is the corner it would like.
pub fn clamp_inside(output: (i32, i32), card: (i32, i32), left: i32, top: i32) -> (i32, i32) {
    (
        left.clamp(0, (output.0 - card.0).max(0)),
        top.clamp(0, (output.1 - card.1).max(0)),
    )
}

/// The corner for a card near a pointer: the pointer is its top-left with the
/// gap, and a card that would hang off the bottom (or the right) flips to the
/// other side of the pointer instead of being pushed under it.
pub fn placement_near_point(output: (i32, i32), card: (i32, i32), point: (i32, i32)) -> (i32, i32) {
    let left = if point.0 + GAP + card.0 > output.0 {
        point.0 - GAP - card.0
    } else {
        point.0 + GAP
    };
    let top = if point.1 + GAP + card.1 > output.1 {
        point.1 - GAP - card.1
    } else {
        point.1 + GAP
    };

    clamp_inside(output, card, left, top)
}

/// The corner for a card that belongs to a bar reading: centred on `center_x`,
/// below `edge_y` (a top bar) or above it (a bottom bar).
pub fn placement_under(
    output: (i32, i32),
    card: (i32, i32),
    center_x: i32,
    edge_y: i32,
    above: bool,
) -> (i32, i32) {
    let left = center_x - card.0 / 2;
    let top = if above {
        edge_y - card.1 - GAP
    } else {
        edge_y + GAP
    };

    clamp_inside(output, card, left, top)
}

pub fn install(menu: Option<WallpaperMenu>) {
    CURRENT.with(|current| *current.borrow_mut() = menu);
}

pub fn current() -> Option<WallpaperMenu> {
    CURRENT.with(|current| current.borrow().clone())
}

/// Close the menu if one is open.  The desktop surface calls this for a click
/// that is not a right click, so any click on the background dismisses it.
pub fn hide_current() {
    if let Some(menu) = current() {
        menu.hide();
    }
}

struct Inner {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    list: gtk::Box,
    count: gtk::Label,
    empty: gtk::Label,
    warning: gtk::Label,
    browse: gtk::Button,
    close: gtk::Button,
    settings: RefCell<WallpaperSettings>,
    thumbs: Rc<Thumbs>,
    css: RefCell<Option<gtk::CssProvider>>,
    on_choose: Rc<dyn Fn(&Path)>,
    on_browse: Rc<dyn Fn()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(css) = self.css.borrow_mut().take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

#[derive(Clone)]
pub struct WallpaperMenu {
    inner: Rc<Inner>,
}

impl WallpaperMenu {
    pub fn new(
        application: &gtk::Application,
        style: &BarSettings,
        settings: &WallpaperSettings,
        on_choose: Rc<dyn Fn(&Path)>,
        on_browse: Rc<dyn Fn()>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk wallpaper")
            .build();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Wallpaper"));
        let count = gtk::Label::new(None);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let empty = gtk::Label::new(Some(
            "No wallpapers found.\nAdd images to the directories in #:paths.",
        ));
        let warning = gtk::Label::new(Some(
            "whelk is not drawing the background (#:enabled? is off).",
        ));
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let browse = gtk::Button::with_label("Browse all…");
        let hint = gtk::Label::new(Some("Click one to use it · Esc closes"));

        window.set_widget_name(&style::widget_id("wallpaper-menu"));
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(true);

        card.add_css_class("whelk-wallpaper-menu");
        header.add_css_class("whelk-wallpaper-menu-header");
        title.add_css_class("whelk-wallpaper-menu-title");
        count.add_css_class("whelk-wallpaper-menu-count");
        close.add_css_class("flat");
        close.add_css_class("whelk-wallpaper-menu-close");
        close.set_tooltip_text(Some("Close (Esc)"));
        empty.add_css_class("whelk-wallpaper-menu-empty");
        empty.set_wrap(true);
        empty.set_visible(false);
        warning.add_css_class("whelk-wallpaper-menu-warning");
        warning.set_wrap(true);
        warning.set_visible(false);
        browse.add_css_class("flat");
        browse.set_halign(gtk::Align::Start);
        hint.add_css_class("whelk-wallpaper-menu-hint");
        hint.set_halign(gtk::Align::End);
        hint.set_hexpand(true);
        hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
        spacer.set_hexpand(true);

        header.append(&title);
        header.append(&count);
        header.append(&spacer);
        header.append(&close);
        footer.append(&browse);
        footer.append(&hint);
        card.append(&header);
        card.append(&list);
        card.append(&empty);
        card.append(&warning);
        card.append(&footer);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Start);
        window.set_child(Some(&card));

        let menu = Self {
            inner: Rc::new(Inner {
                window,
                card,
                list,
                count,
                empty,
                warning,
                browse,
                close,
                settings: RefCell::new(settings.clone()),
                thumbs: Thumbs::new(),
                css: RefCell::new(None),
                on_choose,
                on_browse,
            }),
        };

        menu.configure_surface();
        menu.set_style(style);
        menu.connect_controls();

        menu
    }

    fn configure_surface(&self) {
        let window = self.inner.window.clone();

        if crate::ui::layer_shell_available() {
            window.init_layer_shell();
            window.set_namespace(Some("whelk-rust-wallpaper-menu"));
            window.set_layer(Layer::Overlay);
            // Top and left only, and the surface is the card: the compositor's
            // blur covers the menu instead of the output.
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
        }

        window.present();
        window.set_visible(false);
    }

    fn connect_controls(&self) {
        let window = self.inner.window.clone();
        let close = self.inner.close.clone();

        close.connect_clicked(move |_| window.set_visible(false));

        let browse_window = self.inner.window.clone();
        let browse = self.inner.browse.clone();
        let on_browse = Rc::clone(&self.inner.on_browse);

        browse.connect_clicked(move |_| {
            browse_window.set_visible(false);
            on_browse();
        });

        // A card with nothing to type in has nobody to hand the keyboard to, so
        // Escape is the keyboard's way out and the close button the mouse's.
        let window = self.inner.window.clone();
        let keys = gtk::EventControllerKey::new();

        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                window.set_visible(false);

                return gtk::glib::Propagation::Stop;
            }

            gtk::glib::Propagation::Proceed
        });
        self.inner.window.add_controller(keys);
    }

    pub fn set_style(&self, style: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };

        if let Some(previous) = self.inner.css.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        let mut styled = style.clone();

        if styled.radius < 14 {
            styled.radius = 14;
        }

        let widget_id = style::widget_id("wallpaper-menu");
        let generated = style::css(&widget_id, &styled, "whelk-wallpaper-menu", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{MENU_CSS}\n{generated}"));
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *self.inner.css.borrow_mut() = Some(css);
    }

    /// A reload replaces the collection and the current picture, so the menu
    /// reads them again the next time it opens.
    pub fn set_settings(&self, settings: &WallpaperSettings) {
        *self.inner.settings.borrow_mut() = settings.clone();
    }

    pub fn is_visible(&self) -> bool {
        self.inner.window.is_visible()
    }

    pub fn hide(&self) {
        self.inner.window.set_visible(false);
    }

    /// The full picker, for a collection the menu cannot show at once.
    pub fn browse_all(&self) {
        self.hide();
        (self.inner.on_browse)();
    }

    /// Open the card at a point on `monitor`, in output-relative pixels: what a
    /// right click on an empty desktop asks for.
    pub fn show_at_point(&self, monitor: &gtk::gdk::Monitor, point: (i32, i32)) {
        let geometry = monitor.geometry();
        let output = (geometry.width(), geometry.height());

        self.refresh();

        let card = self.measure_card();

        self.place(
            card,
            placement_near_point(output, card, point),
            Some(monitor),
        );
    }

    /// Open the card at a bar reading: `center_x` is the reading's middle,
    /// `edge_y` the bar's outer edge, and `above` a bar at the bottom.
    pub fn show_under(&self, monitor: &gtk::gdk::Monitor, center_x: i32, edge_y: i32, above: bool) {
        let geometry = monitor.geometry();
        let output = (geometry.width(), geometry.height());

        self.refresh();

        let card = self.measure_card();
        let corner = placement_under(output, card, center_x, edge_y, above);

        self.place(card, corner, Some(monitor));
    }

    /// The size the card actually needs, which is what the layer surface is cut
    /// to: a guessed height would leave the card floating in a taller surface.
    fn measure_card(&self) -> (i32, i32) {
        let card = &self.inner.card;

        card.set_size_request(-1, -1);

        let (_, width, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
        let width = width.max(1);
        let (_, height, _, _) = card.measure(gtk::Orientation::Vertical, width);

        (width, height.max(1))
    }

    fn place(&self, card: (i32, i32), corner: (i32, i32), monitor: Option<&gtk::gdk::Monitor>) {
        let window = &self.inner.window;

        self.inner.card.set_size_request(card.0, card.1);
        window.set_default_size(card.0, card.1);

        if crate::ui::layer_shell_available() {
            window.set_margin(Edge::Left, corner.0);
            window.set_margin(Edge::Top, corner.1);

            if let Some(monitor) = monitor {
                window.set_monitor(Some(monitor));
            }
        }

        window.set_visible(true);
        window.present();
    }

    /// Rebuild the grid and return how many pictures are in it.
    fn refresh(&self) -> usize {
        let settings = self.inner.settings.borrow().clone();
        let items =
            collection::collect(&settings.paths, settings.current.as_deref(), settings.video);

        self.inner.thumbs.reset();

        while let Some(child) = self.inner.list.first_child() {
            self.inner.list.remove(&child);
        }

        self.inner
            .count
            .set_text(&wallpaperpicker::summary(items.len()));
        self.inner.empty.set_visible(items.is_empty());
        self.inner
            .warning
            .set_visible(!items.is_empty() && !settings.enabled);
        self.inner.browse.set_visible(needs_browser(items.len()));

        let shown = items.len().min(capacity());

        for chunk in items[..shown].chunks(COLUMNS) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, COLUMN_GAP);

            for item in chunk {
                let current = settings.current.as_deref() == Some(item.path.as_path());

                row.append(&self.tile(&item.path, item.kind, current));
            }

            self.inner.list.append(&row);
        }

        items.len()
    }

    fn tile(&self, path: &Path, kind: Kind, current: bool) -> gtk::Button {
        let button = gtk::Button::new();

        button.add_css_class("flat");
        button.add_css_class("whelk-wallpaper-menu-tile");
        button.set_size_request(THUMB_WIDTH, THUMB_HEIGHT);
        button.set_tooltip_text(Some(&wallpaperpicker::tile_name(&path.to_string_lossy())));

        if current {
            button.add_css_class("current");
        }

        if kind == Kind::Video {
            // A video tile would be one frame pretending to be a still, so it
            // says what it is instead.
            let mark = gtk::Label::new(Some("▶"));

            mark.add_css_class("whelk-wallpaper-menu-video");
            mark.set_halign(gtk::Align::Center);
            mark.set_valign(gtk::Align::Center);
            button.set_child(Some(&mark));
        } else {
            let picture = gtk::Picture::new();

            picture.add_css_class("whelk-wallpaper-menu-thumb");
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_size_request(THUMB_WIDTH, THUMB_HEIGHT);
            button.set_child(Some(&picture));
            self.inner.thumbs.push(ThumbRequest {
                picture: picture.downgrade(),
                path: path.to_path_buf(),
                width: THUMB_WIDTH,
                height: THUMB_HEIGHT,
                generation: self.inner.thumbs.generation(),
            });
        }

        let chosen = path.to_path_buf();
        let on_choose = Rc::clone(&self.inner.on_choose);
        let inner = Rc::clone(&self.inner);

        button.connect_clicked(move |_| {
            inner.window.set_visible(false);
            on_choose(&chosen);
        });

        button
    }
}

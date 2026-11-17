// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper picker, ported from `src/wallpaper_picker.c`.
//!
//! One overlay surface with a card: the collection as a grid of thumbnails,
//! F11 to expand it to the output, Escape or a click outside to close, and a
//! click on a tile choosing that picture.  Thumbnails are decoded from an idle
//! queue so opening the picker never waits on a directory of 4K files, and a
//! generation counter means a rebuild drops the answers the previous grid
//! asked for.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gdk_pixbuf::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::collection;
use crate::settings::WallpaperSettings;
use crate::style;
use crate::ui::layer_shell_available;
use crate::wallpaper::Kind;

/// The picker's own structural rules; the material comes from `style::css`.
const PICKER_CSS: &str = "
    window#whelk-wallpaper-picker {
        background-color: rgba(0, 0, 0, 0.35);
    }
    .whelk-wallpaper-picker {
        padding: 18px;
    }
    .whelk-wallpaper-picker-header {
        margin-bottom: 4px;
    }
    .whelk-wallpaper-picker-title {
        font-weight: 800;
        font-size: 17px;
    }
    .whelk-wallpaper-picker-count {
        opacity: 0.6;
        font-size: 12px;
    }
    .whelk-wallpaper-picker-warning {
        opacity: 0.85;
        font-size: 12px;
    }
    .whelk-wallpaper-tile {
        padding: 6px;
        border-radius: 10px;
        background-color: transparent;
    }
    .whelk-wallpaper-tile:hover {
        background-color: alpha(currentColor, 0.08);
    }
    .whelk-wallpaper-tile.selected {
        outline: 2px solid alpha(currentColor, 0.55);
    }
    .whelk-wallpaper-tile-name {
        font-size: 12px;
        opacity: 0.85;
    }
    .whelk-wallpaper-thumb {
        border-radius: 8px;
        background-color: alpha(currentColor, 0.08);
    }
    .whelk-wallpaper-thumb-missing {
        background-image: linear-gradient(45deg, alpha(currentColor, 0.10) 25%, transparent 25%);
    }
    .whelk-wallpaper-video-mark {
        font-size: 22px;
    }
    .whelk-wallpaper-video-kind {
        opacity: 0.6;
        font-size: 11px;
    }
    .whelk-wallpaper-picker-empty {
        opacity: 0.7;
    }
    .whelk-wallpaper-picker-hint {
        opacity: 0.5;
        font-size: 11px;
    }
";

// ---------------------------------------------------------------------------
// Policy that needs no display
// ---------------------------------------------------------------------------

/// The name a tile wears: the file's base name without its extension.
pub fn tile_name(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    let base = if base.is_empty() { path } else { base };

    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => base.to_owned(),
    }
}

/// The header's count: `"1 wallpaper"`, `"3 wallpapers"`.
pub fn summary(count: usize) -> String {
    if count == 1 {
        "1 wallpaper".to_owned()
    } else {
        format!("{count} wallpapers")
    }
}

/// The thumbnail size a card of `card_width` with `columns` columns leaves:
/// the configured width less 32 px of padding a side and 12 px between the
/// columns, with a floor the tile stays readable at.
pub fn thumb_size(card_width: i32, columns: i32) -> (i32, i32) {
    let columns = columns.max(1);
    let width = (card_width - 64 - (columns - 1) * 12) / columns;

    (width.max(96), (width.max(96) * 9 / 16).max(72))
}

/// The card's size: the configured one until it expands, never larger than the
/// monitor it opens on.
pub fn card_size(
    expanded: bool,
    configured: (i32, i32),
    monitor: Option<(i32, i32)>,
) -> (i32, i32) {
    let Some((monitor_width, monitor_height)) = monitor else {
        return configured;
    };
    let limit_width = (monitor_width - 96).max(320);
    let limit_height = (monitor_height - 96).max(240);

    if expanded {
        (limit_width, limit_height)
    } else {
        (
            configured.0.min(limit_width),
            configured.1.min(limit_height),
        )
    }
}

// ---------------------------------------------------------------------------
// Thumbnails
// ---------------------------------------------------------------------------

/// One thumbnail to decode.  Shared with the wallpaper menu, which shows the
/// same pictures at its own size through the same one-per-idle queue.
pub(crate) struct ThumbRequest {
    pub(crate) picture: gtk::glib::WeakRef<gtk::Picture>,
    pub(crate) path: PathBuf,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) generation: u64,
}

/// The queue behind the grid.  One idle decodes one picture; the next idle is
/// scheduled only once it is done, so neither the picker nor the menu blocks on
/// a directory of 4K files.
pub(crate) struct Thumbs {
    queue: RefCell<VecDeque<ThumbRequest>>,
    scheduled: Cell<bool>,
    generation: Rc<Cell<u64>>,
}

impl Thumbs {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            queue: RefCell::new(VecDeque::new()),
            scheduled: Cell::new(false),
            generation: Rc::new(Cell::new(0)),
        })
    }

    pub(crate) fn push(self: &Rc<Self>, request: ThumbRequest) {
        self.queue.borrow_mut().push_back(request);
        self.pump_soon();
    }

    /// The generation a request must carry to still be wanted.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn pump_soon(self: &Rc<Self>) {
        if self.scheduled.replace(true) {
            return;
        }

        let thumbs = Rc::clone(self);

        gtk::glib::idle_add_local_once(move || {
            thumbs.scheduled.set(false);

            let request = thumbs.queue.borrow_mut().pop_front();

            if let Some(request) = request {
                decode(&request, thumbs.generation.get());
            }

            if !thumbs.queue.borrow().is_empty() {
                thumbs.pump_soon();
            }
        });
    }

    /// Drop what the previous grid asked for and start a new generation.
    pub(crate) fn reset(&self) {
        self.generation.set(self.generation.get() + 1);
        self.queue.borrow_mut().clear();
    }
}

pub(crate) fn decode(request: &ThumbRequest, generation: u64) {
    if request.generation != generation {
        return;
    }

    let Some(picture) = request.picture.upgrade() else {
        return;
    };

    // The pixbuf loader has the format plugins and can decode at the size the
    // tile needs; GDK's own loader is the second opinion for a format this
    // build of pixbuf does not know.
    if let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_file_at_scale(
        &request.path,
        request.width * 2,
        request.height * 2,
        true,
    ) {
        picture.set_paintable(Some(&texture_from(&pixbuf)));

        return;
    }

    match gtk::gdk::Texture::from_filename(&request.path) {
        Ok(texture) => picture.set_paintable(Some(&texture)),
        Err(_) => picture.add_css_class("whelk-wallpaper-thumb-missing"),
    }
}

pub(crate) fn texture_from(pixbuf: &gdk_pixbuf::Pixbuf) -> gtk::gdk::Texture {
    // `for_pixbuf` copies the pixels into a texture of the format they already
    // have, so a three-channel JPEG needs no alpha pass here.
    gtk::gdk::Texture::for_pixbuf(pixbuf)
}

// ---------------------------------------------------------------------------
// The picker
// ---------------------------------------------------------------------------

/// The picker's surface and its card.  It is created hidden and presented on
/// demand, so the shell pays for it once and a reload only re-reads the style
/// and the collection.
pub struct Picker {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    flow: gtk::FlowBox,
    scroller: gtk::ScrolledWindow,
    count: gtk::Label,
    warning: gtk::Label,
    empty: gtk::Label,
    expand_button: gtk::Button,
    css: RefCell<Option<gtk::CssProvider>>,
    /// The settings the card was last shown with; the key controller resizes
    /// from them.
    settings: Rc<RefCell<WallpaperSettings>>,
    expanded: Rc<Cell<bool>>,
    thumbs: Rc<Thumbs>,
    on_choose: Rc<dyn Fn(&Path)>,
}

impl Picker {
    pub fn new(
        application: &gtk::Application,
        style: &BarSettings,
        settings: &WallpaperSettings,
        on_choose: Rc<dyn Fn(&Path)>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk wallpaper")
            .build();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let title = gtk::Label::new(Some("Wallpaper"));
        let count = gtk::Label::new(None);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let expand = gtk::Button::from_icon_name("view-fullscreen-symbolic");
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        let warning = gtk::Label::new(Some(
            "whelk is not drawing a background.  Turn on #:enabled? in \
             (wallpaper-settings …); drawing also needs a wlr-layer-shell \
             compositor.",
        ));
        let flow = gtk::FlowBox::new();
        let scroller = gtk::ScrolledWindow::new();
        let empty = gtk::Label::new(Some(
            "No wallpapers found.\nAdd PNG, JPEG, WebP, AVIF or GIF images, or a video, to the \
             directories in #:paths.",
        ));
        let hint = gtk::Label::new(Some(
            "Click a wallpaper to use it · F11 expands · Esc closes",
        ));
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);

        window.set_widget_name("whelk-wallpaper-picker");
        window.set_decorated(false);
        card.add_css_class("whelk-wallpaper-picker");
        header.add_css_class("whelk-wallpaper-picker-header");
        title.add_css_class("whelk-wallpaper-picker-title");
        count.add_css_class("whelk-wallpaper-picker-count");
        warning.add_css_class("whelk-wallpaper-picker-warning");
        empty.add_css_class("whelk-wallpaper-picker-empty");
        hint.add_css_class("whelk-wallpaper-picker-hint");
        scroller.add_css_class("whelk-wallpaper-scroller");
        flow.add_css_class("whelk-wallpaper-grid");

        for button in [&expand, &close] {
            button.add_css_class("flat");
            button.set_focusable(false);
        }

        close.set_tooltip_text(Some("Close (Esc)"));
        expand.set_tooltip_text(Some("Expand or restore (F11)"));
        spacer.set_hexpand(true);
        header.append(&title);
        header.append(&count);
        header.append(&spacer);
        header.append(&expand);
        header.append(&close);

        warning.set_wrap(true);
        warning.set_halign(gtk::Align::Start);
        warning.set_visible(false);

        flow.set_homogeneous(true);
        flow.set_column_spacing(12);
        flow.set_row_spacing(12);
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_halign(gtk::Align::Center);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&flow));
        scroller.set_vexpand(true);

        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        empty.set_vexpand(true);
        empty.set_visible(false);
        hint.set_halign(gtk::Align::Start);

        card.append(&header);
        card.append(&warning);
        card.append(&scroller);
        card.append(&empty);
        card.append(&hint);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);

        holder.set_hexpand(true);
        holder.set_vexpand(true);
        holder.append(&card);
        window.set_child(Some(&holder));

        if layer_shell_available() {
            window.init_layer_shell();
            window.set_layer(Layer::Overlay);
            window.set_namespace(Some("whelk-wallpaper-picker"));
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Right, true);
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Left, true);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
            window.set_exclusive_zone(-1);
        } else {
            window.set_default_size(960, 640);
        }

        let picker = Self {
            window,
            card,
            flow,
            scroller,
            count,
            warning,
            empty,
            expand_button: expand,
            css: RefCell::new(None),
            settings: Rc::new(RefCell::new(settings.clone())),
            expanded: Rc::new(Cell::new(false)),
            thumbs: Thumbs::new(),
            on_choose,
        };

        picker.wire_events();
        picker.set_style(style);
        picker.window.set_visible(false);

        picker
    }

    fn wire_events(&self) {
        let window = self.window.clone();
        let card = self.card.clone();
        let flow = self.flow.clone();
        let expand_button = self.expand_button.clone();
        let expanded = Rc::clone(&self.expanded);
        let settings = Rc::clone(&self.settings);
        let keys = gtk::EventControllerKey::new();

        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let toggle = match key {
                gtk::gdk::Key::F11 => true,
                gtk::gdk::Key::f | gtk::gdk::Key::F => {
                    modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                }
                gtk::gdk::Key::Escape => {
                    window.set_visible(false);

                    return gtk::glib::Propagation::Stop;
                }
                _ => false,
            };

            if !toggle {
                return gtk::glib::Propagation::Proceed;
            }

            let now = !expanded.get();

            expanded.set(now);
            apply_size(
                &window,
                &card,
                &flow,
                &expand_button,
                &settings.borrow(),
                now,
            );

            gtk::glib::Propagation::Stop
        });
        self.window.add_controller(keys);

        // A click outside the card closes the overlay; inside it belongs to the
        // tile or button that received it.
        {
            let window = self.window.clone();
            let card = self.card.clone();
            let click = gtk::GestureClick::new();

            click.set_button(gtk::gdk::BUTTON_PRIMARY);
            click.connect_pressed(move |_, _, x, y| {
                let Some(bounds) = card.compute_bounds(&window) else {
                    return;
                };

                let outside = x < f64::from(bounds.x())
                    || y < f64::from(bounds.y())
                    || x > f64::from(bounds.x() + bounds.width())
                    || y > f64::from(bounds.y() + bounds.height());

                if outside {
                    window.set_visible(false);
                }
            });
            self.window.add_controller(click);
        }

        // Enter on the focused tile is the keyboard's answer to a click on it.
        {
            let window = self.window.clone();
            let on_choose = Rc::clone(&self.on_choose);

            self.flow.connect_child_activated(move |_, child| {
                let Some(path) = child.child().and_then(|tile| tile_path(&tile)) else {
                    return;
                };

                on_choose(Path::new(&path));
                window.set_visible(false);
            });
        }

        // The header's two buttons: close hides the overlay, expand toggles
        // the card between its configured size and the output.
        {
            let window = self.window.clone();

            if let Some(header) = self.card.first_child().and_downcast::<gtk::Box>()
                && let Some(close) = header.last_child().and_downcast::<gtk::Button>()
            {
                close.connect_clicked(move |_| window.set_visible(false));
            }
        }

        {
            let window = self.window.clone();
            let card = self.card.clone();
            let flow = self.flow.clone();
            let expand_button = self.expand_button.clone();
            let expanded = Rc::clone(&self.expanded);
            let settings = Rc::clone(&self.settings);

            self.expand_button.connect_clicked(move |_| {
                let now = !expanded.get();

                expanded.set(now);
                apply_size(
                    &window,
                    &card,
                    &flow,
                    &expand_button,
                    &settings.borrow(),
                    now,
                );
            });
        }
    }

    /// Apply the bar's material to the picker's skin, with the card radius
    /// floored so a small bar radius does not clip the overlay's corners.
    pub fn set_style(&self, style: &BarSettings) {
        if let Some(css) = self.css.borrow_mut().take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        let mut styled = style.clone();

        if styled.radius < 14 {
            styled.radius = 14;
        }

        let generated = style::css(
            "whelk-wallpaper-picker",
            &styled,
            "whelk-wallpaper-picker",
            None,
        );
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{PICKER_CSS}\n{generated}"));

        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        *self.css.borrow_mut() = Some(css);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Show the grid for `settings`.  A reload replaces those settings, so the
    /// card is resized and the collection re-walked on every show.
    pub fn show(&self, settings: &WallpaperSettings) {
        *self.settings.borrow_mut() = settings.clone();
        apply_size(
            &self.window,
            &self.card,
            &self.flow,
            &self.expand_button,
            settings,
            self.expanded.get(),
        );
        self.rebuild(settings);
        self.window.set_visible(true);
        self.window.present();

        // An overlay with exclusive keyboard must land on something a key can
        // reach; the first tile is what the arrow keys navigate.
        if let Some(first) = self.flow.first_child() {
            first.grab_focus();
        } else {
            self.expand_button.grab_focus();
        }
    }

    pub fn toggle(&self, settings: &WallpaperSettings) {
        if self.is_visible() {
            self.hide();
        } else {
            self.show(settings);
        }
    }

    fn rebuild(&self, settings: &WallpaperSettings) {
        self.thumbs.reset();

        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }

        let items =
            collection::collect(&settings.paths, settings.current.as_deref(), settings.video);
        let (thumb_width, thumb_height) = thumb_size(
            i32::try_from(settings.picker_width).unwrap_or(960),
            i32::try_from(settings.picker_columns).unwrap_or(4),
        );

        self.count.set_text(&summary(items.len()));
        self.empty.set_visible(items.is_empty());
        self.scroller.set_visible(!items.is_empty());
        self.warning
            .set_visible(!items.is_empty() && !settings.enabled);

        for item in &items {
            let path = item.path.to_string_lossy();
            let tile = self.tile(&path, item.kind, thumb_width, thumb_height);

            if settings.current.as_deref() == Some(item.path.as_path()) {
                tile.add_css_class("selected");
            }

            self.flow.append(&tile);
        }
    }

    fn tile(&self, path: &str, kind: Kind, thumb_width: i32, thumb_height: i32) -> gtk::Button {
        let button = gtk::Button::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let name = gtk::Label::new(Some(&tile_name(path)));

        button.add_css_class("whelk-wallpaper-tile");
        body.add_css_class("whelk-wallpaper-tile-box");
        button.set_valign(gtk::Align::Start);
        name.add_css_class("whelk-wallpaper-tile-name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_max_width_chars(22);
        name.set_halign(gtk::Align::Center);

        if kind == Kind::Video {
            let mark = gtk::Label::new(Some("▶"));
            let kind_label = gtk::Label::new(Some("Video"));
            let thumb = gtk::Box::new(gtk::Orientation::Vertical, 0);

            mark.add_css_class("whelk-wallpaper-video-mark");
            kind_label.add_css_class("whelk-wallpaper-video-kind");
            thumb.add_css_class("whelk-wallpaper-thumb");
            thumb.add_css_class("whelk-wallpaper-video-thumb");
            thumb.set_size_request(thumb_width, thumb_height);
            mark.set_halign(gtk::Align::Center);
            mark.set_valign(gtk::Align::End);
            thumb.append(&mark);
            thumb.append(&kind_label);
            body.append(&thumb);
        } else {
            let picture = gtk::Picture::new();

            picture.add_css_class("whelk-wallpaper-thumb");
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_size_request(thumb_width, thumb_height);
            body.append(&picture);
            self.thumbs.push(ThumbRequest {
                picture: picture.downgrade(),
                path: PathBuf::from(path),
                width: thumb_width,
                height: thumb_height,
                generation: self.thumbs.generation.get(),
            });
        }

        body.append(&name);
        button.set_child(Some(&body));
        button.set_tooltip_text(Some(path));

        let on_choose = Rc::clone(&self.on_choose);
        let window = self.window.clone();
        let chosen = path.to_owned();

        button.connect_clicked(move |_| {
            on_choose(Path::new(&chosen));
            window.set_visible(false);
        });

        button
    }
}

impl Drop for Picker {
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

/// A tile's path, carried as its tooltip so no Rust pointer crosses GTK.
fn tile_path(widget: &gtk::Widget) -> Option<String> {
    widget
        .tooltip_text()
        .map(|tooltip| tooltip.to_string())
        .filter(|tooltip| !tooltip.is_empty())
}

fn apply_size(
    window: &gtk::ApplicationWindow,
    card: &gtk::Box,
    flow: &gtk::FlowBox,
    expand_button: &gtk::Button,
    settings: &WallpaperSettings,
    expanded: bool,
) {
    let monitor = window
        .surface()
        .and_then(|surface| gtk::gdk::Display::default()?.monitor_at_surface(&surface))
        .map(|monitor| {
            let geometry = monitor.geometry();

            (geometry.width(), geometry.height())
        });
    let configured = (
        i32::try_from(settings.picker_width).unwrap_or(960),
        i32::try_from(settings.picker_height).unwrap_or(640),
    );
    let columns = i32::try_from(settings.picker_columns).unwrap_or(4).max(1);
    let (width, height) = card_size(expanded, configured, monitor);

    card.set_size_request(width, height);
    flow.set_min_children_per_line(columns as u32);
    flow.set_max_children_per_line(if expanded { 12 } else { columns as u32 });
    expand_button.set_icon_name(if expanded {
        "view-restore-symbolic"
    } else {
        "view-fullscreen-symbolic"
    });
}

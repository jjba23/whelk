// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The first Rust-owned background.
//!
//! One layer-shell surface per output, a `WhelkWallpaperView` in each, and a
//! rotation timer driven by the collection/playlist modules.  The C engine
//! still owns the session's background until `whelk-rs` is installed as
//! `whelk`; both have separate GTK application ids and layer namespaces.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use gdk_pixbuf::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::collection::collect;
use crate::rotation::Playlist;
use crate::settings::WallpaperSettings;
use crate::ui::layer_shell_available;
use crate::wallpaper::{Animation, Kind, kind_for_path};
use crate::wallpaper_view::WhelkWallpaperView;

thread_local! {
    /// The windows this module presented last.  A repaint (a chosen picture, a
    /// settings save, a hotplug) replaces them instead of stacking a second
    /// background on the same outputs.
    static LIVE: RefCell<Vec<gtk::ApplicationWindow>> = const { RefCell::new(Vec::new()) };
    static ROTATION: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

struct Surface {
    view: WhelkWallpaperView,
    generation: Rc<Cell<u64>>,
}

impl Clone for Surface {
    fn clone(&self) -> Self {
        Self {
            view: self.view.clone(),
            generation: self.generation.clone(),
        }
    }
}

/// Destroy the backgrounds and the rotation timer presented last.
pub fn clear() {
    ROTATION.with(|slot| {
        if let Some(timer) = slot.borrow_mut().take() {
            timer.remove();
        }
    });
    LIVE.with(|live| {
        for window in live.borrow_mut().drain(..) {
            window.set_child(None::<&gtk::Widget>);
            window.destroy();
        }
    });
}

pub fn present(application: &gtk::Application, settings: &WallpaperSettings) {
    clear();

    if !settings.enabled {
        return;
    }

    let items = collect(&settings.paths, settings.current.as_deref(), settings.video);

    if items.is_empty() {
        eprintln!(
            "whelk-rs: wallpaper is enabled but no supported files were found in {:?}",
            settings.paths
        );
        return;
    }

    let mut playlist = Playlist::new(items, settings.video, seed());
    playlist.resolve(settings.current.as_deref());

    let Some(first) = playlist.selected().map(Path::to_owned) else {
        return;
    };

    let display = gtk::gdk::Display::default();
    let monitors = display
        .as_ref()
        .map(|display| {
            let model = display.monitors();
            let mut monitors = Vec::new();

            for index in 0..model.n_items() {
                if let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() {
                    monitors.push(monitor);
                }
            }

            monitors
        })
        .unwrap_or_default();

    let mut surfaces = Vec::new();
    let mut windows = Vec::new();

    if monitors.is_empty() {
        let (window, surface) = create_surface(application, settings, None);
        windows.push(window);
        surfaces.push(surface);
    } else {
        for monitor in monitors {
            let (window, surface) = create_surface(application, settings, Some(&monitor));
            windows.push(window);
            surfaces.push(surface);
        }
    }

    for surface in &surfaces {
        load_into_surface(
            surface,
            &first,
            settings.animation,
            settings.animation_ms,
            settings.video,
            settings.mute_video,
        );
    }

    for window in &windows {
        window.present();
    }

    LIVE.with(|live| *live.borrow_mut() = windows);

    let rotate = playlist.should_rotate(true, settings.interval_seconds);
    let playlist = Rc::new(RefCell::new(playlist));
    let interval = settings.interval_seconds;
    let shuffle = settings.shuffle;
    let animation = settings.animation;
    let animation_ms = settings.animation_ms;
    let video = settings.video;
    let mute_video = settings.mute_video;

    if rotate {
        let timer_surfaces = surfaces;
        let timer = glib::timeout_add_seconds_local(interval, move || {
            let next = playlist.borrow_mut().advance(shuffle).map(Path::to_owned);

            if let Some(path) = next {
                for surface in &timer_surfaces {
                    load_into_surface(surface, &path, animation, animation_ms, video, mute_video);
                }
            }

            glib::ControlFlow::Continue
        });

        ROTATION.with(|slot| *slot.borrow_mut() = Some(timer));
    }
}

fn create_surface(
    application: &gtk::Application,
    settings: &WallpaperSettings,
    monitor: Option<&gtk::gdk::Monitor>,
) -> (gtk::ApplicationWindow, Surface) {
    let view = WhelkWallpaperView::new();
    view.set_fit(settings.fit);
    view.set_ken_burns(settings.ken_burns);

    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk-rs wallpaper")
        .build();

    window.add_css_class("whelk-rs-wallpaper-window");
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_child(Some(&view));

    if layer_shell_available() {
        window.init_layer_shell();
        window.set_layer(Layer::Background);
        window.set_namespace(Some("whelk-rust-wallpaper"));
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::None);

        if let Some(monitor) = monitor {
            window.set_monitor(Some(monitor));
        }
    } else {
        // Broadway and X11 have no layer shell.  Keep the same widget tree in
        // a normal window so the backend can still be developed and smoke
        // tested; it is not meant to replace a desktop background there.
        window.set_default_size(960, 540);
    }

    (
        window,
        Surface {
            view,
            generation: Rc::new(Cell::new(0)),
        },
    )
}

fn load_into_surface(
    surface: &Surface,
    path: &Path,
    animation: Animation,
    animation_ms: u32,
    video: bool,
    mute_video: bool,
) {
    let generation = surface.generation.get().wrapping_add(1);
    surface.generation.set(generation);

    if kind_for_path(path) == Kind::Video {
        if !video {
            surface.view.clear();
            return;
        }

        let stream = gtk::MediaFile::for_filename(path);
        stream.set_loop(true);
        stream.set_muted(mute_video);
        stream.play();
        surface
            .view
            .set_paintable(stream.upcast_ref(), animation, animation_ms);
        return;
    }

    load_image(
        &surface.view,
        surface.generation.clone(),
        generation,
        path,
        animation,
        animation_ms,
    );
}

fn load_image(
    view: &WhelkWallpaperView,
    generation: Rc<Cell<u64>>,
    current_generation: u64,
    path: &Path,
    animation: Animation,
    animation_ms: u32,
) {
    match gdk_pixbuf::PixbufAnimation::from_file(path) {
        Ok(animation_object) if animation_object.is_static_image() => {
            if let Some(pixbuf) = animation_object.static_image() {
                let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
                view.set_paintable(texture.upcast_ref(), animation, animation_ms);
                return;
            }
        }
        Ok(animation_object) => {
            let iter = animation_object.iter(None);
            let delay = frame_delay(iter.delay_time());
            let pixbuf = iter.pixbuf();
            let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);

            view.set_paintable(texture.upcast_ref(), animation, animation_ms);

            schedule_frame(iter, view.clone(), generation, current_generation, delay);
            return;
        }
        Err(_) => {}
    }

    // The animation loader has the final word, but GDK's own loader may know a
    // format it does not.  Keep the C fallback for SVG and newer formats.
    match gtk::gdk::Texture::from_filename(path) {
        Ok(texture) => view.set_paintable(texture.upcast_ref(), animation, animation_ms),
        Err(error) => {
            eprintln!(
                "whelk-rs: cannot decode wallpaper '{}': {error}",
                path.display()
            );
            view.clear();
        }
    }
}

fn schedule_frame(
    iter: gdk_pixbuf::PixbufAnimationIter,
    view: WhelkWallpaperView,
    generation: Rc<Cell<u64>>,
    current_generation: u64,
    delay: Duration,
) {
    glib::timeout_add_local(delay, move || {
        if generation.get() != current_generation {
            return glib::ControlFlow::Break;
        }

        iter.advance(SystemTime::now());

        let pixbuf = iter.pixbuf();
        let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
        view.set_frame(texture.upcast_ref());

        schedule_frame(
            iter.clone(),
            view.clone(),
            generation.clone(),
            current_generation,
            frame_delay(iter.delay_time()),
        );

        glib::ControlFlow::Break
    });
}

/// A GIF's zero delay is not "as fast as the CPU can decode": the C engine has
/// read it as 100 ms for years, and the Rust surface keeps that behaviour.
pub fn frame_delay(delay: Option<Duration>) -> Duration {
    match delay {
        Some(delay) if delay >= Duration::from_millis(20) => delay,
        _ => Duration::from_millis(100),
    }
}

fn seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
}

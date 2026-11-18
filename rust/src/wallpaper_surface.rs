// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The first Rust-owned background.
//!
//! One layer-shell surface per output, a `WhelkWallpaperView` in each, and a
//! rotation timer driven by the collection/playlist modules.  The C engine
//! still owns the session's background until `whelk` is installed as
//! `whelk`; both have separate GTK application ids and layer namespaces.
//!
//! The surface set follows the outputs the way C's `sync_surfaces` does.  The
//! outputs are signed as `CONNECTOR:WxH;` in model order; a hotplug or a
//! resolution change rebuilds every surface once an idle has let the monitor
//! model settle, and an unchanged signature leaves a running movie alone.
//! Rebuilding cleans each surface's media first (`view.clear()` and a paused
//! `MediaStream`), so a removed output cannot leave a decoder running behind a
//! destroyed window.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
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
use crate::sources;
use crate::ui::layer_shell_available;
use crate::wallpaper::{Animation, Kind, kind_for_path};
use crate::wallpaper_view::WhelkWallpaperView;

thread_local! {
    /// The windows and views this module presented last.  A repaint (a chosen
    /// picture, a settings save, a hotplug) replaces them instead of stacking a
    /// second background on the same outputs.
    static LIVE: RefCell<Vec<LiveSurface>> = const { RefCell::new(Vec::new()) };
    static ROTATION: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    /// The monitor model watched while a background is up.
    static MONITORS: RefCell<Option<(gtk::gio::ListModel, glib::SignalHandlerId)>> =
        const { RefCell::new(None) };
    /// One idle per burst of monitor changes.
    static SYNC: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    /// What a rebuild needs after `present` has returned.
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

struct Surface {
    view: WhelkWallpaperView,
    generation: Rc<Cell<u64>>,
    /// The stream currently painting this surface, if the item is a video.
    /// Held so a rebuild can pause it instead of leaving it to the GC.
    media: RefCell<Option<gtk::MediaStream>>,
}

struct LiveSurface {
    window: gtk::ApplicationWindow,
    surface: Surface,
}

struct Session {
    application: gtk::Application,
    settings: WallpaperSettings,
    playlist: Rc<RefCell<Playlist>>,
    /// The item a new output must show; rotation keeps it current.
    selected: PathBuf,
    outputs: String,
}

/// One output as the signature sees it.  Separated from `GdkMonitor` so the
/// signature and its rebuild rule can be tested without a display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub connector: Option<String>,
    pub width: i32,
    pub height: i32,
}

/// C's `monitor_signature`: `CONNECTOR:WxH;` for each output, in model order.
/// A missing connector is `?`, exactly as the C engine writes it.
pub fn signature(outputs: &[Output]) -> String {
    let mut signature = String::new();

    for output in outputs {
        signature.push_str(output.connector.as_deref().unwrap_or("?"));
        signature.push_str(&format!(":{}x{};", output.width, output.height));
    }

    signature
}

/// True when the surfaces no longer match the outputs.  C's `sync_surfaces`
/// rebuilds all of them rather than diffing, so `None` (nothing built yet), a
/// changed signature or a stale surface count all mean the same thing.
pub fn needs_rebuild(previous: Option<&str>, outputs: &[Output], surfaces: usize) -> bool {
    let current = signature(outputs);

    previous != Some(current.as_str()) || surfaces != outputs.len()
}

/// Destroy the backgrounds, the rotation timer and the monitor watch presented
/// last.
pub fn clear() {
    ROTATION.with(|slot| {
        if let Some(timer) = slot.borrow_mut().take() {
            sources::remove(timer);
        }
    });
    MONITORS.with(|slot| {
        if let Some((model, handler)) = slot.borrow_mut().take() {
            model.disconnect(handler);
        }
    });
    SYNC.with(|slot| {
        if let Some(source) = slot.borrow_mut().take() {
            sources::remove(source);
        }
    });
    SESSION.with(|slot| *slot.borrow_mut() = None);
    destroy_live();
}

pub fn present(application: &gtk::Application, settings: &WallpaperSettings) {
    clear();

    if !settings.enabled {
        return;
    }

    let items = collect(&settings.paths, settings.current.as_deref(), settings.video);

    if items.is_empty() {
        eprintln!(
            "whelk: wallpaper is enabled but no supported files were found in {:?}",
            settings.paths
        );
        return;
    }

    let mut playlist = Playlist::new(items, settings.video, seed());
    playlist.resolve(settings.current.as_deref());

    let Some(first) = playlist.selected().map(Path::to_owned) else {
        return;
    };
    let rotate = playlist.should_rotate(true, settings.interval_seconds);
    let outputs = current_outputs();
    let live = build_live(application, settings, &first, &outputs);
    let signed = outputs
        .iter()
        .map(|(_, output)| output.clone())
        .collect::<Vec<_>>();

    LIVE.with(|slot| *slot.borrow_mut() = live);
    SESSION.with(|slot| {
        *slot.borrow_mut() = Some(Session {
            application: application.clone(),
            settings: settings.clone(),
            playlist: Rc::new(RefCell::new(playlist)),
            selected: first,
            outputs: signature(&signed),
        });
    });

    watch_monitors();

    if rotate {
        start_rotation(settings);
    }
}

fn current_outputs() -> Vec<(gtk::gdk::Monitor, Output)> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    let model = display.monitors();
    let mut outputs = Vec::new();

    for index in 0..model.n_items() {
        if let Some(monitor) = model.item(index).and_downcast::<gtk::gdk::Monitor>() {
            let output = output_of(&monitor);

            outputs.push((monitor, output));
        }
    }

    outputs
}

fn output_of(monitor: &gtk::gdk::Monitor) -> Output {
    let geometry = monitor.geometry();

    Output {
        connector: monitor
            .connector()
            .map(|connector| connector.to_string())
            .filter(|connector| !connector.is_empty()),
        width: geometry.width(),
        height: geometry.height(),
    }
}

/// Build one surface per output, or a single monitor-less one on a display
/// that reports none, and give each the current picture.
fn build_live(
    application: &gtk::Application,
    settings: &WallpaperSettings,
    path: &Path,
    outputs: &[(gtk::gdk::Monitor, Output)],
) -> Vec<LiveSurface> {
    let mut live = Vec::new();

    if outputs.is_empty() {
        let (window, surface) = create_surface(application, settings, None);

        load_into_surface(
            &surface,
            path,
            settings.animation,
            settings.animation_ms,
            settings.video,
            settings.mute_video,
        );
        window.present();
        live.push(LiveSurface { window, surface });

        return live;
    }

    for (monitor, _) in outputs {
        let (window, surface) = create_surface(application, settings, Some(monitor));

        load_into_surface(
            &surface,
            path,
            settings.animation,
            settings.animation_ms,
            settings.video,
            settings.mute_video,
        );
        window.present();
        live.push(LiveSurface { window, surface });
    }

    live
}

/// Follow the monitor model while a background is up.  An idle is the same
/// shape the bars and the locker use: rebuilding while the model is still
/// emitting makes a surface ask it for a monitor that is going away.
fn watch_monitors() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let model = display.monitors();
    let handler = model.connect_items_changed(|_, _, _, _| schedule_sync());

    MONITORS.with(|slot| *slot.borrow_mut() = Some((model, handler)));
}

fn schedule_sync() {
    if SYNC.with(|slot| slot.borrow().is_some()) {
        return;
    }

    let source = glib::idle_add_local_once(|| {
        SYNC.with(|slot| *slot.borrow_mut() = None);
        sync_surfaces();
    });

    SYNC.with(|slot| *slot.borrow_mut() = Some(source));
}

/// Bring the surfaces in line with the outputs.  C does not diff: a signature
/// change rebuilds the whole set, and each new surface gets the current item
/// while the old ones (and their decoders) are cleaned up first.
fn sync_surfaces() {
    let Some((application, settings, selected, previous)) = SESSION.with(|slot| {
        slot.borrow().as_ref().map(|session| {
            (
                session.application.clone(),
                session.settings.clone(),
                session.selected.clone(),
                session.outputs.clone(),
            )
        })
    }) else {
        return;
    };

    let outputs = current_outputs();
    let described: Vec<Output> = outputs.iter().map(|(_, output)| output.clone()).collect();
    let surfaces = LIVE.with(|slot| slot.borrow().len());

    if !needs_rebuild(Some(&previous), &described, surfaces) {
        return;
    }

    destroy_live();

    let live = build_live(&application, &settings, &selected, &outputs);

    LIVE.with(|slot| *slot.borrow_mut() = live);
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().as_mut() {
            session.outputs = signature(&described);
        }
    });
}

fn start_rotation(settings: &WallpaperSettings) {
    let Some(playlist) = SESSION.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|session| session.playlist.clone())
    }) else {
        return;
    };
    let interval = settings.interval_seconds;
    let shuffle = settings.shuffle;
    let animation = settings.animation;
    let animation_ms = settings.animation_ms;
    let video = settings.video;
    let mute_video = settings.mute_video;
    let timer = glib::timeout_add_seconds_local(interval, move || {
        let next = playlist.borrow_mut().advance(shuffle).map(Path::to_owned);

        if let Some(path) = next {
            SESSION.with(|slot| {
                if let Some(session) = slot.borrow_mut().as_mut() {
                    session.selected = path.clone();
                }
            });
            LIVE.with(|slot| {
                for live in slot.borrow().iter() {
                    load_into_surface(
                        &live.surface,
                        &path,
                        animation,
                        animation_ms,
                        video,
                        mute_video,
                    );
                }
            });
        }

        glib::ControlFlow::Continue
    });

    ROTATION.with(|slot| *slot.borrow_mut() = Some(timer));
}

/// Destroy every background, cleaning up its media first.  A video stream is
/// paused explicitly before its window goes away; dropping the view alone
/// would leave the decoder to the collector.
fn destroy_live() {
    LIVE.with(|slot| {
        for live in slot.borrow_mut().drain(..) {
            cleanup(&live.surface);
            live.window.set_child(None::<&gtk::Widget>);
            live.window.destroy();
        }
    });
}

fn cleanup(surface: &Surface) {
    // An animated wallpaper's frame timer holds a view clone and keeps
    // rescheduling until its generation moves.  Bump it before the view is
    // cleared, or a hotplug or repaint leaves a chain drawing forever against
    // a destroyed view.
    surface
        .generation
        .set(surface.generation.get().wrapping_add(1));

    if let Some(stream) = surface.media.borrow_mut().take() {
        stream.pause();
    }

    surface.view.clear();
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
        .title("whelk wallpaper")
        .build();

    window.add_css_class("whelk-wallpaper-window");
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

        // A GTK window does not take its size from the compositor's configure:
        // a layer client asks for one.  An anchored surface without a size is
        // only as large as its content -- which is how a desktop ended up with
        // the picture drawn in one corner of the screen.
        let (width, height) = surface_size(monitor);
        window.set_default_size(width, height);
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
            media: RefCell::new(None),
        },
    )
}

/// The size a background surface asks for: its output's whole geometry.
///
/// A background is anchored on all four edges, so the compositor would clip it
/// to the output -- but it will not *enlarge* a GTK window that never asked for
/// a size, which is a wallpaper painted in one corner.  A monitor that reports
/// nothing usable keeps the fallback, so a headless backend still runs.
pub fn surface_size(monitor: Option<&gtk::gdk::Monitor>) -> (i32, i32) {
    let fallback = gtk::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gtk::gdk::Monitor>();
    let Some(monitor) = monitor.or(fallback.as_ref()) else {
        return (960, 540);
    };
    let geometry = monitor.geometry();

    (geometry.width().max(1), geometry.height().max(1))
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

    // Whatever this surface showed before is replaced now; pause it before
    // the new picture or stream takes over.
    if let Some(stream) = surface.media.borrow_mut().take() {
        stream.pause();
    }

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
        surface.media.borrow_mut().replace(stream.clone().upcast());

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
                "whelk: cannot decode wallpaper '{}': {error}",
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

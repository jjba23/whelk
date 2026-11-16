// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper widget: one surface, two paintables and a transition.
//!
//! This is a direct port of `src/wallpaper_view.c`.  A `GdkPaintable` can be a
//! still texture or a `GtkMediaStream`, so the widget does not care whether the
//! picture is an image or a movie.  The C engine's fit, offset, opacity, zoom
//! and Ken Burns arithmetic stays in `wallpaper.rs`; this file only applies it
//! to a GTK snapshot.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk4 as gtk;

use crate::wallpaper::{Animation, Fit, ease};

const KEN_BURNS_PERIOD: Duration = Duration::from_secs(60);
const KEN_BURNS_ZOOM: f64 = 0.06;
const TICK_INTERVAL: Duration = Duration::from_millis(33);

mod imp {
    use super::*;

    pub struct WhelkWallpaperView {
        pub(crate) state: RefCell<ViewState>,
    }

    impl Default for WhelkWallpaperView {
        fn default() -> Self {
            Self {
                state: RefCell::new(ViewState::default()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WhelkWallpaperView {
        const NAME: &'static str = "WhelkRsWallpaperView";
        type Type = super::WhelkWallpaperView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for WhelkWallpaperView {
        fn constructed(&self) {
            self.parent_constructed();

            let widget = self.obj();
            widget.add_css_class("whelk-wallpaper-view");
            widget.set_hexpand(true);
            widget.set_vexpand(true);
        }

        fn dispose(&self) {
            if let Some(tick) = self.state.borrow_mut().tick_id.take() {
                tick.remove();
            }
        }
    }

    impl WidgetImpl for WhelkWallpaperView {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let view = self.obj();
            let width = f64::from(view.width());
            let height = f64::from(view.height());

            if width <= 0.0 || height <= 0.0 {
                return;
            }

            snapshot.append_color(
                &gtk::gdk::RGBA::BLACK,
                &gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
            );

            let state = self.state.borrow();
            let Some(current) = state.current.as_ref() else {
                return;
            };

            let progress = if state.transitioning {
                ease(state.progress)
            } else {
                1.0
            };
            let offsets = if state.transitioning {
                state.animation.offsets(progress, width, height)
            } else {
                Default::default()
            };

            let target = DrawTarget {
                snapshot,
                view_width: width,
                view_height: height,
                fit: state.fit,
            };

            if state.transitioning
                && let Some(previous) = state.previous.as_ref()
            {
                target.draw(
                    previous,
                    offsets.out_x,
                    offsets.out_y,
                    1.0,
                    state.animation.alpha_out(progress),
                );
            }

            let zoom = if state.transitioning {
                state.animation.zoom_in(progress)
            } else {
                ken_burns_zoom(state.ken_burns, state.ken_burns_started.elapsed())
            };

            target.draw(
                current,
                offsets.in_x,
                offsets.in_y,
                zoom,
                if state.transitioning {
                    state.animation.alpha_in(progress)
                } else {
                    1.0
                },
            );
        }
    }
}

glib::wrapper! {
    pub struct WhelkWallpaperView(ObjectSubclass<imp::WhelkWallpaperView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for WhelkWallpaperView {
    fn default() -> Self {
        Self::new()
    }
}

impl WhelkWallpaperView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Start a transition to PAINTABLE.  There is no transition from the very
    /// first picture, from `Animation::None`, or from a zero duration.
    pub fn set_paintable(
        &self,
        paintable: &gtk::gdk::Paintable,
        animation: Animation,
        duration_ms: u32,
    ) {
        let mut state = self.imp().state.borrow_mut();
        let same = state
            .current
            .as_ref()
            .is_some_and(|current| current == paintable);

        if same {
            drop(state);
            self.queue_draw();
            return;
        }

        state.previous = state.current.take();
        state.current = Some(paintable.clone());
        state.animation = animation;
        state.duration_ms = duration_ms;
        state.progress = 0.0;
        state.transition_started = Instant::now();
        state.ken_burns_started = state.transition_started;
        state.last_draw = None;
        state.transitioning =
            state.previous.is_some() && animation != Animation::None && duration_ms > 0;

        if !state.transitioning {
            state.previous = None;
        }

        let needs_tick = state.transitioning || (state.ken_burns && state.current.is_some());

        drop(state);

        if needs_tick {
            self.ensure_tick();
        }

        self.queue_draw();
    }

    /// Replace the picture without a transition: a new movie frame or an
    /// animated image's next frame.
    pub fn set_frame(&self, paintable: &gtk::gdk::Paintable) {
        let mut state = self.imp().state.borrow_mut();
        state.current = Some(paintable.clone());
        drop(state);
        self.queue_draw();
    }

    pub fn set_fit(&self, fit: Fit) {
        self.imp().state.borrow_mut().fit = fit;
        self.queue_draw();
    }

    pub fn set_ken_burns(&self, enabled: bool) {
        let mut state = self.imp().state.borrow_mut();
        state.ken_burns = enabled;
        state.ken_burns_started = Instant::now();
        let needs_tick = enabled && state.current.is_some();
        drop(state);

        if needs_tick {
            self.ensure_tick();
        }

        self.queue_draw();
    }

    /// Drop both pictures, leaving the black surface.
    pub fn clear(&self) {
        let mut state = self.imp().state.borrow_mut();
        state.transitioning = false;
        state.progress = 0.0;
        state.previous = None;
        state.current = None;

        if let Some(tick) = state.tick_id.take() {
            tick.remove();
        }

        drop(state);
        self.queue_draw();
    }

    fn ensure_tick(&self) {
        if self.imp().state.borrow().tick_id.is_some() {
            return;
        }

        let weak = self.downgrade();
        let tick = self.add_tick_callback(move |_, _| {
            let Some(view) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            view.tick()
        });

        self.imp().state.borrow_mut().tick_id = Some(tick);
    }

    fn tick(&self) -> glib::ControlFlow {
        let mut state = self.imp().state.borrow_mut();
        let now = Instant::now();
        let was_transitioning = state.transitioning;

        if state.transitioning {
            state.progress = transition_progress(
                now.saturating_duration_since(state.transition_started),
                state.duration_ms,
            );

            if state.progress >= 1.0 {
                state.transitioning = false;
                state.previous = None;
            }
        }

        let draw = was_transitioning
            || state
                .last_draw
                .is_none_or(|last| now.saturating_duration_since(last) >= TICK_INTERVAL);

        if draw {
            state.last_draw = Some(now);
        }

        let keep = state.transitioning || (state.ken_burns && state.current.is_some());

        if !keep {
            state.tick_id = None;
        }

        drop(state);

        if draw {
            self.queue_draw();
        }

        if keep {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    }
}

pub(crate) struct ViewState {
    current: Option<gtk::gdk::Paintable>,
    previous: Option<gtk::gdk::Paintable>,
    animation: Animation,
    fit: Fit,
    duration_ms: u32,
    transition_started: Instant,
    progress: f64,
    transitioning: bool,
    ken_burns: bool,
    ken_burns_started: Instant,
    last_draw: Option<Instant>,
    tick_id: Option<gtk::TickCallbackId>,
}

impl Default for ViewState {
    fn default() -> Self {
        let now = Instant::now();

        Self {
            current: None,
            previous: None,
            animation: Animation::default(),
            fit: Fit::default(),
            duration_ms: 0,
            transition_started: now,
            progress: 0.0,
            transitioning: false,
            ken_burns: false,
            ken_burns_started: now,
            last_draw: None,
            tick_id: None,
        }
    }
}

/// Transition progress in 0..1.  A zero duration completes immediately.
pub fn transition_progress(elapsed: Duration, duration_ms: u32) -> f64 {
    if duration_ms == 0 {
        return 1.0;
    }

    (elapsed.as_secs_f64() * 1000.0 / f64::from(duration_ms)).clamp(0.0, 1.0)
}

/// One slow breath from the fitted picture to 6% larger and back.  A movie is
/// not zoomed by the engine, so callers pass its Ken Burns flag as false.
pub fn ken_burns_zoom(enabled: bool, elapsed: Duration) -> f64 {
    if !enabled {
        return 1.0;
    }

    let phase = (elapsed.as_secs_f64() / KEN_BURNS_PERIOD.as_secs_f64()).fract();
    let breath = 0.5 - 0.5 * (phase * 2.0 * std::f64::consts::PI).cos();

    1.0 + KEN_BURNS_ZOOM * breath
}

struct DrawTarget<'a> {
    snapshot: &'a gtk::Snapshot,
    view_width: f64,
    view_height: f64,
    fit: Fit,
}

impl DrawTarget<'_> {
    fn draw(
        &self,
        paintable: &gtk::gdk::Paintable,
        offset_x: f64,
        offset_y: f64,
        zoom: f64,
        alpha: f64,
    ) {
        if alpha <= 0.0 {
            return;
        }

        let mut image_width = paintable.intrinsic_width();
        let mut image_height = paintable.intrinsic_height();

        if image_width <= 0 || image_height <= 0 {
            image_width = (self.view_width as i32).max(1);
            image_height = (self.view_height as i32).max(1);
        }

        let geometry = self.fit.geometry(
            self.view_width,
            self.view_height,
            f64::from(image_width),
            f64::from(image_height),
            zoom,
        );

        self.snapshot.push_opacity(alpha.clamp(0.0, 1.0));
        self.snapshot.save();
        self.snapshot.translate(&gtk::graphene::Point::new(
            (offset_x + geometry.offset_x) as f32,
            (offset_y + geometry.offset_y) as f32,
        ));
        self.snapshot
            .scale(geometry.scale_x as f32, geometry.scale_y as f32);
        paintable.snapshot(
            self.snapshot,
            f64::from(image_width),
            f64::from(image_height),
        );
        self.snapshot.restore();
        self.snapshot.pop();
    }
}

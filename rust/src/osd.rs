// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The volume and brightness HUD, ported from `src/osd.c`.
//!
//! One overlay belongs to the shell, not to a bar: a volume key should produce
//! one card even when three monitors each have a bar.  Modules call
//! [`Osd::show_current`] after their [`crate::level::Level`] reports a real
//! change; the HUD hides itself and does not take keyboard focus.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge};
use crate::style;

pub const TIMEOUT: Duration = Duration::from_millis(1_400);
const DEFAULT_BOTTOM_MARGIN: i32 = 64;

thread_local! {
    static CURRENT: RefCell<Option<Osd>> = const { RefCell::new(None) };
}

/// The pure placement and formatting rules the HUD uses.
pub fn bottom_margin(bar: &BarSettings) -> i32 {
    if bar.edge == BarEdge::Bottom {
        bar.height + 22
    } else {
        DEFAULT_BOTTOM_MARGIN
    }
}

pub fn progress(percent: i32) -> f64 {
    f64::from(percent.clamp(0, 100)) / 100.0
}

pub fn value_text(percent: i32) -> String {
    format!("{percent}%")
}

/// The shell's one HUD.  Clone it into the modules; the thread-local handle is
/// installed by `ui` and cleared on teardown.
#[derive(Clone)]
pub struct Osd {
    inner: Rc<RefCell<State>>,
}

struct State {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    icon: gtk::Label,
    title: gtk::Label,
    progress: gtk::ProgressBar,
    value: gtk::Label,
    css: Option<gtk::CssProvider>,
    timer: Option<gtk::glib::SourceId>,
    generation: u64,
}

impl Osd {
    pub fn new(application: &gtk::Application, style: &BarSettings) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk osd")
            .build();

        window.set_widget_name(&style::widget_id("osd"));
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(false);

        let card = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        card.add_css_class("whelk-osd");
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);
        card.set_hexpand(false);

        let icon = gtk::Label::new(None);
        icon.add_css_class("whelk-icon");
        icon.add_css_class("whelk-osd-icon");
        icon.set_valign(gtk::Align::Center);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 7);
        column.set_valign(gtk::Align::Center);

        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(None);
        title.add_css_class("whelk-osd-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);

        let value = gtk::Label::new(None);
        value.add_css_class("whelk-osd-value");
        value.set_xalign(1.0);
        value.set_halign(gtk::Align::End);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(false);
        progress.set_size_request(190, -1);

        heading.append(&title);
        heading.append(&value);
        column.append(&heading);
        column.append(&progress);
        card.append(&icon);
        card.append(&column);

        let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        holder.append(&card);
        window.set_child(Some(&holder));

        let osd = Self {
            inner: Rc::new(RefCell::new(State {
                window,
                card,
                icon,
                title,
                progress,
                value,
                css: None,
                timer: None,
                generation: 0,
            })),
        };

        osd.install_style(style);

        {
            let state = osd.inner.borrow();
            let window = &state.window;

            if crate::ui::layer_shell_available() {
                window.init_layer_shell();
                window.set_layer(Layer::Overlay);
                window.set_namespace(Some("whelk-rust-osd"));
                window.set_anchor(Edge::Bottom, true);
                // The HUD overlaps windows instead of reserving a strip for
                // itself; 0 matches the C surface's non-exclusive zone.
                window.set_exclusive_zone(0);
                window.set_keyboard_mode(KeyboardMode::None);
                window.set_margin(Edge::Bottom, bottom_margin(style));
            } else {
                window.set_default_size(360, 96);
            }
        }

        // Present once so the layer surface is realized, then keep it out of
        // sight until a reading changes.
        {
            let state = osd.inner.borrow();
            state.window.present();
            state.window.set_visible(false);
        }

        osd
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub fn install(osd: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = osd);
    }

    pub fn show_current(icon: &str, title: &str, percent: i32) {
        if let Some(osd) = Self::current() {
            osd.show(icon, title, percent);
        }
    }

    pub fn show(&self, icon: &str, title: &str, percent: i32) {
        let weak = Rc::downgrade(&self.inner);
        let mut state = self.inner.borrow_mut();

        state.icon.set_text(icon);
        state.title.set_text(title);
        state.progress.set_fraction(progress(percent));
        state.value.set_text(&value_text(percent));
        state.window.present();

        if let Some(timer) = state.timer.take() {
            timer.remove();
        }

        state.generation = state.generation.wrapping_add(1);

        let generation = state.generation;

        state.timer = Some(gtk::glib::timeout_add_local_once(TIMEOUT, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let mut state = inner.borrow_mut();

            if state.generation == generation {
                state.timer = None;
                state.window.set_visible(false);
            }
        }));
    }

    pub fn hide(&self) {
        let mut state = self.inner.borrow_mut();

        state.generation = state.generation.wrapping_add(1);

        if let Some(timer) = state.timer.take() {
            timer.remove();
        }

        state.window.set_visible(false);
    }

    pub fn set_style(&self, style: &BarSettings) {
        self.install_style(style);

        let state = self.inner.borrow();

        if crate::ui::layer_shell_available() {
            state.window.set_margin(Edge::Bottom, bottom_margin(style));
        }

        state.card.queue_draw();
    }

    fn install_style(&self, bar: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let widget_id = style::widget_id("osd");
        let accent = bar
            .accent
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|accent| {
                format!(
                    ".whelk-osd progressbar progress {{ background-color: {accent}; }}\n\
                     .whelk-osd progressbar trough {{ background-color: alpha(currentColor, 0.16); }}"
                )
            });
        let generated = style::css(&widget_id, bar, "whelk-osd", accent.as_deref());
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{BASE_CSS}\n{generated}"));

        let mut state = self.inner.borrow_mut();

        if let Some(previous) = state.css.take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        state.css = Some(css);
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.destroy();
    }
}

const BASE_CSS: &str = "
    .whelk-osd {
        padding: 16px 20px;
        min-width: 286px;
        background-image: linear-gradient(to bottom, rgba(255, 255, 255, 0.09),
                                          rgba(255, 255, 255, 0) 42%);
        box-shadow: 0 10px 28px alpha(black, 0.34);
    }
    .whelk-osd-icon {
        font-size: 34px;
        min-width: 40px;
    }
    .whelk-osd-title {
        font-size: 0.95em;
        font-weight: 600;
    }
    .whelk-osd-value {
        font-size: 0.85em;
        opacity: 0.62;
    }
    .whelk-osd progressbar trough,
    .whelk-osd progressbar progress {
        min-height: 8px;
        border-radius: 4px;
    }
";

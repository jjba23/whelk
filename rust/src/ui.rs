// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The first Rust-owned surface: a small top bar.
//!
//! It reads the same bar alist as the C shell.  The widget tree is still a
//! bootstrap -- the configured modules are not all built yet -- but the
//! geometry, edge, layer, margins, clock format and generated skin now come
//! from Scheme rather than a hardcoded stylesheet.

use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge, Layer as BarLayer};
use crate::locker;
use crate::settings::WallpaperSettings;
use crate::style;

pub fn run(
    settings: WallpaperSettings,
    bar: BarSettings,
    config_label: String,
) -> gtk::glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("org.jointhefreeworld.whelk.rust")
        .build();

    let settings = Rc::new(settings);
    let bar = Rc::new(bar);
    let config_label = Rc::new(config_label);

    application.connect_activate(move |application| {
        present(application, &settings, &bar, &config_label);
    });

    let quit = gtk::gio::SimpleAction::new("quit", None);
    let application_weak = application.downgrade();

    quit.connect_activate(move |_, _| {
        if let Some(application) = application_weak.upgrade() {
            application.quit();
        }
    });

    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);

    // main() has already consumed whelk's options; give GTK only a program
    // name, or it will reject --config as one of its own.
    application.run_with_args(&["whelk-rs"])
}

fn present(
    application: &gtk::Application,
    settings: &WallpaperSettings,
    bar_settings: &BarSettings,
    config_label: &str,
) {
    install_css(bar_settings);

    // The C shell prepares the locker at startup, before a `--lock` or a
    // logind signal can arrive; the Rust shell watches logind at the same
    // point and starts `whelk-rs --lock` when a trigger fires.
    locker::watch_logind();

    let widget_id = style::widget_id(&bar_settings.name);
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk-rs")
        .build();

    window.set_widget_name(&widget_id);
    window.add_css_class("whelk-surface");
    window.set_decorated(false);
    window.set_resizable(false);

    let bar = gtk::CenterBox::new();
    bar.add_css_class("whelk-bar");
    bar.set_size_request(-1, bar_settings.height);

    let name = gtk::Label::new(Some("whelk"));
    name.add_css_class("whelk-rs-name");
    name.set_tooltip_text(Some(config_label));

    let wallpaper = gtk::Label::new(Some(&settings.summary()));
    wallpaper.add_css_class("whelk-rs-status");
    wallpaper.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let clock = gtk::Label::new(None);
    let clock_format = bar_settings
        .clock_format()
        .unwrap_or_else(|| "%H:%M".to_owned());

    clock.add_css_class("whelk-rs-clock");
    update_clock(&clock, &clock_format);

    bar.set_start_widget(Some(&name));
    bar.set_center_widget(Some(&wallpaper));
    bar.set_end_widget(Some(&clock));
    window.set_child(Some(&bar));

    if layer_shell_available() {
        configure_layer_shell(&window, bar_settings);
    } else {
        // Broadway and a plain X11 test display have no layer shell.  A normal
        // window is still a useful smoke test for the widget tree.
        window.set_default_size(720, bar_settings.height.max(32));
    }

    let clock = clock.clone();
    gtk::glib::timeout_add_seconds_local(1, move || {
        update_clock(&clock, &clock_format);
        gtk::glib::ControlFlow::Continue
    });

    window.present();

    crate::wallpaper_surface::present(application, settings);
}

fn configure_layer_shell(window: &gtk::ApplicationWindow, bar: &BarSettings) {
    window.init_layer_shell();
    window.set_layer(layer_for(bar.layer));
    window.set_namespace(Some("whelk-rust"));

    let edge = edge_for(bar.edge);

    window.set_anchor(edge, true);

    match bar.edge {
        BarEdge::Top | BarEdge::Bottom => {
            window.set_anchor(Edge::Left, true);
            window.set_anchor(Edge::Right, true);
        }
        BarEdge::Left | BarEdge::Right => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Bottom, true);
        }
    }

    window.set_margin(Edge::Top, bar.margin_top);
    window.set_margin(Edge::Right, bar.margin_right);
    window.set_margin(Edge::Bottom, bar.margin_bottom);
    window.set_margin(Edge::Left, bar.margin_left);
    window.set_exclusive_zone(if bar.exclusive { bar.height } else { -1 });
    window.set_keyboard_mode(KeyboardMode::None);
}

fn edge_for(edge: BarEdge) -> Edge {
    match edge {
        BarEdge::Top => Edge::Top,
        BarEdge::Bottom => Edge::Bottom,
        BarEdge::Left => Edge::Left,
        BarEdge::Right => Edge::Right,
    }
}

fn layer_for(layer: BarLayer) -> Layer {
    match layer {
        BarLayer::Background => Layer::Background,
        BarLayer::Bottom => Layer::Bottom,
        BarLayer::Top => Layer::Top,
        BarLayer::Overlay => Layer::Overlay,
    }
}

pub(crate) fn layer_shell_available() -> bool {
    // gtk4-layer-shell is about the Wayland protocol, not merely the library.
    // On Broadway or X11, init_layer_shell() asserts; keep those backends on
    // the ordinary-window path.
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };

    // Check the backend first: gtk_layer_is_supported() itself asserts on a
    // non-Wayland display instead of quietly answering false.
    display.backend().is_wayland() && gtk4_layer_shell::is_supported()
}

fn update_clock(label: &gtk::Label, format: &str) {
    let text = gtk::glib::DateTime::now_local()
        .and_then(|now| now.format(format))
        .map(|formatted| formatted.to_string())
        .unwrap_or_else(|_| "--:--".to_owned());

    label.set_text(&text);
}

fn install_css(bar: &BarSettings) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let widget_id = style::widget_id(&bar.name);
    let base = "
        .whelk-surface {
            background-color: transparent;
        }
        .whelk-rs-bar {
            padding: 0;
        }
        .whelk-rs-name {
            font-weight: 800;
        }
        .whelk-rs-status {
            opacity: 0.72;
        }
        .whelk-rs-clock {
            font-weight: 700;
        }
        ";
    let generated = style::css(&widget_id, bar, "whelk-bar", bar.css.as_deref());
    let css = gtk::CssProvider::new();

    css.load_from_string(&format!("{base}\n{generated}"));

    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

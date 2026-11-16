// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The first Rust-owned surface: a small top bar.
//!
//! This is deliberately not the C bar reimplemented.  It proves the stack end
//! to end -- a GTK4 application, a wlr-layer-shell surface when the compositor
//! supports one, and labels fed from the Scheme configuration -- while the C
//! bar keeps working.  Later slices can grow the module system behind this
//! window without changing the bridge.

use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::settings::WallpaperSettings;

const BAR_HEIGHT: i32 = 32;

pub fn run(settings: WallpaperSettings, config_label: String) -> gtk::glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("org.jointhefreeworld.whelk.rust")
        .build();

    let settings = Rc::new(settings);
    let config_label = Rc::new(config_label);

    application.connect_activate(move |application| {
        present(application, &settings, &config_label);
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

fn present(application: &gtk::Application, settings: &WallpaperSettings, config_label: &str) {
    install_css();

    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk-rs")
        .build();

    window.add_css_class("whelk-rs-bar-window");
    window.set_decorated(false);
    window.set_resizable(false);

    let bar = gtk::CenterBox::new();
    bar.add_css_class("whelk-rs-bar");
    bar.set_size_request(-1, BAR_HEIGHT);

    let name = gtk::Label::new(Some("whelk-rs"));
    name.add_css_class("whelk-rs-name");
    name.set_tooltip_text(Some(config_label));

    let wallpaper = gtk::Label::new(Some(&settings.summary()));
    wallpaper.add_css_class("whelk-rs-status");
    wallpaper.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let clock = gtk::Label::new(None);
    clock.add_css_class("whelk-rs-clock");
    update_clock(&clock);

    bar.set_start_widget(Some(&name));
    bar.set_center_widget(Some(&wallpaper));
    bar.set_end_widget(Some(&clock));
    window.set_child(Some(&bar));

    if layer_shell_available() {
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace(Some("whelk-rust"));
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(BAR_HEIGHT);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_margin(Edge::Top, 0);
        window.set_margin(Edge::Left, 0);
        window.set_margin(Edge::Right, 0);
    } else {
        // Broadway and a plain X11 test display have no layer shell.  A normal
        // window is still a useful smoke test for the widget tree.
        window.set_default_size(720, BAR_HEIGHT);
    }

    let clock = clock.clone();
    gtk::glib::timeout_add_seconds_local(1, move || {
        update_clock(&clock);
        gtk::glib::ControlFlow::Continue
    });

    window.present();
}

fn layer_shell_available() -> bool {
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

fn update_clock(label: &gtk::Label) {
    let text = gtk::glib::DateTime::now_local()
        .and_then(|now| now.format("%H:%M"))
        .map(|formatted| formatted.to_string())
        .unwrap_or_else(|_| "--:--".to_owned());

    label.set_text(&text);
}

fn install_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let css = gtk::CssProvider::new();
    css.load_from_string(
        "
        .whelk-rs-bar-window {
            background-color: rgba(18, 18, 28, 0.94);
        }
        .whelk-rs-bar {
            padding: 0 14px;
            color: #cdd6f4;
            font-family: \"Inter\", \"Cantarell\", sans-serif;
            font-size: 11pt;
        }
        .whelk-rs-name {
            color: #89b4fa;
            font-weight: 800;
        }
        .whelk-rs-status {
            color: #a6adc8;
        }
        .whelk-rs-clock {
            color: #f5e0dc;
            font-weight: 700;
        }
        ",
    );

    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

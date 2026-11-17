// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! About This System, ported from `src/about.c`.
//!
//! The menu's first item is a report of the machine, not of the program: the
//! host name, the processor, the memory and disk that are there now, the
//! display in front of the reader.  It is deliberately opaque -- a system
//! report is text, and text over a wallpaper is the one thing an About window
//! must not be -- while its palette still comes from the bar.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::BarSettings;
use crate::hostinfo;
use crate::style;

thread_local! {
    /// A GTK window with no application is not kept alive by `present()` alone.
    /// Hold the one About window here until its own destroy signal clears it;
    /// asking again brings the same report forward instead of stacking copies.
    static CURRENT: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}

/// Show one About window for `style`'s palette.
pub fn show(style: &BarSettings) {
    if let Some(window) = CURRENT.with(|current| current.borrow().clone()) {
        window.present();

        return;
    }

    let host = gtk::glib::host_name().to_string();
    let host = if host.is_empty() {
        "This System".to_owned()
    } else {
        host
    };
    let os = hostinfo::os_name();
    let readings = [
        ("Processor", hostinfo::cpu_summary()),
        ("Memory", hostinfo::memory_summary()),
        ("Display", hostinfo::display_summary()),
        ("Disk", hostinfo::disk_summary()),
        ("Kernel", hostinfo::kernel_release()),
        ("Session", Some(hostinfo::session_summary())),
        ("Uptime", hostinfo::uptime_summary()),
    ];

    let window = gtk::Window::builder()
        .title("About This System")
        .default_width(560)
        .resizable(false)
        .build();

    window.set_icon_name(Some("org.jointhefreeworld.whelk"));
    window.add_css_class("whelk-about-window");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(24);
    content.set_margin_bottom(20);
    content.add_css_class("whelk-about");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let badge = gtk::Image::from_icon_name("computer-symbolic");
    badge.set_pixel_size(56);
    badge.set_valign(gtk::Align::Center);
    badge.add_css_class("whelk-about-badge");

    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
    titles.set_valign(gtk::Align::Center);

    let name = gtk::Label::new(Some(&host));
    name.set_xalign(0.0);
    name.add_css_class("whelk-about-host");

    let system = gtk::Label::new(Some(&os));
    system.set_xalign(0.0);
    system.set_ellipsize(gtk::pango::EllipsizeMode::End);
    system.add_css_class("whelk-about-os");

    titles.append(&name);
    titles.append(&system);
    header.append(&badge);
    header.append(&titles);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(24);
    grid.set_row_spacing(9);
    grid.add_css_class("whelk-about-rows");

    let mut report = format!("{host}\n{os}\n\n");

    for (row, (key, value)) in readings.into_iter().enumerate() {
        let value = value.filter(|value| !value.is_empty());

        let key_label = gtk::Label::new(Some(key));
        key_label.set_xalign(0.0);
        key_label.set_halign(gtk::Align::Start);
        key_label.add_css_class("whelk-about-key");

        let reading = gtk::Label::new(Some(value.as_deref().unwrap_or("—")));
        reading.set_xalign(1.0);
        reading.set_halign(gtk::Align::End);
        reading.set_hexpand(true);
        reading.set_selectable(true);
        reading.set_ellipsize(gtk::pango::EllipsizeMode::End);
        reading.add_css_class("whelk-about-value");

        grid.attach(&key_label, 0, row as i32, 1, 1);
        grid.attach(&reading, 1, row as i32, 1, 1);

        report.push_str(key);
        report.push_str(": ");
        report.push_str(value.as_deref().unwrap_or("—"));
        report.push('\n');
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let version = gtk::Label::new(Some(&format!(
        "whelk {} · GTK {}.{}.{}",
        env!("CARGO_PKG_VERSION"),
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version()
    )));
    version.set_xalign(0.0);
    version.set_hexpand(true);
    version.add_css_class("whelk-about-footer");

    let copy = gtk::Button::with_label("Copy");
    copy.set_tooltip_text(Some("Copy this report to the clipboard (Ctrl+C)"));
    copy.add_css_class("whelk-about-action");

    let close = gtk::Button::with_label("Close");
    close.add_css_class("whelk-about-action");
    close.add_css_class("suggested-action");

    let report = Rc::new(report);
    let copy_report = report.clone();

    copy.connect_clicked(move |_| copy_to_clipboard(&copy_report));
    close.connect_clicked({
        let window = window.downgrade();

        move |_| {
            if let Some(window) = window.upgrade() {
                window.close();
            }
        }
    });

    footer.append(&version);
    footer.append(&copy);
    footer.append(&close);

    content.append(&header);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&grid);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&footer);
    window.set_child(Some(&content));

    let widget_id = style::widget_id("about");
    window.set_widget_name(&widget_id);

    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();

        provider.load_from_string(&window_css(&widget_id, style));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        window.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    let keys = gtk::EventControllerKey::new();
    let window_weak = window.downgrade();
    let key_report = report.clone();

    keys.connect_key_pressed(move |_, key, _, state| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = window_weak.upgrade() {
                window.close();
            }

            return gtk::glib::Propagation::Stop;
        }

        if state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && (key == gtk::gdk::Key::c || key == gtk::gdk::Key::C)
        {
            copy_to_clipboard(&key_report);

            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);

    CURRENT.with(|current| *current.borrow_mut() = Some(window.clone()));
    window.connect_destroy(|_| {
        CURRENT.with(|current| {
            current.borrow_mut().take();
        });
    });
    window.present();
}

fn copy_to_clipboard(report: &str) {
    if let Some(display) = gtk::gdk::Display::default() {
        display.clipboard().set_text(report);
    }
}

fn window_css(widget_id: &str, style: &BarSettings) -> String {
    let mut css = format!(
        "window#{widget_id} headerbar {{\n\
           background: transparent;\n\
           border: none;\n\
           box-shadow: none;\n\
         }}\n\
         window#{widget_id} .whelk-about {{\n\
           background: none;\n\
           border: none;\n\
           box-shadow: none;\n\
           border-radius: 0;\n\
           padding: 0;\n\
         }}\n\
         window#{widget_id} .whelk-about-badge {{\n\
           padding: 10px;\n\
           border-radius: 40px;\n\
           background: alpha(currentColor, 0.08);\n\
         }}\n\
         window#{widget_id} .whelk-about-host {{\n\
           font-size: 1.4em;\n\
           font-weight: 700;\n\
         }}\n\
         window#{widget_id} .whelk-about-os {{\n\
           opacity: 0.6;\n\
           font-size: 0.92em;\n\
         }}\n\
         window#{widget_id} .whelk-about-key {{\n\
           opacity: 0.55;\n\
         }}\n\
         window#{widget_id} .whelk-about-value {{\n\
           font-weight: 500;\n\
         }}\n\
         window#{widget_id} .whelk-about-footer {{\n\
           opacity: 0.5;\n\
           font-size: 0.82em;\n\
         }}\n\
         window#{widget_id} .whelk-about-action {{\n\
           padding: 5px 14px;\n\
           border-radius: 8px;\n\
         }}\n"
    );

    if let Some(solid) = style::solid(style.background.as_deref()) {
        css.push_str(&format!(
            "window#{widget_id} {{\n  background-color: {solid};\n}}\n"
        ));
    }

    if let Some(foreground) = style
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(
            "window#{widget_id} {{\n  color: {foreground};\n}}\n"
        ));
    }

    css
}

thread_local! {
    /// The About whelk window, kept the same way the system report is: asking
    /// again brings the same window forward.
    static PROGRAM: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}

/// Show the program's own About window: what whelk is, which build this is, the
/// session it is running in and where its documentation, source and package
/// channels are.
pub fn show_program(style: &BarSettings, config_label: &str) {
    if let Some(window) = PROGRAM.with(|current| current.borrow().clone()) {
        window.present();

        return;
    }

    let session = crate::session::Session::detect();
    let rows = [
        ("Version", Some(env!("CARGO_PKG_VERSION").to_owned())),
        ("Session", Some(session.name().to_owned())),
        (
            "GTK",
            Some(format!(
                "{}.{}.{}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version()
            )),
        ),
        ("Settings", Some(config_label.to_owned())),
        (
            "Documents",
            Some("docs/index.org in the source tree".to_owned()),
        ),
        ("Source", Some("https://github.com/jjba23/whelk".to_owned())),
        (
            "Guix channel",
            Some("https://github.com/jjba23/guix-jointhefreeworld".to_owned()),
        ),
        (
            "Nix flake",
            Some("https://github.com/jjba23/nix-jointhefreeworld".to_owned()),
        ),
    ];
    let window = gtk::Window::builder()
        .title("About whelk")
        .default_width(560)
        .resizable(false)
        .build();

    window.set_icon_name(Some("org.jointhefreeworld.whelk"));
    window.add_css_class("whelk-about-window");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);

    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(24);
    content.set_margin_bottom(20);
    content.add_css_class("whelk-about");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let badge = gtk::Image::from_icon_name("org.jointhefreeworld.whelk");
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);

    badge.set_pixel_size(56);
    badge.set_valign(gtk::Align::Center);
    badge.add_css_class("whelk-about-badge");
    titles.set_valign(gtk::Align::Center);

    let name = gtk::Label::new(Some("whelk"));

    name.set_xalign(0.0);
    name.add_css_class("whelk-about-host");

    let subtitle = gtk::Label::new(Some(
        "A Wayland shell in Rust, GTK4 and Guile Scheme: bars, a launcher and panels.",
    ));

    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("whelk-about-os");
    titles.append(&name);
    titles.append(&subtitle);
    header.append(&badge);
    header.append(&titles);

    let grid = gtk::Grid::new();

    grid.set_column_spacing(24);
    grid.set_row_spacing(9);
    grid.add_css_class("whelk-about-rows");

    let mut report = format!("whelk {}\n\n", env!("CARGO_PKG_VERSION"));

    for (row, (key, value)) in rows.into_iter().enumerate() {
        let value = value.filter(|value| !value.is_empty());
        let key_label = gtk::Label::new(Some(key));
        let reading = gtk::Label::new(Some(value.as_deref().unwrap_or("—")));

        key_label.set_xalign(0.0);
        key_label.set_halign(gtk::Align::Start);
        key_label.add_css_class("whelk-about-key");
        reading.set_xalign(1.0);
        reading.set_halign(gtk::Align::End);
        reading.set_hexpand(true);
        reading.set_selectable(true);
        reading.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        reading.add_css_class("whelk-about-value");
        grid.attach(&key_label, 0, row as i32, 1, 1);
        grid.attach(&reading, 1, row as i32, 1, 1);
        report.push_str(key);
        report.push_str(": ");
        report.push_str(value.as_deref().unwrap_or("—"));
        report.push('\n');
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let licence = gtk::Label::new(Some(&format!(
        "MIT · © Josep Bigorra · GTK {}.{}",
        gtk::major_version(),
        gtk::minor_version()
    )));

    licence.set_xalign(0.0);
    licence.set_hexpand(true);
    licence.add_css_class("whelk-about-footer");

    let docs = gtk::Button::with_label("Documentation");
    let copy = gtk::Button::with_label("Copy");
    let close = gtk::Button::with_label("Close");

    docs.set_tooltip_text(Some("Open the project page"));
    docs.add_css_class("whelk-about-action");
    copy.set_tooltip_text(Some("Copy this report to the clipboard"));
    copy.add_css_class("whelk-about-action");
    close.add_css_class("whelk-about-action");
    close.add_css_class("suggested-action");

    let report = Rc::new(report);
    let copy_report = Rc::clone(&report);

    docs.connect_clicked(|_| {
        crate::command::spawn("xdg-open 'https://github.com/jjba23/whelk#readme'");
    });
    copy.connect_clicked(move |_| copy_to_clipboard(&copy_report));
    close.connect_clicked({
        let window = window.downgrade();

        move |_| {
            if let Some(window) = window.upgrade() {
                window.close();
            }
        }
    });

    footer.append(&licence);
    footer.append(&docs);
    footer.append(&copy);
    footer.append(&close);
    content.append(&header);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&grid);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&footer);
    window.set_child(Some(&content));

    let widget_id = style::widget_id("about-whelk");

    window.set_widget_name(&widget_id);

    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();

        provider.load_from_string(&window_css(&widget_id, style));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        window.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    // Escape closes it and Ctrl+C copies the report, the same keys the system
    // report answers to.
    let keys = gtk::EventControllerKey::new();
    let window_weak = window.downgrade();
    let key_report = Rc::clone(&report);

    keys.connect_key_pressed(move |_, key, _, state| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = window_weak.upgrade() {
                window.close();
            }

            return gtk::glib::Propagation::Stop;
        }

        if state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && (key == gtk::gdk::Key::c || key == gtk::gdk::Key::C)
        {
            copy_to_clipboard(&key_report);

            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);

    let current = window.clone();

    window.connect_destroy(move |_| {
        PROGRAM.with(|slot| {
            let mut slot = slot.borrow_mut();

            if slot.as_ref().is_some_and(|held| held == &current) {
                slot.take();
            }
        });
    });
    PROGRAM.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window.present();
}

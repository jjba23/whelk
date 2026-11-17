// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The start bar module, ported from `modules/start.c`.
//!
//! The first thing on the bar is the machine's own menu, behind the drawn
//! whelk mark.  Its rows are configuration from `(whelk menu)`, its Recent
//! Items submenu is filled from GTK's recent manager when it opens, and an item
//! beginning `shell:` is a thing this process does itself.  Every popover,
//! submenus included, walks with Up, Down, Right and Escape.

use std::rc::Rc;

use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::about;
use crate::bar::{BarSettings, ModuleSpec};
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::command;
use crate::format;
use crate::launcherpanel::Launcher;
use crate::locker;
use crate::networkpanel::NetworkPanel;
use crate::notificationcenter::NotificationCenter;
use crate::notifications::NotificationService;
use crate::startmenu::{CommandAction, MenuEntry, command_action};
use crate::style;

const DEFAULT_INTERVAL_SECONDS: u32 = 0;
const DEFAULT_ICON_SIZE: i32 = 18;
const DEFAULT_TOOLTIP: &str = "whelk";
const RECENT_LIMIT: usize = 12;

const MENU_CSS: &str = "
.whelk-start-button {
  padding: 0;
  min-width: 0;
  min-height: 0;
  background: transparent;
  border: none;
  box-shadow: none;
}
.whelk-start-popover {
  padding: 6px;
}
.whelk-start-popover .whelk-menu {
  padding: 2px;
}
.whelk-start-popover .whelk-menu-item {
  padding: 4px 10px;
  border-radius: 5px;
}
.whelk-start-popover separator {
  margin: 4px 8px;
}
";

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    let button = gtk::MenuButton::new();
    let glyph = module.text("label");
    let tooltip = module
        .text("tooltip")
        .unwrap_or_else(|| DEFAULT_TOOLTIP.to_owned());
    let pixels = module
        .integer("icon-size")
        .unwrap_or(i64::from(DEFAULT_ICON_SIZE))
        .clamp(8, 96) as i32;

    button.add_css_class("flat");
    button.add_css_class("whelk-start-button");
    button.set_child(Some(&button_child(glyph.as_deref(), pixels, context)));
    button.set_tooltip_text(Some(&tooltip));

    let popover = gtk::Popover::new();
    popover.add_css_class("whelk-start-popover");
    popover.set_has_arrow(false);

    let entries = if context.shell_menu.is_empty() {
        MenuEntry::fallback_menu()
    } else {
        (*context.shell_menu).clone()
    };
    let style = Rc::new(context.bar.clone());

    fill_menu(&popover, &entries, &style);
    attach_keys(&popover);
    button.set_popover(Some(&popover));
    root.append(&button);

    if let Some(provider) = install_css(&context.bar) {
        root.connect_destroy(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
        });
    }

    drop(host);

    Some(root)
}

fn button_child(glyph: Option<&str>, pixels: i32, context: &ModuleContext) -> gtk::Widget {
    if let Some(glyph) = glyph.filter(|glyph| !glyph.is_empty()) {
        let label = gtk::Label::new(None);

        label.set_markup(&format::icon_markup(glyph, context.icon_font.as_deref()));

        return label.upcast();
    }

    let mark = gtk::DrawingArea::new();
    let ink = context.bar.foreground.clone();

    mark.set_size_request(pixels, pixels);
    mark.set_valign(gtk::Align::Center);
    mark.set_draw_func(move |area, cr, width, height| {
        let colour = ink
            .as_deref()
            .and_then(|text| gtk::gdk::RGBA::parse(text).ok())
            .unwrap_or_else(|| area.color());

        cr.set_source_rgba(
            f64::from(colour.red()),
            f64::from(colour.green()),
            f64::from(colour.blue()),
            f64::from(colour.alpha()),
        );
        draw_mark(cr, f64::from(width.min(height)));

        let _ = cr.fill();
    });

    mark.upcast()
}

fn fill_menu(popover: &gtk::Popover, entries: &[MenuEntry], style: &Rc<BarSettings>) {
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-menu");
    popover.set_child(Some(&menu));

    for entry in entries {
        add_entry(&menu, entry, popover, style);
    }
}

fn add_entry(menu: &gtk::Box, entry: &MenuEntry, parent: &gtk::Popover, style: &Rc<BarSettings>) {
    match entry {
        MenuEntry::Separator => {
            menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        MenuEntry::Submenu {
            label,
            marker,
            entries,
        } => {
            let button = gtk::MenuButton::new();

            button.set_has_frame(false);
            button.set_label(label);
            button.set_direction(gtk::ArrowType::Right);
            button.add_css_class("flat");
            button.add_css_class("whelk-menu-item");
            button.set_halign(gtk::Align::Fill);

            let popover = gtk::Popover::new();
            popover.add_css_class("whelk-start-popover");
            popover.set_has_arrow(false);
            fill_menu(&popover, entries, style);
            attach_keys(&popover);

            if marker.as_deref() == Some("shell:recent-items") {
                popover.connect_show(fill_recent_items);
            }

            button.set_popover(Some(&popover));
            menu.append(&button);
        }
        MenuEntry::Item {
            label,
            action,
            enabled,
        } => {
            let button = gtk::Button::new();
            let row = gtk::Label::new(Some(label));

            row.set_xalign(0.0);
            row.set_hexpand(true);
            button.set_child(Some(&row));
            button.add_css_class("flat");
            button.add_css_class("whelk-menu-item");
            button.set_halign(gtk::Align::Fill);
            button.set_has_frame(false);
            button.set_sensitive(*enabled);

            if let Some(action) = action.clone() {
                let popover = parent.downgrade();
                let style = style.clone();

                button.connect_clicked(move |_| {
                    if let Some(popover) = popover.upgrade() {
                        popover.popdown();
                    }

                    run_action(&action, &style);
                });
            }

            menu.append(&button);
        }
    }
}

fn fill_recent_items(popover: &gtk::Popover) {
    let manager = gtk::RecentManager::default();
    let mut items = manager.items();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let mut shown = 0;

    items.sort_by_key(|info| std::cmp::Reverse(info.modified().to_unix()));
    menu.add_css_class("whelk-menu");

    for info in items {
        if shown >= RECENT_LIMIT {
            break;
        }

        let uri = info.uri().to_string();

        if uri.is_empty() {
            continue;
        }

        let label = info.display_name().to_string();
        let label = if label.is_empty() { uri.clone() } else { label };
        let command = format!(
            "xdg-open {}",
            gtk::glib::shell_quote(&uri).to_string_lossy()
        );
        let button = menu_button(&label);
        let popover = popover.downgrade();

        button.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }

            command::spawn(&command);
        });
        menu.append(&button);
        shown += 1;
    }

    if shown == 0 {
        let empty = gtk::Label::new(Some("No Recent Items"));

        empty.add_css_class("dim-label");
        menu.append(&empty);
    }

    popover.set_child(Some(&menu));
}

fn menu_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);

    button.add_css_class("flat");
    button.add_css_class("whelk-menu-item");
    button.set_halign(gtk::Align::Fill);

    button
}

fn attach_keys(popover: &gtk::Popover) {
    let keys = gtk::EventControllerKey::new();
    let popover_weak = popover.downgrade();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        let Some(popover) = popover_weak.upgrade() else {
            return gtk::glib::Propagation::Proceed;
        };

        if key == gtk::gdk::Key::Escape {
            popover.popdown();

            return gtk::glib::Propagation::Stop;
        }

        let focus = popover.root().and_then(|root| root.focus());
        let focus = focus.filter(|focus| focus.is_ancestor(&popover));

        if key == gtk::gdk::Key::Right
            && let Some(button) = focus
                .as_ref()
                .and_then(|focus| focus.downcast_ref::<gtk::MenuButton>())
            && let Some(submenu) = button.popover()
        {
            submenu.popup();

            return gtk::glib::Propagation::Stop;
        }

        if key != gtk::gdk::Key::Down && key != gtk::gdk::Key::Up {
            return gtk::glib::Propagation::Proceed;
        }

        let Some(menu) = popover.child() else {
            return gtk::glib::Propagation::Proceed;
        };

        if let Some(target) = neighbour_item(&menu, focus.as_ref(), key == gtk::gdk::Key::Down) {
            target.grab_focus();
        }

        gtk::glib::Propagation::Stop
    });
    popover.add_controller(keys);
}

fn neighbour_item(
    menu: &gtk::Widget,
    from: Option<&gtk::Widget>,
    down: bool,
) -> Option<gtk::Widget> {
    let mut items = Vec::new();
    let mut child = menu.first_child();

    while let Some(widget) = child {
        if focusable(&widget) {
            items.push(widget.clone());
        }

        child = widget.next_sibling();
    }

    if items.is_empty() {
        return None;
    }

    let count = items.len();

    if let Some(from) = from {
        let index = items.iter().position(|item| item == from)?;

        Some(
            items[if down {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            }]
            .clone(),
        )
    } else {
        Some(items[if down { 0 } else { count - 1 }].clone())
    }
}

fn focusable(widget: &gtk::Widget) -> bool {
    widget.is::<gtk::Button>() && widget.is_sensitive() && widget.is_visible()
}

fn run_action(action: &str, style: &BarSettings) {
    if let Some(name) = action.strip_prefix("shell:") {
        run_shell_action(name, style);

        return;
    }

    match command_action(action) {
        CommandAction::Reload => activate("reload"),
        CommandAction::Quit => quit(),
        CommandAction::Run(command) => command::spawn(&command),
    }
}

fn run_shell_action(name: &str, style: &BarSettings) {
    match name {
        "launcher" | "search" => Launcher::show_current(None),
        "force-quit" => Launcher::show_current(Some("quit")),
        "network" => NetworkPanel::show_current("wifi"),
        "bluetooth" => NetworkPanel::show_current("bluetooth"),
        "notifications" => NotificationCenter::toggle_current(),
        "dnd" => {
            if let Some(service) = NotificationService::current() {
                let enabled = service.has_dnd();

                service.set_dnd(!enabled);
            }
        }
        "lock" => locker::start_lock_screen(),
        "reload" => activate("reload"),
        "quit" => quit(),
        "about" => about::show(style),
        "settings" => activate("settings"),
        _ => eprintln!("whelk-rs: start: no shell action named '{name}'"),
    }
}

fn activate(action: &str) {
    if let Some(application) = gio::Application::default() {
        application.activate_action(action, None);
    }
}

fn quit() {
    if let Some(application) = gio::Application::default() {
        application.quit();
    }
}

fn install_css(style: &BarSettings) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let mut css = MENU_CSS.to_owned();

    if let Some(foreground) = style
        .foreground
        .as_deref()
        .filter(|foreground| !foreground.is_empty())
    {
        css.push_str(&format!(
            ".whelk-start-popover {{ color: {foreground}; }}\n"
        ));
    }

    if let Some(background) = style::solid(style.background.as_deref()) {
        css.push_str(&format!(
            ".whelk-start-popover {{ background-color: {background}; }}\n"
        ));
    }

    let provider = gtk::CssProvider::new();

    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(provider)
}

/// The whelk mark as one closed ribbon, in widget coordinates.
///
/// It spirals out from the centre; the radius doubles every turn, the way a
/// whelk's whorls grow, and the ribbon widens with it but never fills the gap
/// it leaves.  The result is the outer edge followed by the inner edge in
/// reverse, ready for `move_to`/`line_to` and `close_path`.
pub fn mark_polygon(box_size: f64) -> Vec<(f64, f64)> {
    const STEPS: usize = 240;

    if !box_size.is_finite() || box_size <= 0.0 {
        return Vec::new();
    }

    let turns: f64 = 3.0;
    let ratio: f64 = 2.0;
    let spin: f64 = -0.45;
    let margin = 0.06;
    let grow = ratio.ln() / (2.0 * std::f64::consts::PI);
    let c = spin.cos();
    let s = spin.sin();
    let mut px = [0.0; STEPS + 1];
    let mut py = [0.0; STEPS + 1];
    let mut half = [0.0; STEPS + 1];
    let mut outer = [(0.0, 0.0); STEPS + 1];
    let mut inner = [(0.0, 0.0); STEPS + 1];
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for index in 0..=STEPS {
        let t = index as f64 / STEPS as f64;
        let theta = t * turns * 2.0 * std::f64::consts::PI;
        let radius = (grow * theta).exp();
        let x = radius * theta.cos();
        let y = radius * theta.sin();

        px[index] = x * c - y * s;
        py[index] = x * s + y * c;
        half[index] = (0.19 * radius).max(0.26);
    }

    for index in 0..=STEPS {
        let before = index.saturating_sub(1);
        let after = (index + 1).min(STEPS);
        let dx = px[after] - px[before];
        let dy = py[after] - py[before];
        let length = dx.hypot(dy);
        let nx = -dy / length;
        let ny = dx / length;

        outer[index] = (px[index] + nx * half[index], py[index] + ny * half[index]);
        inner[index] = (px[index] - nx * half[index], py[index] - ny * half[index]);
        min_x = min_x.min(outer[index].0).min(inner[index].0);
        max_x = max_x.max(outer[index].0).max(inner[index].0);
        min_y = min_y.min(outer[index].1).min(inner[index].1);
        max_y = max_y.max(outer[index].1).max(inner[index].1);
    }

    let span = (max_x - min_x).max(max_y - min_y);

    if span <= 0.0 {
        return Vec::new();
    }

    let scale = box_size * (1.0 - 2.0 * margin) / span;
    let mid = box_size / 2.0;
    let centre_x = (min_x + max_x) / 2.0;
    let centre_y = (min_y + max_y) / 2.0;
    let mut points = Vec::with_capacity((STEPS + 1) * 2);

    for (x, y) in outer {
        points.push((mid + (x - centre_x) * scale, mid + (y - centre_y) * scale));
    }

    for (x, y) in inner.iter().rev() {
        points.push((mid + (x - centre_x) * scale, mid + (y - centre_y) * scale));
    }

    points
}

fn draw_mark(cr: &gtk::cairo::Context, box_size: f64) {
    let points = mark_polygon(box_size);

    let Some((first, rest)) = points.split_first() else {
        return;
    };

    cr.move_to(first.0, first.1);

    for (x, y) in rest {
        cr.line_to(*x, *y);
    }

    cr.close_path();
}

#[cfg(test)]
mod tests {
    use super::mark_polygon;

    #[test]
    fn the_mark_fits_inside_its_box() {
        let points = mark_polygon(18.0);

        assert_eq!(points.len(), 482);

        for (x, y) in points {
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..=18.0).contains(&x), "x={x}");
            assert!((0.0..=18.0).contains(&y), "y={y}");
        }
    }

    #[test]
    fn an_empty_or_invalid_box_has_no_mark() {
        assert!(mark_polygon(0.0).is_empty());
        assert!(mark_polygon(-1.0).is_empty());
        assert!(mark_polygon(f64::NAN).is_empty());
    }
}

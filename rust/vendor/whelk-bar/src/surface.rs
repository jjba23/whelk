// whelk-bar -- a bar surface for GTK4 Wayland shells
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! One bar surface, from a typed [`BarSettings`] to a presented window.
//!
//! The host supplies the parts that are not the bar's: the widgets a module
//! kind builds, the stylesheet for its palette, and where an overflow-shelf
//! choice is remembered.  Everything else -- the layer surface, the three rows,
//! the priority fold, the clock fallback, the context menu, auto-hide and the
//! keyboard path -- belongs to this project.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge, Layer as BarLayer, ModuleSpec};
use crate::{a11y, autohide, barmodule, sources};

/// The host half of a bar.
///
/// A host maps its own configuration and services onto this small boundary.  It
/// can build modules from any toolkit state it owns; the bar only asks for the
/// widget and gives it a place on the row.
pub trait BarHost {
    /// Build the widget for one configured module, or leave it out.
    fn module(&self, spec: &ModuleSpec) -> Option<gtk::Box>;

    /// The host's generated skin for this bar and window id.  The bar adds its
    /// own structural rules around it.
    fn css(&self, bar: &BarSettings, widget_id: &str) -> String;

    /// Remember whether the configured overflow row is open.
    fn save_overflow(&self, expanded: bool);
}

pub struct BarWindow {
    window: gtk::ApplicationWindow,
    clock_timer: Option<ClockTimer>,
    css: Option<gtk::CssProvider>,
}

impl BarWindow {
    pub fn window(&self) -> &gtk::ApplicationWindow {
        &self.window
    }
}

struct ClockTimer {
    source: Rc<RefCell<Option<gtk::glib::SourceId>>>,
    alive: Rc<Cell<bool>>,
}

impl Drop for ClockTimer {
    fn drop(&mut self) {
        self.alive.set(false);

        if let Some(source) = self.source.borrow_mut().take() {
            sources::remove(source);
        }
    }
}

impl Drop for BarWindow {
    fn drop(&mut self) {
        drop(self.clock_timer.take());

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        autohide::forget(&self.window);

        // Detach the CenterBox explicitly so the module roots receive their
        // destroy signal before the window itself goes away.
        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    application: &gtk::Application,
    bar_settings: &BarSettings,
    monitor: Option<&gtk::gdk::Monitor>,
    widget_id: &str,
    config_label: &str,
    host: Rc<dyn BarHost>,
) -> BarWindow {
    let vertical = bar_settings.is_vertical();
    let orientation = if vertical {
        gtk::Orientation::Vertical
    } else {
        gtk::Orientation::Horizontal
    };
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("whelk")
        .build();

    window.set_widget_name(widget_id);
    window.add_css_class("whelk-surface");
    window.set_decorated(false);
    // A layer surface is sized by the compositor -- an exclusive zone, a scale
    // change or a margin all shrink or grow it -- and a non-resizable window
    // ignores those configure events, laying out at its first size instead.
    window.set_resizable(true);

    let bar = gtk::CenterBox::new();
    bar.add_css_class("whelk-bar");
    bar.set_orientation(orientation);
    bar.set_size_request(
        if vertical {
            bar_settings.height.max(1)
        } else {
            -1
        },
        if vertical {
            -1
        } else {
            bar_settings.height.max(1)
        },
    );
    bar.set_tooltip_text(Some(config_label));

    let spacing = bar_settings.spacing.max(0);
    let start = gtk::Box::new(orientation, spacing);
    let center = gtk::Box::new(orientation, spacing);
    let end = gtk::Box::new(orientation, spacing);

    start.add_css_class("whelk-start");
    center.add_css_class("whelk-center");
    end.add_css_class("whelk-end");

    if vertical {
        start.set_halign(gtk::Align::Center);
        center.set_halign(gtk::Align::Center);
        end.set_halign(gtk::Align::Center);
    }

    append_modules(&start, &bar_settings.left, &*host);
    append_modules(&center, &bar_settings.center, &*host);

    let mut readings = append_modules(&end, &bar_settings.right, &*host);

    let configured_clock = bar_settings.module("clock").is_some()
        || bar_settings
            .overflow
            .iter()
            .any(|module| module.kind == "clock");
    let clock_timer = if !configured_clock {
        let clock = gtk::Label::new(None);
        let clock_format = bar_settings
            .clock_format()
            .unwrap_or_else(|| "%H:%M".to_owned());
        let weak_clock = clock.downgrade();
        let source = Rc::new(RefCell::new(None));
        let alive = Rc::new(Cell::new(true));
        let callback_source = source.clone();
        let callback_alive = alive.clone();
        let timer_format = clock_format.clone();

        let timer = gtk::glib::timeout_add_seconds_local(1, move || {
            if !callback_alive.get() {
                return gtk::glib::ControlFlow::Break;
            }

            let Some(clock) = weak_clock.upgrade() else {
                *callback_source.borrow_mut() = None;

                return gtk::glib::ControlFlow::Break;
            };

            update_clock(&clock, &timer_format);

            gtk::glib::ControlFlow::Continue
        });

        *source.borrow_mut() = Some(timer);

        clock.add_css_class("whelk-clock");
        clock.add_css_class("whelk-module");
        update_clock(&clock, &clock_format);
        end.append(&clock);
        readings.push((
            clock.clone().upcast::<gtk::Widget>(),
            crate::bar::KEEP_PRIORITY,
        ));

        Some(ClockTimer { source, alive })
    } else {
        None
    };

    let overflow = append_overflow(&end, bar_settings, Rc::clone(&host), vertical);
    let more = build_folded_shelf(&end, vertical);

    bar.set_start_widget(Some(&start));
    bar.set_center_widget(Some(&center));
    bar.set_end_widget(Some(&end));
    window.set_child(Some(&bar));
    build_bar_context_menu(application, &bar, &start, &center, &end);

    let css = install_css(bar_settings, widget_id, &*host);

    if layer_shell_available() {
        configure_layer_shell(&window, bar_settings, &bar, monitor);
    } else {
        window.set_default_size(
            if vertical {
                bar_settings.height.max(32)
            } else {
                720
            },
            if vertical {
                720
            } else {
                bar_settings.height.max(32)
            },
        );
    }

    if bar_settings.keyboard {
        a11y::install_keyboard_navigation(&window, &[start.clone(), center.clone(), end.clone()]);
    }

    fold_after_map(
        FoldState {
            bar: bar.clone(),
            start: start.clone(),
            center: center.clone(),
            end: end.clone(),
            overflow,
            more,
            readings,
            spacing: bar_settings.spacing,
            padding: bar_settings.padding,
            vertical,
        },
        &window,
    );
    window.present();

    autohide::install(&window, bar_settings, monitor);

    BarWindow {
        window,
        clock_timer,
        css,
    }
}

fn configure_layer_shell(
    window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    content: &gtk::CenterBox,
    monitor: Option<&gtk::gdk::Monitor>,
) {
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
    window.set_exclusive_zone(if bar.exclusive {
        bar_thickness(bar, content)
    } else {
        -1
    });
    window.set_keyboard_mode(if bar.keyboard {
        KeyboardMode::OnDemand
    } else {
        KeyboardMode::None
    });

    if let Some(monitor) = monitor {
        window.set_monitor(Some(monitor));
    }

    apply_surface_size(window, bar, content, monitor);
}

fn bar_thickness(bar: &BarSettings, content: &gtk::CenterBox) -> i32 {
    if !bar.is_vertical() {
        return bar.height.max(1);
    }

    let (_, natural, _, _) = content.measure(gtk::Orientation::Horizontal, -1);

    natural.max(bar.height).max(1)
}

fn apply_surface_size(
    window: &gtk::ApplicationWindow,
    bar: &BarSettings,
    content: &gtk::CenterBox,
    monitor: Option<&gtk::gdk::Monitor>,
) {
    let fallback = gtk::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gtk::gdk::Monitor>();
    let Some(monitor) = monitor.or(fallback.as_ref()) else {
        return;
    };
    let geometry = monitor.geometry();

    let (width, height) = match bar.edge {
        BarEdge::Top | BarEdge::Bottom => {
            (geometry.width() - bar.margin_left - bar.margin_right, -1)
        }
        BarEdge::Left | BarEdge::Right => (
            bar_thickness(bar, content),
            geometry.height() - bar.margin_top - bar.margin_bottom,
        ),
    };

    window.set_default_size(width.max(1), if height < 0 { -1 } else { height.max(1) });
}

pub(crate) fn edge_for(edge: BarEdge) -> Edge {
    match edge {
        BarEdge::Top => Edge::Top,
        BarEdge::Bottom => Edge::Bottom,
        BarEdge::Left => Edge::Left,
        BarEdge::Right => Edge::Right,
    }
}

pub(crate) fn layer_for(layer: BarLayer) -> Layer {
    match layer {
        BarLayer::Background => Layer::Background,
        BarLayer::Bottom => Layer::Bottom,
        BarLayer::Top => Layer::Top,
        BarLayer::Overlay => Layer::Overlay,
    }
}

pub fn layer_shell_available() -> bool {
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };

    display.backend().is_wayland() && gtk4_layer_shell::is_supported()
}

fn append_modules(
    section: &gtk::Box,
    modules: &[ModuleSpec],
    host: &dyn BarHost,
) -> Vec<(gtk::Widget, i32)> {
    let mut readings = Vec::new();

    for module in modules {
        if let Some(widget) = host.module(module) {
            section.append(&widget);
            readings.push((
                widget.upcast::<gtk::Widget>(),
                crate::bar::module_priority(module),
            ));
        }
    }

    readings
}

fn append_overflow(
    end: &gtk::Box,
    bar_settings: &BarSettings,
    host: Rc<dyn BarHost>,
    vertical: bool,
) -> Option<OverflowRow> {
    if bar_settings.overflow.is_empty() {
        return None;
    }

    let expanded = bar_settings.overflow_expanded;
    let orientation = if vertical {
        gtk::Orientation::Vertical
    } else {
        gtk::Orientation::Horizontal
    };
    let overflow = gtk::Box::new(orientation, bar_settings.spacing.max(0));
    overflow.add_css_class("whelk-overflow");

    if vertical {
        overflow.set_halign(gtk::Align::Center);
    } else {
        overflow.set_valign(gtk::Align::Center);
    }

    append_modules(&overflow, &bar_settings.overflow, &*host);

    let (closed_icon, open_icon) = if vertical {
        ("pan-up-symbolic", "pan-down-symbolic")
    } else {
        ("pan-start-symbolic", "pan-end-symbolic")
    };
    let toggle = gtk::ToggleButton::new();
    let icon = gtk::Image::from_icon_name(closed_icon);

    icon.set_pixel_size(16);
    toggle.set_child(Some(&icon));
    toggle.add_css_class("flat");
    toggle.add_css_class("whelk-overflow-toggle");

    if vertical {
        toggle.set_halign(gtk::Align::Center);
    } else {
        toggle.set_valign(gtk::Align::Center);
    }

    toggle.set_active(expanded);
    overflow.set_visible(expanded);
    icon.set_icon_name(Some(if expanded { open_icon } else { closed_icon }));
    toggle.set_tooltip_text(Some(if expanded {
        "Extra items · click to hide"
    } else {
        "Extra items · click to show"
    }));

    let overflow_for_toggle = overflow.clone();
    let icon_for_toggle = icon.clone();
    toggle.connect_toggled(move |button| {
        let expanded = button.is_active();

        overflow_for_toggle.set_visible(expanded);
        host.save_overflow(expanded);
        icon_for_toggle.set_icon_name(Some(if expanded { open_icon } else { closed_icon }));
        button.set_tooltip_text(Some(if expanded {
            "Extra items · click to hide"
        } else {
            "Extra items · click to show"
        }));
    });

    end.append(&overflow);
    end.append(&toggle);

    Some(OverflowRow {
        shelf: overflow,
        toggle,
    })
}

struct OverflowRow {
    shelf: gtk::Box,
    toggle: gtk::ToggleButton,
}

struct FoldedShelf {
    button: gtk::MenuButton,
    count: gtk::Label,
    shelf: gtk::Box,
}

fn build_folded_shelf(end: &gtk::Box, vertical: bool) -> FoldedShelf {
    let shelf = gtk::Box::new(gtk::Orientation::Vertical, 4);

    shelf.add_css_class("whelk-overflow-box");

    let popover = gtk::Popover::new();

    popover.set_child(Some(&shelf));

    let count = gtk::Label::new(Some("+1"));

    count.add_css_class("whelk-folded-count");

    let button = gtk::MenuButton::new();

    button.set_child(Some(&count));
    button.add_css_class("flat");
    button.add_css_class("whelk-overflow-more");
    button.set_tooltip_text(Some("Readings that did not fit"));
    button.set_popover(Some(&popover));
    button.set_visible(false);

    if vertical {
        button.set_halign(gtk::Align::Center);
    }

    barmodule::attach_exclusive_popover(&button, &popover);
    end.append(&button);

    FoldedShelf {
        button,
        count,
        shelf,
    }
}

struct FoldState {
    bar: gtk::CenterBox,
    start: gtk::Box,
    center: gtk::Box,
    end: gtk::Box,
    overflow: Option<OverflowRow>,
    more: FoldedShelf,
    readings: Vec<(gtk::Widget, i32)>,
    spacing: i32,
    padding: i32,
    vertical: bool,
}

fn fold_after_map(state: FoldState, window: &gtk::ApplicationWindow) {
    let state = Rc::new(state);
    let armed = Rc::new(Cell::new(false));

    let map_state = Rc::clone(&state);

    window.connect_map(move |_| {
        if armed.replace(true) {
            return;
        }

        let state = Rc::clone(&map_state);

        gtk::glib::timeout_add_local_once(Duration::from_millis(100), move || {
            collapse_end(&state);
        });
    });

    if let Some(surface) = window.surface() {
        let state = Rc::clone(&state);

        if state.vertical {
            surface.connect_height_notify(move |_| {
                collapse_end(&state);
            });
        } else {
            surface.connect_width_notify(move |_| {
                collapse_end(&state);
            });
        }
    }
}

fn widget_extent(widget: &gtk::Widget, vertical: bool) -> i32 {
    if !widget.is_visible() {
        return 0;
    }

    let (extent, axis) = if vertical {
        (widget.height(), gtk::Orientation::Vertical)
    } else {
        (widget.width(), gtk::Orientation::Horizontal)
    };

    if extent > 0 {
        return extent;
    }

    let (_, natural, _, _) = widget.measure(axis, -1);

    natural.max(0)
}

fn reading_extent(widget: &gtk::Widget, vertical: bool) -> i32 {
    let extent = widget_extent(widget, vertical);

    if extent > 0 { extent } else { 24 }
}

fn collapse_end(state: &FoldState) {
    let root_extent = if state.vertical {
        state.bar.height()
    } else {
        state.bar.width()
    };

    if root_extent <= 1 || state.readings.is_empty() {
        return;
    }

    let mut used_by_others = widget_extent(state.start.upcast_ref::<gtk::Widget>(), state.vertical)
        + widget_extent(state.center.upcast_ref::<gtk::Widget>(), state.vertical)
        + 2 * state.padding.max(0);

    if let Some(overflow) = &state.overflow {
        used_by_others += widget_extent(overflow.shelf.upcast_ref::<gtk::Widget>(), state.vertical)
            + widget_extent(overflow.toggle.upcast_ref::<gtk::Widget>(), state.vertical);
    }

    let available = (root_extent - used_by_others - 16).max(0);
    let readings: Vec<crate::bar::Reading> = state
        .readings
        .iter()
        .map(|(widget, priority)| crate::bar::Reading {
            width: reading_extent(widget, state.vertical),
            priority: *priority,
        })
        .collect();
    let kept = crate::bar::fold_visible(&readings, available, state.spacing);

    while let Some(child) = state.more.shelf.first_child() {
        state.more.shelf.remove(&child);
    }

    let row = state.end.clone().upcast::<gtk::Widget>();

    for (widget, _) in &state.readings {
        if widget.parent().as_ref() == Some(&row) {
            state.end.remove(widget);
        }
    }

    for index in kept.iter().rev() {
        if let Some((widget, _)) = state.readings.get(*index) {
            state.end.prepend(widget);
        }
    }

    for (index, (widget, _)) in state.readings.iter().enumerate() {
        if !kept.contains(&index) {
            state.more.shelf.append(widget);
        }
    }

    let folded = state.readings.len() - kept.len();

    if folded > 0 {
        state.more.count.set_text(&format!("+{folded}"));
        state.more.button.set_tooltip_text(Some(&format!(
            "{folded} reading{} did not fit · click to see {}",
            if folded == 1 { "" } else { "s" },
            if folded == 1 { "it" } else { "them" }
        )));
    }

    state.more.button.set_visible(folded > 0);
}

fn build_bar_context_menu(
    application: &gtk::Application,
    bar: &gtk::CenterBox,
    start: &gtk::Box,
    center: &gtk::Box,
    end: &gtk::Box,
) {
    let popover = gtk::Popover::new();

    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_parent(bar);
    barmodule::watch_popover(&popover);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-bar-menu");

    let heading = gtk::Label::new(Some("whelk"));
    heading.add_css_class("whelk-bar-menu-heading");
    heading.set_xalign(0.0);
    heading.set_margin_start(8);
    heading.set_margin_end(8);
    heading.set_margin_top(4);
    heading.set_margin_bottom(4);

    let edit = gtk::Button::with_label("Edit settings…");
    edit.add_css_class("flat");
    edit.set_halign(gtk::Align::Start);

    let reload = gtk::Button::with_label("Reload settings");
    reload.add_css_class("flat");
    reload.set_halign(gtk::Align::Start);

    menu.append(&heading);
    menu.append(&edit);
    menu.append(&reload);
    popover.set_child(Some(&menu));

    let application_for_edit = application.clone();
    let popover_for_edit = popover.clone();

    edit.connect_clicked(move |_| {
        application_for_edit.activate_action("settings", None);
        popover_for_edit.popdown();
    });

    let application_for_reload = application.clone();
    let popover_for_reload = popover.clone();

    reload.connect_clicked(move |_| {
        application_for_reload.activate_action("reload", None);
        popover_for_reload.popdown();
    });

    let gesture = gtk::GestureClick::new();

    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);

    let bar_for_pick = bar.clone();
    let start = start.clone().upcast::<gtk::Widget>();
    let center = center.clone().upcast::<gtk::Widget>();
    let end = end.clone().upcast::<gtk::Widget>();

    gesture.connect_pressed(move |_, _, x, y| {
        let root = bar_for_pick.clone().upcast::<gtk::Widget>();
        let target = bar_for_pick.pick(x, y, gtk::PickFlags::DEFAULT);
        let structural = target.as_ref().is_some_and(|target| {
            target == &root || target == &start || target == &center || target == &end
        });

        if !structural {
            return;
        }

        barmodule::close_popovers(Some(&popover));
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    bar.add_controller(gesture);
}

fn update_clock(label: &gtk::Label, format: &str) {
    let text = gtk::glib::DateTime::now_local()
        .and_then(|now| now.format(format))
        .map(|formatted| formatted.to_string())
        .unwrap_or_else(|_| "--:--".to_owned());

    label.set_text(&text);
}

fn install_css(bar: &BarSettings, widget_id: &str, host: &dyn BarHost) -> Option<gtk::CssProvider> {
    let display = gtk::gdk::Display::default()?;
    let base = "
        .whelk-surface {
            background-color: transparent;
        }
        .whelk-clock {
            font-weight: 700;
        }
        .whelk-bar-menu {
            padding: 4px;
        }
        .whelk-bar-menu-heading {
            font-weight: 700;
            opacity: 0.7;
        }
        .whelk-overflow-box {
            padding: 6px;
        }
        .whelk-module {
            border-radius: 6px;
            transition: background-color 120ms ease-in-out;
        }
        .whelk-module:hover {
            background-color: alpha(currentColor, 0.10);
        }
        .whelk-label {
            font-feature-settings: \"tnum\";
        }
        .whelk-battery.warning {
            color: #ffb340;
        }
        .whelk-battery.critical {
            color: #ff5f56;
            font-weight: bold;
        }
        .whelk-wifi.off, .whelk-bluetooth.off {
            opacity: 0.55;
        }
        .whelk-window {
            font-weight: 600;
        }
        .whelk-workspace.inactive {
            opacity: 0.62;
        }
        .whelk-workspace.empty {
            opacity: 0.38;
        }
        .whelk-media-controls button {
            color: inherit;
            padding: 0 3px;
            min-width: 20px;
            min-height: 20px;
            font-size: 14px;
        }
        ";
    let generated = host.css(bar, widget_id);
    let css = gtk::CssProvider::new();

    css.load_from_string(&format!("{base}\n{generated}"));

    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Some(css)
}

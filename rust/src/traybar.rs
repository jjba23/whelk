// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `tray` bar module, ported from `modules/tray.c`.
//!
//! Other programs' status icons speak `org.kde.StatusNotifierItem`.  The
//! watcher owns the list; this module follows its Registered/Unregistered
//! signals, opens one `GDBusProxy` per item, and paints the icon from the
//! item's own properties.  Actions and menus go back over D-Bus: a left click
//! calls `Activate`, a right click reads the item's `com.canonical.dbusmenu`
//! layout and paints it as a popover.
//!
//! The widget callbacks hold a weak state reference, and every asynchronous
//! answer carries the watcher epoch or the item's request serial.  A late proxy
//! or menu reply therefore cannot resurrect a removed item or replace a newer
//! menu.  No callback needs to keep a raw pointer into a tree or a widget alive.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::chord::ChordStyle;
use crate::tray::{self, Pixmap};
use crate::trayicon::tray_icon_file;
use crate::traymenu::{self, TrayMenuAction, TrayMenuEntry};

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const DEFAULT_SPACING: i32 = 6;
const MENU_TIMEOUT_MS: i32 = 1_500;

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    let spacing = module
        .integer("spacing")
        .unwrap_or(i64::from(DEFAULT_SPACING))
        .clamp(0, 64) as i32;
    root.set_spacing(spacing);
    host.add_class("whelk-tray-box");

    let more = gtk::MenuButton::new();
    let chevron = gtk::Image::from_icon_name("pan-down-symbolic");

    chevron.set_pixel_size(16);
    more.set_child(Some(&chevron));
    more.add_css_class("flat");
    more.add_css_class("whelk-tray-more");
    more.set_tooltip_text(Some("More status icons"));
    more.set_visible(false);

    let overflow = gtk::Box::new(gtk::Orientation::Vertical, 4);

    overflow.add_css_class("whelk-tray-overflow");

    let overflow_popover = gtk::Popover::new();

    overflow_popover.set_has_arrow(false);
    overflow_popover.set_child(Some(&overflow));
    more.set_popover(Some(&overflow_popover));
    root.append(&more);

    let state = Rc::new(RefCell::new(TrayState {
        host,
        overflow,
        more,
        items: Vec::new(),
        bus: None,
        watcher: None,
        watcher_id: None,
        watcher_epoch: 0,
        icon_paths: HashSet::new(),
        hide: module.text("hide").unwrap_or_default(),
        hide_network: context.bar.native_network,
        hide_bluetooth: context.bar.native_bluetooth,
        max_items: tray::max_items(module.integer("max-items")),
        epoch: 0,
    }));

    start_watcher(&state);

    let destroy_state = Rc::clone(&state);

    root.connect_destroy(move |_| {
        destroy_state.borrow_mut().shutdown();
    });

    Some(root)
}

/// The module has no reading to refresh, so the shared host starts no timer.
const DEFAULT_INTERVAL_SECONDS: u32 = 0;

struct TrayState {
    host: ModuleHost,
    overflow: gtk::Box,
    more: gtk::MenuButton,
    items: Vec<Rc<TrayItem>>,
    bus: Option<gio::DBusConnection>,
    watcher: Option<gio::DBusProxy>,
    watcher_id: Option<gio::WatcherId>,
    /// Invalidates a watcher proxy request that outlived its owner.
    watcher_epoch: u64,
    icon_paths: HashSet<String>,
    hide: String,
    hide_network: bool,
    hide_bluetooth: bool,
    max_items: usize,
    /// Bumped whenever the watcher list is reset.  A proxy answer from an older
    /// list is then stale even if its entry string comes back.
    epoch: u64,
}

impl TrayState {
    fn shutdown(&mut self) {
        if let Some(id) = self.watcher_id.take() {
            gio::bus_unwatch_name(id);
        }

        self.watcher.take();
        self.items.clear();
        self.bus.take();
    }

    fn clear_items(&mut self) {
        self.items.clear();
        self.epoch = self.epoch.wrapping_add(1);
        reflow(self);
    }

    fn find(&self, entry: &str) -> Option<Rc<TrayItem>> {
        self.items.iter().find(|item| item.entry == entry).cloned()
    }
}

impl Drop for TrayState {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct TrayItem {
    entry: String,
    service: String,
    proxy: gio::DBusProxy,
    image: gtk::Image,
    menu: RefCell<Option<gtk::Popover>>,
    /// Invalidate an older `AboutToShow`/`GetLayout` pair.
    request: Cell<u64>,
}

impl Drop for TrayItem {
    fn drop(&mut self) {
        if let Some(menu) = self.menu.borrow_mut().take() {
            menu.popdown();
            menu.unparent();
        }

        self.image.unparent();
    }
}

// ---------------------------------------------------------------------------
// The watcher
// ---------------------------------------------------------------------------

fn start_watcher(state: &Rc<RefCell<TrayState>>) {
    let weak = Rc::downgrade(state);

    gio::bus_get(
        gio::BusType::Session,
        gio::Cancellable::NONE,
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let bus = match result {
                Ok(bus) => bus,
                Err(error) => {
                    eprintln!("whelk: tray: no session bus, so there is no tray: {error}");
                    return;
                }
            };

            state.borrow_mut().bus = Some(bus.clone());

            let weak_appeared = Rc::downgrade(&state);
            let weak_vanished = Rc::downgrade(&state);
            let watcher_id = gio::bus_watch_name_on_connection(
                &bus,
                WATCHER_NAME,
                gio::BusNameWatcherFlags::NONE,
                move |connection, _name, _owner| {
                    let Some(state) = weak_appeared.upgrade() else {
                        return;
                    };
                    let attempt = {
                        let mut state = state.borrow_mut();

                        state.watcher_epoch = state.watcher_epoch.wrapping_add(1);
                        state.watcher_epoch
                    };

                    open_watcher(&state, &connection, attempt);
                },
                move |_connection, _name| {
                    let Some(state) = weak_vanished.upgrade() else {
                        return;
                    };

                    let mut state = state.borrow_mut();

                    state.watcher_epoch = state.watcher_epoch.wrapping_add(1);
                    state.watcher.take();
                    state.clear_items();
                },
            );

            state.borrow_mut().watcher_id = Some(watcher_id);
        },
    );
}

fn open_watcher(state: &Rc<RefCell<TrayState>>, connection: &gio::DBusConnection, attempt: u64) {
    let weak = Rc::downgrade(state);

    gio::DBusProxy::new(
        connection,
        gio::DBusProxyFlags::NONE,
        None,
        Some(WATCHER_NAME),
        WATCHER_PATH,
        WATCHER_INTERFACE,
        None::<&gio::Cancellable>,
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if state.borrow().watcher_epoch != attempt {
                return;
            }
            let watcher = match result {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("whelk: tray: StatusNotifierWatcher is not reachable: {error}");
                    return;
                }
            };

            let weak_signal = Rc::downgrade(&state);

            watcher.connect_local("g-signal", false, move |values| {
                let signal = values.get(2).and_then(|value| value.get::<String>().ok());
                let entry = values
                    .get(3)
                    .and_then(|value| value.get::<glib::Variant>().ok())
                    .and_then(|parameters| parameters.get::<(String,)>())
                    .map(|(entry,)| entry);

                if let (Some(signal), Some(entry)) = (signal, entry)
                    && let Some(state) = weak_signal.upgrade()
                {
                    match signal.as_str() {
                        "StatusNotifierItemRegistered" => add_entry(&state, &entry),
                        "StatusNotifierItemUnregistered" => remove_entry(&state, &entry),
                        _ => {}
                    }
                }

                None
            });

            let host_name = state
                .borrow()
                .bus
                .as_ref()
                .and_then(|bus| bus.unique_name())
                .map(|name| name.to_string())
                .unwrap_or_default();

            if !host_name.is_empty() {
                watcher.call(
                    "RegisterStatusNotifierHost",
                    Some(&(host_name,).to_variant()),
                    gio::DBusCallFlags::NONE,
                    -1,
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            }

            let entries = registered_items(&watcher);

            {
                let mut state = state.borrow_mut();

                if state.watcher.is_some() {
                    state.clear_items();
                }

                state.watcher = Some(watcher);
            }

            for entry in entries {
                add_entry(&state, &entry);
            }
        },
    );
}

fn registered_items(watcher: &gio::DBusProxy) -> Vec<String> {
    // The standard property, plus the name an early watcher used.  An item
    // already running when the bar starts is only in this list.
    watcher
        .cached_property("RegisteredStatusNotifierItems")
        .or_else(|| watcher.cached_property("RegisteredStatusItems"))
        .and_then(|items| items.get::<Vec<String>>())
        .unwrap_or_default()
}

fn add_entry(state: &Rc<RefCell<TrayState>>, entry: &str) {
    let Some((service, path)) = tray::split_entry(entry) else {
        return;
    };

    let (bus, epoch, hide, hide_network, hide_bluetooth) = {
        let state = state.borrow();

        if state.find(entry).is_some() {
            return;
        }
        let Some(bus) = state.bus.clone() else {
            return;
        };

        (
            bus,
            state.epoch,
            state.hide.clone(),
            state.hide_network,
            state.hide_bluetooth,
        )
    };

    let weak = Rc::downgrade(state);
    let callback_entry = entry.to_owned();
    let item_service = service.clone();

    gio::DBusProxy::new(
        &bus,
        gio::DBusProxyFlags::NONE,
        None,
        Some(&service),
        &path,
        ITEM_INTERFACE,
        None::<&gio::Cancellable>,
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let proxy = match result {
                Ok(proxy) => proxy,
                Err(error) => {
                    eprintln!("whelk: tray: item {callback_entry} is not reachable: {error}");
                    return;
                }
            };

            if state.borrow().epoch != epoch || state.borrow().find(&callback_entry).is_some() {
                return;
            }

            let id = cached_string(&proxy, "Id");
            let title = cached_string(&proxy, "Title");

            if tray::should_hide(
                id.as_deref(),
                title.as_deref(),
                &hide,
                hide_network,
                hide_bluetooth,
            ) {
                eprintln!(
                    "whelk: tray: hiding '{}': whelk reads this itself",
                    id.or(title).unwrap_or_else(|| callback_entry.clone())
                );
                return;
            }

            let image = gtk::Image::new();

            image.add_css_class("whelk-tray-item");
            image.set_pixel_size(tray::DEFAULT_ITEM_SIZE);
            image.set_valign(gtk::Align::Center);

            let item = Rc::new(TrayItem {
                entry: callback_entry,
                service: item_service,
                proxy,
                image,
                menu: RefCell::new(None),
                request: Cell::new(0),
            });

            attach_item_handlers(&state, &item);
            {
                let mut state = state.borrow_mut();

                state.items.push(Rc::clone(&item));
                reflow(&state);
            }
            refresh_item(&item, &mut state.borrow_mut().icon_paths);
        },
    );
}

fn remove_entry(state: &Rc<RefCell<TrayState>>, entry: &str) {
    let mut state = state.borrow_mut();
    let Some(index) = state.items.iter().position(|item| item.entry == entry) else {
        return;
    };

    state.items.remove(index);
    reflow(&state);
}

/// The first icons stay on the bar; the rest live behind the chevron.  The
/// widgets are the same either way, so a click still reaches the item wherever
/// it is drawn.
fn reflow(state: &TrayState) {
    let Some(root) = state.host.root() else {
        return;
    };
    let visible = tray::visible_count(state.items.len(), state.max_items);

    for (index, item) in state.items.iter().enumerate() {
        let target = if index < visible {
            &root
        } else {
            &state.overflow
        };
        let target_widget = target.clone().upcast::<gtk::Widget>();

        if item.image.parent().as_ref() == Some(&target_widget) {
            continue;
        }

        item.image.unparent();
        target.append(&item.image);
    }

    root.reorder_child_after(&state.more, None::<&gtk::Widget>);
    state
        .more
        .set_visible(tray::needs_overflow(state.items.len(), state.max_items));
}

// ---------------------------------------------------------------------------
// One item
// ---------------------------------------------------------------------------

fn attach_item_handlers(state: &Rc<RefCell<TrayState>>, item: &Rc<TrayItem>) {
    let weak_signal = Rc::downgrade(state);
    let signal_entry = item.entry.clone();

    item.proxy.connect_local("g-signal", false, move |values| {
        let signal = values.get(2).and_then(|value| value.get::<String>().ok());

        if matches!(
            signal.as_deref(),
            Some("NewIcon" | "NewTitle" | "NewToolTip" | "NewStatus")
        ) && let Some(state) = weak_signal.upgrade()
        {
            refresh_entry(&state, &signal_entry);
        }

        None
    });

    let weak_properties = Rc::downgrade(state);
    let properties_entry = item.entry.clone();

    item.proxy
        .connect_local("g-properties-changed", false, move |_| {
            if let Some(state) = weak_properties.upgrade() {
                refresh_entry(&state, &properties_entry);
            }

            None
        });

    let left = gtk::GestureClick::new();
    let weak_left = Rc::downgrade(state);
    let left_entry = item.entry.clone();

    left.set_button(gtk::gdk::BUTTON_PRIMARY);
    left.connect_pressed(move |_, _, _, _| {
        if let Some(state) = weak_left.upgrade() {
            on_left_click(&state, &left_entry);
        }
    });
    item.image.add_controller(left);

    let right = gtk::GestureClick::new();
    let weak_right = Rc::downgrade(state);
    let right_entry = item.entry.clone();

    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.connect_pressed(move |_, _, _, _| {
        if let Some(state) = weak_right.upgrade() {
            show_menu(&state, &right_entry);
        }
    });
    item.image.add_controller(right);

    item.image.set_cursor_from_name(Some("pointer"));
}

fn refresh_entry(state: &Rc<RefCell<TrayState>>, entry: &str) {
    let Some(item) = state.borrow().find(entry) else {
        return;
    };

    refresh_item(&item, &mut state.borrow_mut().icon_paths);
}

fn refresh_item(item: &TrayItem, icon_paths: &mut HashSet<String>) {
    let icon_name = cached_string(&item.proxy, "IconName");
    let theme_path = cached_string(&item.proxy, "IconThemePath");

    if let Some(path) = theme_path.as_deref() {
        use_icon_path(icon_paths, path);
    }

    if let Some(name) = icon_name.as_deref().filter(|name| !name.is_empty()) {
        if let Some(file) = tray_icon_file(theme_path.as_deref().unwrap_or_default(), name) {
            item.image.set_from_file(Some(file));
        } else {
            item.image.set_icon_name(Some(name));
        }
    } else if let Some(pixmaps) = item.proxy.cached_property("IconPixmap")
        && let Some(texture) = texture_from_pixmaps(&pixmaps)
    {
        item.image.set_paintable(Some(&texture));
    } else {
        item.image.set_icon_name(Some("application-x-executable"));
    }

    let label = cached_string(&item.proxy, "Title")
        .filter(|title| !title.is_empty())
        .or_else(|| cached_string(&item.proxy, "Id"))
        .unwrap_or_default();

    item.image.set_tooltip_text(Some(&label));
}

fn use_icon_path(paths: &mut HashSet<String>, path: &str) {
    if path.is_empty() || !std::path::Path::new(path).is_absolute() {
        return;
    }

    if !paths.insert(path.to_owned()) {
        return;
    }

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(path);
    }
}

fn texture_from_pixmaps(pixmaps: &glib::Variant) -> Option<gtk::gdk::MemoryTexture> {
    let pixmap = first_pixmap(pixmaps)?;
    let stride = usize::try_from(pixmap.width).ok()?.checked_mul(4)?;
    let bytes = glib::Bytes::from_owned(pixmap.rgba);

    Some(gtk::gdk::MemoryTexture::new(
        pixmap.width,
        pixmap.height,
        gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &bytes,
        stride,
    ))
}

fn first_pixmap(pixmaps: &glib::Variant) -> Option<Pixmap> {
    let pixmap_type = glib::VariantTy::new("a(iiay)").ok()?;

    if !pixmaps.is_type(pixmap_type) {
        return None;
    }

    let first = pixmaps.try_child_value(0)?;
    let (width, height, bytes) = first.get::<(i32, i32, Vec<u8>)>()?;

    tray::argb32_pixmap_to_rgba(width, height, &bytes)
}

fn on_left_click(state: &Rc<RefCell<TrayState>>, entry: &str) {
    let Some(item) = state.borrow().find(entry) else {
        return;
    };

    if cached_bool(&item.proxy, "ItemIsMenu").unwrap_or(false) {
        show_menu(state, entry);
    } else {
        call_item(&item, "Activate");
    }
}

fn call_item(item: &TrayItem, method: &str) {
    item.proxy.call(
        method,
        Some(&(0i32, 0i32).to_variant()),
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |_| {},
    );
}

fn cached_string(proxy: &gio::DBusProxy, key: &str) -> Option<String> {
    proxy
        .cached_property(key)
        .and_then(|value| value.str().map(str::to_owned))
}

fn cached_bool(proxy: &gio::DBusProxy, key: &str) -> Option<bool> {
    proxy
        .cached_property(key)
        .and_then(|value| value.get::<bool>())
}

// ---------------------------------------------------------------------------
// The item's own menu
// ---------------------------------------------------------------------------

fn show_menu(state: &Rc<RefCell<TrayState>>, entry: &str) {
    let Some(item) = state.borrow().find(entry) else {
        return;
    };

    let menu_path =
        cached_string(&item.proxy, "Menu").filter(|path| !path.is_empty() && path != "/");

    let Some(menu_path) = menu_path else {
        // No menu object: ContextMenu is all the protocol offers the item.
        call_item(&item, "ContextMenu");
        return;
    };

    let service = item
        .proxy
        .name_owner()
        .map(|owner| owner.to_string())
        .filter(|owner| !owner.is_empty())
        .unwrap_or_else(|| item.service.clone());
    let Some(bus) = state.borrow().bus.clone() else {
        call_item(&item, "ContextMenu");
        return;
    };

    let popover = menu_popover(&item);
    let loading = gtk::Label::new(Some("Loading menu\u{2026}"));

    loading.add_css_class("whelk-tray-empty");
    popover.set_child(Some(&loading));
    popover.popup();

    let serial = item.request.get().wrapping_add(1);

    item.request.set(serial);

    let weak = Rc::downgrade(state);
    let callback_item = Rc::clone(&item);
    let about_service = service.clone();
    let about_path = menu_path.clone();

    bus.call(
        Some(&service),
        &menu_path,
        MENU_INTERFACE,
        "AboutToShow",
        Some(&(0i32,).to_variant()),
        glib::VariantTy::new("(b)").ok(),
        gio::DBusCallFlags::NONE,
        MENU_TIMEOUT_MS,
        None::<&gio::Cancellable>,
        move |_result| {
            let Some(state) = weak.upgrade() else {
                return;
            };

            if !menu_request_current(&state, &callback_item, serial) {
                return;
            }

            fetch_layout(&state, &callback_item, &about_service, &about_path, serial);
        },
    );
}

fn fetch_layout(
    state: &Rc<RefCell<TrayState>>,
    item: &Rc<TrayItem>,
    service: &str,
    path: &str,
    serial: u64,
) {
    let Some(bus) = state.borrow().bus.clone() else {
        return;
    };
    let weak = Rc::downgrade(state);
    let callback_item = Rc::clone(item);
    let service = service.to_owned();
    let path = path.to_owned();
    let callback_service = service.clone();
    let callback_path = path.clone();

    bus.call(
        Some(&service),
        &path,
        MENU_INTERFACE,
        "GetLayout",
        Some(&(0i32, -1i32).to_variant()),
        glib::VariantTy::new("(u(ia{sv}av))").ok(),
        gio::DBusCallFlags::NONE,
        MENU_TIMEOUT_MS,
        None::<&gio::Cancellable>,
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };

            if !menu_request_current(&state, &callback_item, serial) {
                return;
            }

            let Some(popover) = callback_item.menu.borrow().clone() else {
                return;
            };
            let entries = match result {
                Ok(reply) => traymenu::parse_layout(
                    &reply,
                    &callback_service,
                    &callback_path,
                    ChordStyle::Cua,
                ),
                Err(error) => {
                    eprintln!("whelk: tray: could not read the applet menu: {error}");
                    Vec::new()
                }
            };

            let child: gtk::Widget = if entries.is_empty() {
                let empty = gtk::Label::new(Some("This applet has no menu"));

                empty.add_css_class("whelk-tray-empty");
                empty.upcast()
            } else {
                menu_box(&entries, &state, &popover).upcast()
            };

            popover.set_child(Some(&child));
        },
    );
}

fn menu_request_current(state: &Rc<RefCell<TrayState>>, item: &Rc<TrayItem>, serial: u64) -> bool {
    item.request.get() == serial
        && state
            .borrow()
            .items
            .iter()
            .any(|current| Rc::ptr_eq(current, item))
}

fn menu_popover(item: &Rc<TrayItem>) -> gtk::Popover {
    if let Some(popover) = item.menu.borrow().as_ref() {
        return popover.clone();
    }

    let popover = gtk::Popover::new();

    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    popover.add_css_class("whelk-tray-menu");
    popover.set_parent(&item.image);
    *item.menu.borrow_mut() = Some(popover.clone());

    popover
}

fn menu_box(
    entries: &[TrayMenuEntry],
    state: &Rc<RefCell<TrayState>>,
    containing: &gtk::Popover,
) -> gtk::Box {
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 2);

    menu.add_css_class("whelk-menu");

    for entry in entries {
        append_menu_entry(&menu, entry, state, containing);
    }

    menu
}

fn append_menu_entry(
    menu: &gtk::Box,
    entry: &TrayMenuEntry,
    state: &Rc<RefCell<TrayState>>,
    containing: &gtk::Popover,
) {
    let TrayMenuEntry::Item {
        label,
        enabled,
        accel,
        action,
        submenu,
    } = entry
    else {
        menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        return;
    };

    if !submenu.is_empty() {
        let button = gtk::MenuButton::new();
        let popover = gtk::Popover::new();

        button.set_has_frame(false);
        button.set_label(label);
        button.set_direction(gtk::ArrowType::Right);
        button.add_css_class("flat");
        button.add_css_class("whelk-menu-item");
        button.set_halign(gtk::Align::Fill);

        popover.set_has_arrow(false);
        popover.add_css_class("whelk-tray-menu");
        popover.set_child(Some(&menu_box(submenu, state, &popover)));
        button.set_popover(Some(&popover));
        menu.append(&button);
        return;
    }

    let button = gtk::Button::new();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    let text = gtk::Label::new(Some(label));

    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_halign(gtk::Align::Start);
    row.append(&text);

    if let Some(accel) = accel {
        let shortcut = gtk::Label::new(Some(accel));

        shortcut.add_css_class("whelk-menu-accel");
        row.append(&shortcut);
    }

    button.set_child(Some(&row));
    button.add_css_class("flat");
    button.add_css_class("whelk-menu-item");
    button.set_halign(gtk::Align::Fill);
    button.set_has_frame(false);
    button.set_sensitive(*enabled);

    let weak = Rc::downgrade(state);
    let action = action.clone();
    let popover = containing.downgrade();

    button.connect_clicked(move |_| {
        if let Some(popover) = popover.upgrade() {
            popover.popdown();
        }

        if let Some(state) = weak.upgrade() {
            activate(&state, &action);
        }
    });

    menu.append(&button);
}

fn activate(state: &Rc<RefCell<TrayState>>, action: &TrayMenuAction) {
    let Some(bus) = state.borrow().bus.clone() else {
        return;
    };
    let parameters = (action.id, "clicked", 0i32.to_variant(), 0u32).to_variant();

    bus.call(
        Some(&action.service),
        &action.path,
        MENU_INTERFACE,
        "Event",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib::prelude::ToVariant;

    #[test]
    fn the_first_pixmap_is_decoded_and_invalid_ones_are_refused() {
        let pixmaps = vec![(1i32, 1i32, vec![0x11u8, 0x22, 0x33, 0x44])].to_variant();
        let pixmap = first_pixmap(&pixmaps).unwrap();

        assert_eq!((pixmap.width, pixmap.height), (1, 1));
        assert_eq!(pixmap.rgba, vec![0x22, 0x33, 0x44, 0x11]);

        assert_eq!(first_pixmap(&42i32.to_variant()), None);
        assert_eq!(
            first_pixmap(&Vec::<(i32, i32, Vec<u8>)>::new().to_variant()),
            None
        );
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The notification centre and its toast stack, ported from
//! `src/notification_center.c`.
//!
//! The centre groups history by application, newest first within a group, and
//! the corner shows at most [`TOAST_LIMIT`] transient notices.  Grouping and
//! queue rules are plain data so they are testable without a display; the GTK
//! half only builds the widgets those rules select.
//!
//! The service's posted/closed events are what distinguish a replacement from
//! a new notification and let a transient toast expire on its own timer.  The
//! centre keeps one event subscription for the shell's lifetime.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::{BarSettings, Edge as BarEdge};
use crate::format;
use crate::notifications::{
    Notification, NotificationEvent, NotificationEventSubscription, NotificationService,
};
use crate::sources;
use crate::style;

pub const PANEL_WIDTH: i32 = 380;
pub const TOAST_WIDTH: i32 = 380;
pub const TOAST_LIMIT: usize = 5;
const SURFACE_MARGIN: i32 = 8;
const PANEL_PADDING: i32 = 12;

thread_local! {
    /// One centre belongs to the shell, not to a bar.  Modules open it through
    /// the thread-local handle, as they do for the HUD.
    static CURRENT: RefCell<Option<NotificationCenter>> = const { RefCell::new(None) };
}

/// The application name a notification is grouped under.  The protocol allows
/// an empty name; the C centre gives those a readable heading.
pub fn group_app(notification: &Notification) -> &str {
    if notification.app_name.is_empty() {
        "Notifications"
    } else {
        &notification.app_name
    }
}

/// One application's column in the centre, with its newest notification first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub app: String,
    pub icon: String,
    pub notifications: Vec<Notification>,
}

/// Group an oldest-first snapshot by application.  The first appearance fixes
/// both the group's order and its icon, matching the C `rebuild_panel` walk.
pub fn group_notifications(snapshot: &[Notification]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();

    for notification in snapshot {
        let app = group_app(notification);

        if let Some(group) = groups.iter_mut().find(|group| group.app == app) {
            group.notifications.insert(0, notification.clone());
        } else {
            groups.push(Group {
                app: app.to_owned(),
                icon: notification.icon().to_owned(),
                notifications: vec![notification.clone()],
            });
        }
    }

    groups
}

pub fn panel_summary(total: usize) -> String {
    match total {
        0 => "Nothing new".to_owned(),
        1 => "1 notification".to_owned(),
        total => format!("{total} notifications"),
    }
}

/// The local `HH:MM` stamp shown on a panel card.  The store keeps
/// microseconds, like the C service.
pub fn time_label(timestamp_us: i64) -> String {
    gtk::glib::DateTime::from_unix_local(timestamp_us / 1_000_000)
        .ok()
        .and_then(|stamp| stamp.format("%H:%M").ok())
        .map(|stamp| stamp.to_string())
        .unwrap_or_default()
}

/// A body is markup only when Pango accepts it.  Only `b`, `i` and `u` keep
/// their meaning; every other tag is removed and its text escaped.  A
/// notification is D-Bus input from any session client, and Pango's `span`
/// attributes can spoof shell UI or exhaust the renderer with huge sizes.
pub fn body_markup(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }

    let mut output = String::new();
    let mut rest = body;
    let mut saw_tag = false;
    let mut open = Vec::new();

    while let Some(start) = rest.find('<') {
        output.push_str(&format::markup_escape(&rest[..start]));

        let end = rest[start..].find('>')?;
        let tag = rest[start + 1..start + end].to_ascii_lowercase();

        match tag.as_str() {
            "b" | "i" | "u" => {
                open.push(tag.chars().next().expect("one-character tag"));
                output.push('<');
                output.push_str(&tag);
                output.push('>');
            }
            "/b" | "/i" | "/u" => {
                let expected = tag.chars().nth(1).expect("closing tag name");

                if open.pop() != Some(expected) {
                    return None;
                }

                output.push('<');
                output.push_str(&tag);
                output.push('>');
            }
            _ => {}
        }

        saw_tag = true;
        rest = &rest[start + end + 1..];
    }

    if !open.is_empty() {
        return None;
    }

    output.push_str(&format::markup_escape(rest));

    if saw_tag {
        Some(output)
    } else {
        gtk::pango::parse_markup(body, '\0')
            .ok()
            .map(|_| body.to_owned())
    }
}

/// The newest-first ids the toast stack is showing.  Posting an id that is
/// already present moves it to the front; the oldest id falls off at the limit.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ToastQueue {
    ids: Vec<u32>,
}

impl ToastQueue {
    pub fn ids(&self) -> &[u32] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Add a notification and return the id that no longer fits, if any.
    pub fn posted(&mut self, id: u32) -> Option<u32> {
        self.ids.retain(|candidate| *candidate != id);
        self.ids.insert(0, id);

        if self.ids.len() > TOAST_LIMIT {
            self.ids.pop()
        } else {
            None
        }
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.ids.len();

        self.ids.retain(|candidate| *candidate != id);

        self.ids.len() != before
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }
}

/// The toast stack starts below a top bar and otherwise uses the screen margin.
pub fn toast_top_margin(bar: &BarSettings) -> i32 {
    if bar.edge == BarEdge::Top {
        bar.height + SURFACE_MARGIN
    } else {
        SURFACE_MARGIN
    }
}

/// The shell's notification centre and toast stack.
#[derive(Clone)]
pub struct NotificationCenter {
    inner: Rc<RefCell<State>>,
}

struct State {
    panel: gtk::Box,
    panel_window: gtk::ApplicationWindow,
    groups: gtk::Box,
    dnd: gtk::ToggleButton,
    clear: gtk::Button,
    close: gtk::Button,
    summary: gtk::Label,
    toast_stack: gtk::Box,
    toast_window: gtk::ApplicationWindow,
    service: NotificationService,
    toasts: ToastQueue,
    timers: HashMap<u32, gtk::glib::SourceId>,
    css: Option<gtk::CssProvider>,
    events: Option<NotificationEventSubscription>,
    updating_dnd: bool,
}

impl NotificationCenter {
    pub fn new(
        application: &gtk::Application,
        service: &NotificationService,
        style: &BarSettings,
    ) -> Self {
        let panel_window = surface_window(application, "notifications", "whelk notifications");
        let panel = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let heading = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let title = gtk::Label::new(Some("Notifications"));
        let summary = gtk::Label::new(None);
        let dnd = gtk::ToggleButton::new();
        let clear = panel_button("user-trash-symbolic", "Clear all");
        let close = panel_button("window-close-symbolic", "Close");
        let scroller = gtk::ScrolledWindow::new();
        let groups = gtk::Box::new(gtk::Orientation::Vertical, 14);

        panel.add_css_class("whelk-panel");
        panel.set_size_request(PANEL_WIDTH, -1);
        panel.set_margin_start(PANEL_PADDING);
        panel.set_margin_end(PANEL_PADDING);
        panel.set_margin_top(PANEL_PADDING);
        panel.set_margin_bottom(PANEL_PADDING);
        title.add_css_class("whelk-panel-title");
        title.set_xalign(0.0);
        summary.add_css_class("whelk-panel-subtitle");
        summary.set_xalign(0.0);
        heading.set_hexpand(true);
        heading.append(&title);
        heading.append(&summary);

        dnd.set_child(Some(&gtk::Image::from_icon_name(
            "notifications-disabled-symbolic",
        )));
        dnd.add_css_class("whelk-panel-button");
        dnd.add_css_class("whelk-dnd");
        dnd.set_tooltip_text(Some("Do not disturb"));

        header.append(&heading);
        header.append(&dnd);
        header.append(&clear);
        header.append(&close);

        groups.add_css_class("whelk-notification-groups");
        scroller.set_child(Some(&groups));
        scroller.set_vexpand(true);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

        panel.append(&header);
        panel.append(&scroller);
        panel_window.set_child(Some(&panel));

        let toast_window = surface_window(application, "toasts", "whelk toasts");
        let toast_stack = gtk::Box::new(gtk::Orientation::Vertical, 8);

        toast_stack.add_css_class("whelk-toast-stack");
        toast_stack.set_size_request(TOAST_WIDTH, -1);
        toast_window.set_child(Some(&toast_stack));

        let inner = Rc::new(RefCell::new(State {
            panel,
            panel_window,
            groups,
            dnd,
            clear,
            close,
            summary,
            toast_stack,
            toast_window,
            service: service.clone(),
            toasts: ToastQueue::default(),
            timers: HashMap::new(),
            css: None,
            events: None,
            updating_dnd: false,
        }));
        let center = Self { inner };

        center.install_style(style);
        center.configure_surfaces(style);
        center.connect_controls();
        center.sync();

        let weak = Rc::downgrade(&center.inner);
        let events = service.on_event(move |event| {
            let Some(inner) = weak.upgrade() else {
                return;
            };

            NotificationCenter { inner }.handle_event(event);
        });

        center.inner.borrow_mut().events = Some(events);
        center
    }

    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub fn install(center: Option<Self>) {
        CURRENT.with(|current| *current.borrow_mut() = center);
    }

    pub fn show_current() {
        if let Some(center) = Self::current() {
            center.show();
        }
    }

    pub fn hide_current() {
        if let Some(center) = Self::current() {
            center.hide();
        }
    }

    pub fn toggle_current() {
        if let Some(center) = Self::current() {
            center.toggle();
        }
    }

    pub fn is_visible_current() -> bool {
        Self::current().is_some_and(|center| center.is_visible())
    }

    pub fn show(&self) {
        self.clear_toasts();
        self.rebuild_panel();
        self.inner.borrow().panel_window.present();
    }

    pub fn hide(&self) {
        self.inner.borrow().panel_window.set_visible(false);
    }

    pub fn toggle(&self) {
        if self.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.inner.borrow().panel_window.is_visible()
    }

    pub fn set_style(&self, style: &BarSettings) {
        self.install_style(style);

        if crate::ui::layer_shell_available() {
            self.inner
                .borrow()
                .toast_window
                .set_margin(Edge::Top, toast_top_margin(style));
        }

        let state = self.inner.borrow();

        state.panel.queue_draw();
        state.toast_stack.queue_draw();
    }

    fn configure_surfaces(&self, style: &BarSettings) {
        let state = self.inner.borrow();
        let panel = &state.panel_window;
        let toast = &state.toast_window;

        panel.set_resizable(true);
        toast.set_resizable(true);

        if crate::ui::layer_shell_available() {
            panel.init_layer_shell();
            panel.set_namespace(Some("whelk-rust-notifications"));
            panel.set_layer(Layer::Overlay);
            panel.set_anchor(Edge::Top, true);
            panel.set_anchor(Edge::Bottom, true);
            panel.set_anchor(Edge::Right, true);
            panel.set_margin(Edge::Top, SURFACE_MARGIN);
            panel.set_margin(Edge::Right, SURFACE_MARGIN);
            panel.set_margin(Edge::Bottom, SURFACE_MARGIN);
            panel.set_margin(Edge::Left, SURFACE_MARGIN);
            panel.set_exclusive_zone(0);
            panel.set_keyboard_mode(KeyboardMode::None);

            toast.init_layer_shell();
            toast.set_namespace(Some("whelk-rust-toasts"));
            toast.set_layer(Layer::Overlay);
            toast.set_anchor(Edge::Top, true);
            toast.set_anchor(Edge::Right, true);
            toast.set_margin(Edge::Top, toast_top_margin(style));
            toast.set_margin(Edge::Right, SURFACE_MARGIN);
            toast.set_margin(Edge::Bottom, SURFACE_MARGIN);
            toast.set_margin(Edge::Left, SURFACE_MARGIN);
            toast.set_exclusive_zone(0);
            toast.set_keyboard_mode(KeyboardMode::None);
        } else {
            panel.set_default_size(PANEL_WIDTH + 24, 640);
            toast.set_default_size(TOAST_WIDTH + 24, 240);
        }

        if crate::ui::layer_shell_available() {
            let height = gtk::gdk::Display::default()
                .and_then(|display| {
                    display
                        .monitors()
                        .item(0)
                        .and_downcast::<gtk::gdk::Monitor>()
                })
                .map(|monitor| monitor.geometry().height())
                .unwrap_or(800);

            panel.set_default_size(
                PANEL_WIDTH + 2 * PANEL_PADDING,
                (height - 2 * SURFACE_MARGIN).max(240),
            );
        }

        panel.present();
        panel.set_visible(false);
        toast.present();
        toast.set_visible(false);
    }

    fn connect_controls(&self) {
        let state = self.inner.borrow();
        let service = state.service.clone();
        let weak = Rc::downgrade(&self.inner);

        state.dnd.connect_toggled(move |button| {
            let active = button.is_active();
            let Some(inner) = weak.upgrade() else {
                return;
            };

            if inner.borrow().updating_dnd {
                return;
            }

            let service = inner.borrow().service.clone();

            service.set_dnd(active);
        });

        let clear_service = service.clone();

        state
            .clear
            .connect_clicked(move |_| clear_service.close_all());

        let weak = Rc::downgrade(&self.inner);

        state.close.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                NotificationCenter { inner }.hide();
            }
        });
    }

    fn handle_event(&self, event: NotificationEvent) {
        match event {
            NotificationEvent::Changed => self.sync(),
            NotificationEvent::Posted(id) => self.posted(id),
            NotificationEvent::Closed(id) => self.closed(id),
        }
    }

    fn sync(&self) {
        self.rebuild_panel();
        self.rebuild_toasts();

        let dnd = self.inner.borrow().service.has_dnd();
        let button = self.inner.borrow().dnd.clone();

        self.inner.borrow_mut().updating_dnd = true;
        button.set_active(dnd);
        self.inner.borrow_mut().updating_dnd = false;
    }

    fn posted(&self, id: u32) {
        if self.is_visible() {
            self.rebuild_panel();

            return;
        }

        let Some(notification) = self.find(id) else {
            return;
        };
        let dropped = self.inner.borrow_mut().toasts.posted(id);

        if let Some(dropped) = dropped {
            self.cancel_timer(dropped);
        }

        self.rebuild_toasts();
        self.schedule_expiry(&notification);
    }

    fn closed(&self, id: u32) {
        self.cancel_timer(id);
        self.inner.borrow_mut().toasts.remove(id);
        self.rebuild_toasts();
    }

    fn find(&self, id: u32) -> Option<Notification> {
        self.inner
            .borrow()
            .service
            .snapshot()
            .into_iter()
            .find(|notification| notification.id == id)
    }

    fn rebuild_panel(&self) {
        let snapshot = self.inner.borrow().service.snapshot();
        let groups = group_notifications(&snapshot);
        let total = snapshot.len();
        let groups_box = self.inner.borrow().groups.clone();
        let summary = self.inner.borrow().summary.clone();

        while let Some(child) = groups_box.first_child() {
            groups_box.remove(&child);
        }

        if total == 0 {
            groups_box.append(&empty_state());
        } else {
            for group in &groups {
                groups_box.append(&self.build_group(group));
            }
        }

        summary.set_text(&panel_summary(total));
    }

    fn build_group(&self, group: &Group) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(None);

        if !group.icon.is_empty() {
            let mark = icon_image(&group.icon, 18);

            mark.add_css_class("whelk-group-icon");
            header.append(&mark);
        }

        title.set_markup(&format::markup_escape(&group.app));
        title.add_css_class("whelk-group-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);

        let count = gtk::Label::new(Some(&group.notifications.len().to_string()));

        count.add_css_class("whelk-group-count");
        header.append(&title);
        header.append(&count);

        let cards = gtk::Box::new(gtk::Orientation::Vertical, 6);

        for notification in &group.notifications {
            cards.append(&self.build_card(notification, false));
        }

        container.append(&header);
        container.append(&cards);
        container
    }

    fn build_card(&self, notification: &Notification, toast: bool) -> gtk::Box {
        let service = self.inner.borrow().service.clone();
        let id = notification.id;
        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let summary = gtk::Label::new(None);
        let close = panel_button("window-close-symbolic", "Dismiss");

        card.add_css_class("whelk-notification");
        card.add_css_class("whelk-card");

        if notification.urgency >= 2 {
            card.add_css_class("critical");
        }

        summary.set_markup(&format::markup_escape(&notification.summary));
        summary.add_css_class("whelk-notification-summary");
        summary.set_xalign(0.0);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        summary.set_hexpand(true);

        if !notification.icon().is_empty() {
            let image = icon_image(notification.icon(), if toast { 32 } else { 36 });

            image.set_valign(gtk::Align::Start);
            image.add_css_class("whelk-notification-icon");
            head.append(&image);
        }

        head.append(&summary);
        head.append(&close);
        card.append(&head);

        if !notification.body.is_empty() {
            let body = gtk::Label::new(None);

            set_body(&body, &notification.body);
            body.add_css_class("whelk-notification-body");
            body.set_xalign(0.0);
            body.set_wrap(true);
            body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            body.set_lines(if toast { 2 } else { 4 });
            body.set_ellipsize(gtk::pango::EllipsizeMode::End);
            card.append(&body);
        }

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        if !toast {
            let stamp = gtk::Label::new(Some(&time_label(notification.timestamp_us)));

            stamp.add_css_class("whelk-notification-time");
            stamp.set_xalign(0.0);
            stamp.set_hexpand(true);
            footer.append(&stamp);
        }

        for (index, action) in notification.actions.iter().enumerate() {
            if toast && index >= 2 {
                break;
            }

            if action.label.is_empty() {
                continue;
            }

            let button = gtk::Button::with_label(&action.label);
            let action_id = action.id.clone();
            let action_service = service.clone();

            button.add_css_class("whelk-notification-action");
            button.connect_clicked(move |_| {
                action_service.invoke(id, Some(&action_id));
            });
            footer.append(&button);
        }

        if footer.first_child().is_some() {
            card.append(&footer);
        }

        self.attach_card_click(&card, id, toast, &service);

        let close_service = service.clone();

        close.connect_clicked(move |_| {
            close_service.close(id);
        });

        card
    }

    fn attach_card_click(
        &self,
        card: &gtk::Box,
        id: u32,
        toast: bool,
        service: &NotificationService,
    ) {
        let gesture = gtk::GestureClick::new();
        let weak = Rc::downgrade(&self.inner);
        let service = service.clone();

        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_pressed(move |_, _, _, _| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let center = NotificationCenter { inner };
            let Some(notification) = center.find(id) else {
                return;
            };

            if toast {
                center.show();

                return;
            }

            if notification
                .actions
                .iter()
                .any(|action| action.id == "default")
            {
                service.invoke(id, Some("default"));
            } else {
                center.show();
            }
        });
        card.add_controller(gesture);
        card.set_cursor_from_name(Some("pointer"));
    }

    fn rebuild_toasts(&self) {
        let (queue, stack, window) = {
            let state = self.inner.borrow();

            (
                state.toasts.clone(),
                state.toast_stack.clone(),
                state.toast_window.clone(),
            )
        };
        let snapshot = self.inner.borrow().service.snapshot();
        let mut ids = queue.ids().to_vec();

        ids.retain(|id| snapshot.iter().any(|notification| notification.id == *id));

        while let Some(child) = stack.first_child() {
            stack.remove(&child);
        }

        for id in &ids {
            if let Some(notification) = snapshot.iter().find(|notification| notification.id == *id)
            {
                stack.append(&self.build_card(notification, true));
            }
        }

        window.set_visible(!ids.is_empty());
    }

    fn clear_toasts(&self) {
        let ids = self.inner.borrow().toasts.ids().to_vec();

        for id in ids {
            self.cancel_timer(id);
        }

        self.inner.borrow_mut().toasts.clear();
        self.rebuild_toasts();
    }

    fn schedule_expiry(&self, notification: &Notification) {
        let id = notification.id;
        let timeout = notification.expire_timeout;

        self.cancel_timer(id);

        if timeout <= 0 {
            return;
        }

        let weak = Rc::downgrade(&self.inner);
        let source =
            gtk::glib::timeout_add_local_once(Duration::from_millis(timeout as u64), move || {
                if let Some(inner) = weak.upgrade() {
                    NotificationCenter { inner }.expire_toast(id);
                }
            });

        self.inner.borrow_mut().timers.insert(id, source);
    }

    fn expire_toast(&self, id: u32) {
        self.inner.borrow_mut().timers.remove(&id);
        self.inner.borrow_mut().toasts.remove(id);

        let transient = self
            .find(id)
            .is_some_and(|notification| notification.transient);

        if transient {
            let service = self.inner.borrow().service.clone();

            service.expire(id);
        } else {
            self.rebuild_toasts();
        }
    }

    fn cancel_timer(&self, id: u32) {
        let source = self.inner.borrow_mut().timers.remove(&id);

        if let Some(source) = source {
            sources::remove(source);
        }
    }

    fn install_style(&self, bar: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let panel_css = style::css(
            &style::widget_id("notifications"),
            bar,
            "whelk-panel",
            bar.css.as_deref(),
        );
        let toast_css = style::css(&style::widget_id("toasts"), bar, "whelk-toast-stack", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{BASE_CSS}\n{panel_css}\n{toast_css}"));

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
        let timers = std::mem::take(&mut self.timers);

        for (_, source) in timers {
            sources::remove(source);
        }

        if let Some(css) = self.css.take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.panel_window.destroy();
        self.toast_window.destroy();
    }
}

fn surface_window(
    application: &gtk::Application,
    name: &str,
    title: &str,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title(title)
        .build();

    window.set_widget_name(&style::widget_id(name));
    window.add_css_class("whelk-surface");
    window.set_decorated(false);

    window
}

fn panel_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);

    button.add_css_class("whelk-panel-button");
    button.set_tooltip_text(Some(tooltip));

    button
}

fn icon_image(icon: &str, pixel_size: i32) -> gtk::Image {
    let image = if Path::new(icon).is_absolute() {
        gtk::Image::from_file(icon)
    } else {
        gtk::Image::from_icon_name(icon)
    };

    image.set_pixel_size(pixel_size);
    image
}

fn set_body(label: &gtk::Label, body: &str) {
    if let Some(markup) = body_markup(body) {
        label.set_markup(&markup);
    } else {
        label.set_text(body);
    }
}

fn empty_state() -> gtk::Box {
    let empty = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let mark = gtk::Image::from_icon_name("notifications-symbolic");
    let title = gtk::Label::new(Some("You are all caught up."));
    let hint = gtk::Label::new(Some("Notifications show up here, newest first."));

    empty.add_css_class("whelk-panel-empty");
    mark.set_pixel_size(44);
    mark.add_css_class("whelk-panel-empty-icon");
    title.add_css_class("whelk-panel-empty-title");
    hint.add_css_class("whelk-panel-empty-hint");
    mark.set_halign(gtk::Align::Center);
    title.set_halign(gtk::Align::Center);
    hint.set_halign(gtk::Align::Center);
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    empty.append(&mark);
    empty.append(&title);
    empty.append(&hint);

    empty
}

const BASE_CSS: &str = "
    .whelk-surface {
        background-color: transparent;
    }
    .whelk-panel {
        border-radius: 14px;
    }
    .whelk-panel-title {
        font-weight: 700;
        font-size: 1.05em;
    }
    .whelk-panel-button {
        padding: 4px;
        min-width: 0;
        min-height: 0;
        border-radius: 8px;
        background: transparent;
    }
    .whelk-panel-button:hover {
        background: alpha(currentColor, 0.12);
    }
    .whelk-panel-button:checked {
        background: alpha(currentColor, 0.18);
    }
    .whelk-panel-subtitle {
        opacity: 0.55;
        font-size: 0.85em;
    }
    .whelk-panel-empty {
        padding: 30px 8px;
    }
    .whelk-panel-empty-icon {
        opacity: 0.3;
    }
    .whelk-panel-empty-title {
        font-weight: 600;
    }
    .whelk-panel-empty-hint {
        opacity: 0.5;
        font-size: 0.88em;
    }
    .whelk-group-title {
        font-weight: 600;
        font-size: 0.95em;
    }
    .whelk-group-icon {
        opacity: 0.9;
    }
    .whelk-group-count {
        opacity: 0.7;
        font-size: 0.78em;
        min-width: 0;
        padding: 0 7px;
        border-radius: 999px;
        background: alpha(currentColor, 0.1);
    }
    .whelk-card {
        padding: 12px;
        border-radius: 12px;
        background: alpha(currentColor, 0.06);
    }
    .whelk-notification:hover {
        background: alpha(currentColor, 0.09);
    }
    .whelk-notification.critical {
        border-left: 3px solid #e01b24;
    }
    .whelk-notification-icon {
        border-radius: 6px;
    }
    .whelk-notification-summary {
        font-weight: 600;
    }
    .whelk-notification-body {
        opacity: 0.85;
        font-size: 0.95em;
    }
    .whelk-notification-time {
        opacity: 0.5;
        font-size: 0.85em;
    }
    .whelk-notification-action {
        padding: 3px 9px;
        border-radius: 7px;
        background: alpha(currentColor, 0.08);
    }
    .whelk-notification-action:hover {
        background: alpha(currentColor, 0.16);
    }
    .whelk-toast-stack .whelk-card {
        background: alpha(currentColor, 0.12);
        box-shadow: 0 8px 24px alpha(black, 0.28);
    }
";

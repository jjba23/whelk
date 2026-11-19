// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The shared lifecycle of a bar module.
//!
//! A module is a small reading with a widget, an interval and, often, a
//! command.  This host owns the parts every kind shares: the root box and its
//! CSS classes, the refresh timer, one in-flight command with a deadline, the
//! generation token that makes a late answer harmless, and the click/scroll
//! options from the configuration.  A factory only supplies the reading and
//! what to paint when it arrives.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::command::{self, CommandHandle};
use crate::format;
use crate::sources;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 1;
pub const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 5;
pub const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 60 * 60;

/// The mouse vocabulary every module shares, read from the same option names as
/// the C `whelk_click_attach`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleActions {
    pub primary: Option<String>,
    pub middle: Option<String>,
    pub secondary: Option<String>,
    pub scroll_up: Option<String>,
    pub scroll_down: Option<String>,
}

impl ModuleActions {
    fn from_module(module: &ModuleSpec) -> Self {
        fn command(module: &ModuleSpec, key: &str) -> Option<String> {
            module.text(key).filter(|value| !value.is_empty())
        }

        Self {
            primary: command(module, "on-click"),
            middle: command(module, "on-click-middle"),
            secondary: command(module, "on-click-right"),
            scroll_up: command(module, "on-scroll-up"),
            scroll_down: command(module, "on-scroll-down"),
        }
    }

    pub fn has_clicks(&self) -> bool {
        self.primary.is_some() || self.middle.is_some() || self.secondary.is_some()
    }

    pub fn has_scroll(&self) -> bool {
        self.scroll_up.is_some() || self.scroll_down.is_some()
    }
}

/// The common options a factory does not have to re-read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSettings {
    pub interval_seconds: u32,
    /// Zero disables the deadline; the default is five seconds.
    pub command_timeout: Duration,
    pub css_class: Option<String>,
    pub format: Option<String>,
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    pub actions: ModuleActions,
}

impl ModuleSettings {
    pub fn from_module(module: &ModuleSpec, fallback_interval: u32) -> Self {
        let interval = module
            .integer("interval")
            .unwrap_or(i64::from(fallback_interval));
        let interval_seconds = if interval > 0 {
            interval.min(i64::from(u32::MAX)) as u32
        } else {
            0
        };
        let timeout = module
            .integer("command-timeout")
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECONDS as i64);
        let command_timeout = if timeout <= 0 {
            Duration::ZERO
        } else {
            Duration::from_secs(timeout.min(MAX_COMMAND_TIMEOUT_SECONDS as i64) as u64)
        };

        Self {
            interval_seconds,
            command_timeout,
            css_class: module.text("css-class").filter(|value| !value.is_empty()),
            format: module.text("format").filter(|value| !value.is_empty()),
            icon: module.text("icon").filter(|value| !value.is_empty()),
            tooltip: module.text("tooltip").filter(|value| !value.is_empty()),
            actions: ModuleActions::from_module(module),
        }
    }

    /// `None` means refresh once and start no timer, the C `interval <= 0` rule.
    pub fn refresh_interval(&self) -> Option<u32> {
        (self.interval_seconds > 0).then_some(self.interval_seconds)
    }
}

/// The request generation.  Starting a request returns a token; cancelling or
/// starting another request makes every older token stale.  This is deliberately
/// a small value type so a late command answer can be rejected without touching
/// GTK or a command handle.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ModuleState {
    next: u64,
    current: Option<u64>,
}

impl ModuleState {
    pub fn begin(&mut self) -> Option<u64> {
        if self.current.is_some() {
            return None;
        }

        self.next = self.next.wrapping_add(1);
        self.current = Some(self.next);

        self.current
    }

    pub fn finish(&mut self, token: u64) -> bool {
        if self.current != Some(token) {
            return false;
        }

        self.current = None;

        true
    }

    pub fn cancel(&mut self) {
        self.current = None;
    }

    pub fn is_in_flight(&self) -> bool {
        self.current.is_some()
    }
}

type UpdateCallback = dyn Fn(&ModuleHost) + 'static;

struct HostInner {
    root: gtk::glib::WeakRef<gtk::Box>,
    /// The configured kind, so the accessible name can say what this is.
    kind: String,
    settings: ModuleSettings,
    icon_font: Option<String>,
    label: RefCell<Option<gtk::Label>>,
    text: RefCell<String>,
    state: RefCell<ModuleState>,
    pending: RefCell<Option<CommandHandle>>,
    timer: RefCell<Option<gtk::glib::SourceId>>,
    update: RefCell<Option<Rc<UpdateCallback>>>,
    alive: Cell<bool>,
}

impl HostInner {
    fn stop(&self) {
        if !self.alive.replace(false) {
            return;
        }

        if let Some(source) = self.timer.borrow_mut().take() {
            sources::remove(source);
        }

        self.update.borrow_mut().take();
        self.state.borrow_mut().cancel();

        if let Some(handle) = self.pending.borrow_mut().take() {
            handle.cancel();
        }
    }
}

impl Drop for HostInner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A live module.  Clone it into update closures; the root widget keeps the
/// last reference alive until GTK destroys it.
#[derive(Clone)]
pub struct ModuleHost {
    inner: Rc<HostInner>,
}

/// A non-owning module handle for callbacks owned by a service that outlives
/// the bar.  Upgrading after the root is destroyed returns `None`.
#[derive(Clone)]
pub struct WeakModuleHost {
    inner: Weak<HostInner>,
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        self.inner.upgrade().map(|inner| ModuleHost { inner })
    }
}

impl ModuleHost {
    /// Build the common root and install the shared controllers.  The caller
    /// must keep the returned host and then call [`start`](Self::start).
    pub fn new(
        module: &ModuleSpec,
        fallback_interval: u32,
        bar_spacing: i32,
        icon_font: Option<&str>,
    ) -> (gtk::Box, Self) {
        let settings = ModuleSettings::from_module(module, fallback_interval);
        // The icon-to-label gap is the bar's own metric, not a second one: the
        // stylesheet insets every module by half its spacing, and this keeps
        // the inside of a reading on the same scale as the space outside it.
        let spacing = crate::style::module_inner_spacing(bar_spacing.max(0));
        let root = gtk::Box::new(gtk::Orientation::Horizontal, spacing);

        root.add_css_class("whelk-module");
        root.add_css_class(&format!("whelk-{}", module.kind));

        if let Some(css_class) = settings.css_class.as_deref() {
            root.add_css_class(css_class);
        }

        attach_actions(&root, &settings.actions);
        // A screen reader gets the reading's role and name from the first
        // paint on; `present`/`set_text` keep the name current.
        crate::a11y::describe(
            &root,
            &module.kind,
            "",
            settings.tooltip.as_deref(),
            settings.actions.has_clicks(),
        );

        let host = Self {
            inner: Rc::new(HostInner {
                root: root.downgrade(),
                kind: module.kind.clone(),
                settings,
                icon_font: icon_font.map(str::to_owned),
                label: RefCell::new(None),
                text: RefCell::new(String::new()),
                state: RefCell::new(ModuleState::default()),
                pending: RefCell::new(None),
                timer: RefCell::new(None),
                update: RefCell::new(None),
                alive: Cell::new(true),
            }),
        };
        let destroy_host = host.clone();

        // The signal handler owns the host, while the host only holds a weak
        // reference to the widget.  Destroying the widget therefore stops the
        // timer and cancels an in-flight command without a GTK/Rust cycle.
        root.connect_destroy(move |_| destroy_host.shutdown());

        if host.inner.settings.actions.primary.is_some() {
            register_activatable(&host);
        }

        (root, host)
    }

    /// Install the reading and start its first refresh.  A positive interval
    /// adds the repeat timer; a zero interval leaves a one-shot module.
    pub fn start<F>(&self, update: F)
    where
        F: Fn(&ModuleHost) + 'static,
    {
        *self.inner.update.borrow_mut() = Some(Rc::new(update));
        self.refresh();

        if !self.is_alive() {
            return;
        }

        let Some(seconds) = self.inner.settings.refresh_interval() else {
            return;
        };

        if let Some(previous) = self.inner.timer.borrow_mut().take() {
            sources::remove(previous);
        }

        let host = self.clone();
        let source = gtk::glib::timeout_add_seconds_local(seconds, move || {
            if !host.is_alive() {
                return gtk::glib::ControlFlow::Break;
            }

            host.refresh();

            gtk::glib::ControlFlow::Continue
        });

        *self.inner.timer.borrow_mut() = Some(source);
    }

    /// Run the module's reading now.  The interval timer calls the same path.
    pub fn refresh(&self) {
        if !self.is_alive() {
            return;
        }

        let update = self.inner.update.borrow().clone();

        if let Some(update) = update {
            update(self);
        }
    }

    /// Run one command for this module.  If another command is already in
    /// flight the tick is skipped, exactly as the C module's `busy` guard does.
    /// The callback is only called for the current request; a timeout, a
    /// cancellation, or destruction of the root makes it stale and silent.
    pub fn run_command<F>(&self, command_line: &str, callback: F)
    where
        F: FnOnce(Option<String>) + 'static,
    {
        if !self.is_alive() || command_line.is_empty() {
            return;
        }

        let Some(token) = self.inner.state.borrow_mut().begin() else {
            return;
        };
        let weak = Rc::downgrade(&self.inner);
        let timeout = self.inner.settings.command_timeout;
        let handle = command::run_with_timeout(command_line, timeout, move |answer| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let host = ModuleHost { inner };

            if !host.is_alive() || !host.inner.state.borrow_mut().finish(token) {
                return;
            }

            // The callback is running, so the handle is complete.  Dropping it
            // here releases the command's resources without cancelling again.
            let pending = host.inner.pending.borrow_mut().take();

            drop(pending);
            callback(answer);
        });

        if let Some(handle) = handle {
            if self.is_alive() {
                *self.inner.pending.borrow_mut() = Some(handle);
            } else {
                handle.cancel();
            }
        }
    }

    /// Invalidate the current request and terminate its child, without stopping
    /// the timer.  A later refresh may start a fresh request.
    pub fn cancel(&self) {
        self.inner.state.borrow_mut().cancel();

        if let Some(handle) = self.inner.pending.borrow_mut().take() {
            handle.cancel();
        }
    }

    /// Stop refreshing and cancel everything owned by this module.
    pub fn shutdown(&self) {
        self.inner.stop();
    }

    pub fn is_alive(&self) -> bool {
        self.inner.alive.get()
    }

    pub fn downgrade(&self) -> WeakModuleHost {
        WeakModuleHost {
            inner: Rc::downgrade(&self.inner),
        }
    }

    pub fn root(&self) -> Option<gtk::Box> {
        self.inner.root.upgrade()
    }

    pub fn settings(&self) -> &ModuleSettings {
        &self.inner.settings
    }

    /// Add the one label most readings use.  Calling this twice returns the
    /// same label, so a factory cannot accidentally stack two readings.
    pub fn add_label(&self) -> Option<gtk::Label> {
        if let Some(label) = self.label() {
            return Some(label);
        }

        let root = self.root()?;
        let label = gtk::Label::new(None);

        label.add_css_class("whelk-label");
        label.set_valign(gtk::Align::Center);
        root.append(&label);
        *self.inner.label.borrow_mut() = Some(label.clone());

        Some(label)
    }

    pub fn label(&self) -> Option<gtk::Label> {
        self.inner.label.borrow().clone()
    }

    /// Render the configured `format` into the module's label and its configured
    /// `tooltip` into the hover text.  `{icon}` comes from the spec and the
    /// bar's icon font; `values` are the reading's named pieces.
    pub fn present(&self, default_format: &str, values: &[(&str, &str)]) {
        let format = self
            .inner
            .settings
            .format
            .as_deref()
            .unwrap_or(default_format);
        let icon = self.inner.settings.icon.as_deref().unwrap_or("");

        self.present_with(format, icon, self.inner.settings.tooltip.as_deref(), values);
    }

    /// [`present`](Self::present) with a reading-dependent format or icon, as
    /// the muted, charging or disconnected states need.
    pub fn present_with(
        &self,
        format: &str,
        icon: &str,
        tooltip: Option<&str>,
        values: &[(&str, &str)],
    ) {
        let icon_markup = format::icon_markup(icon, self.inner.icon_font.as_deref());
        let plain = render_values(format, icon, &icon_markup, false, values);
        let markup = render_values(format, icon, &icon_markup, true, values);

        *self.inner.text.borrow_mut() = plain.clone();

        if let Some(label) = self.label() {
            label.set_markup(&markup);
        }

        if let Some(root) = self.root() {
            let tooltip = tooltip.or(self.inner.settings.tooltip.as_deref());

            self.describe(&root, &plain, tooltip);
        }

        if let Some(tooltip_format) = tooltip.or(self.inner.settings.tooltip.as_deref()) {
            let mut tooltip_values = values.to_vec();

            tooltip_values.push(("text", plain.as_str()));

            let tooltip_markup = render_values(tooltip_format, "", "", true, &tooltip_values);

            if let Some(root) = self.root() {
                root.set_tooltip_markup(Some(&tooltip_markup));
            }
        }
    }

    /// Set a plain reading, used when a reading has no format values (or when
    /// it is empty).
    pub fn set_text(&self, text: &str) {
        *self.inner.text.borrow_mut() = text.to_owned();

        if let Some(label) = self.label() {
            label.set_markup(&format::markup_escape(text));
        }

        self.refresh_tooltip();

        if let Some(root) = self.root() {
            let tooltip = self.inner.settings.tooltip.as_deref();

            self.describe(&root, text, tooltip);
        }
    }

    /// Keep the root's accessible name in step with the reading it draws.
    fn describe(&self, root: &gtk::Box, text: &str, tooltip: Option<&str>) {
        crate::a11y::describe(
            root,
            &self.inner.kind,
            text,
            tooltip,
            self.inner.settings.actions.has_clicks(),
        );
    }

    /// Run the reading's own click command, for the keyboard path.  False means
    /// this module has no command to run -- a switch the module owns itself
    /// (do-not-disturb, the wallpaper card) is reached with the pointer.
    pub fn activate(&self) -> bool {
        let Some(command_line) = self.inner.settings.actions.primary.as_deref() else {
            return false;
        };

        if command_line.is_empty() {
            return false;
        }

        command::spawn(command_line);

        true
    }

    fn refresh_tooltip(&self) {
        let Some(tooltip_format) = self.inner.settings.tooltip.as_deref() else {
            return;
        };
        let text = self.inner.text.borrow().clone();
        let values = [("text", text.as_str())];
        let tooltip_markup = render_values(tooltip_format, "", "", true, &values);

        if let Some(root) = self.root() {
            root.set_tooltip_markup(Some(&tooltip_markup));
        }
    }

    pub fn add_class(&self, css_class: &str) {
        if let Some(root) = self.root() {
            root.add_css_class(css_class);
        }
    }

    pub fn remove_class(&self, css_class: &str) {
        if let Some(root) = self.root() {
            root.remove_css_class(css_class);
        }
    }

    pub fn set_visible(&self, visible: bool) {
        if let Some(root) = self.root() {
            root.set_visible(visible);
        }
    }
}

fn render_values(
    format: &str,
    icon: &str,
    icon_markup: &str,
    escape_values: bool,
    values: &[(&str, &str)],
) -> String {
    let mut text = format.to_owned();

    for (key, value) in values {
        let value = if escape_values {
            format::markup_escape(value)
        } else {
            (*value).to_owned()
        };

        text = format::subst(&text, key, &value);
    }

    let icon = if escape_values { icon_markup } else { icon };

    format::subst(&text, "icon", icon)
}

fn attach_actions(root: &gtk::Box, actions: &ModuleActions) {
    attach_click(root, gtk::gdk::BUTTON_PRIMARY, actions.primary.as_deref());
    attach_click(root, gtk::gdk::BUTTON_MIDDLE, actions.middle.as_deref());
    attach_click(
        root,
        gtk::gdk::BUTTON_SECONDARY,
        actions.secondary.as_deref(),
    );

    if actions.has_clicks() {
        root.set_cursor_from_name(Some("pointer"));
    }

    if !actions.has_scroll() {
        return;
    }

    let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let up = actions.scroll_up.clone();
    let down = actions.scroll_down.clone();

    controller.connect_scroll(move |_, _, dy| {
        if dy < 0.0
            && let Some(command_line) = up.as_deref()
        {
            command::spawn(command_line);

            return gtk::glib::Propagation::Stop;
        }

        if dy > 0.0
            && let Some(command_line) = down.as_deref()
        {
            command::spawn(command_line);

            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });
    root.add_controller(controller);
}

fn attach_click(root: &gtk::Box, button: u32, command_line: Option<&str>) {
    let Some(command_line) = command_line else {
        return;
    };
    let gesture = gtk::GestureClick::new();
    let command_line = command_line.to_owned();

    gesture.set_button(button);
    gesture.connect_pressed(move |_, _, _, _| command::spawn(&command_line));
    root.add_controller(gesture);
}

/// How long a chosen menu row waits before it runs.
///
/// A row pops its popover down first, and the compositor then hands the
/// keyboard back to the window the menu describes.  A chord typed by the
/// shortcut tool, or a compositor verb such as Close Window, acts on whichever
/// window holds the keyboard: run in the same turn as the click, it reaches the
/// bar instead of the application.
pub const ACTION_GRACE: Duration = Duration::from_millis(120);

thread_local! {
    /// The popovers a bar item can open, held weakly so a destroyed bar leaves
    /// nothing behind.
    ///
    /// A layer surface can have one grabbing popup at a time: a second one
    /// mapped while the first is still up is refused by GDK ("Tried to map a
    /// grabbing popup with a non-top most parent") and the click that asked
    /// for it does nothing at all.  Bar items therefore close the others
    /// before they open their own.
    static BAR_POPOVERS: RefCell<Vec<gtk::glib::WeakRef<gtk::Popover>>> =
        const { RefCell::new(Vec::new()) };
}

/// Register a bar-level popover so another item can close it first.
///
/// Only popovers opened from the bar itself belong here: a submenu inside an
/// open menu is nested in it, and closing the parent would take the submenu
/// with it.
pub fn watch_popover(popover: &gtk::Popover) {
    BAR_POPOVERS.with(|popovers| {
        let mut popovers = popovers.borrow_mut();

        popovers.retain(|held| held.upgrade().is_some());

        if !popovers
            .iter()
            .any(|held| held.upgrade().as_ref() == Some(popover))
        {
            popovers.push(popover.downgrade());
        }
    });
}

/// Pop down every registered bar popover except `keep`.
///
/// The list is copied out before anything is popped down, so a closed signal
/// that walks the same registry cannot re-enter the borrow.
pub fn close_popovers(keep: Option<&gtk::Popover>) {
    let open: Vec<gtk::Popover> = BAR_POPOVERS.with(|popovers| {
        let mut popovers = popovers.borrow_mut();

        popovers.retain(|held| held.upgrade().is_some());
        popovers.iter().filter_map(|held| held.upgrade()).collect()
    });

    for popover in open {
        if Some(&popover) != keep && popover.is_visible() {
            popover.popdown();
        }
    }
}

thread_local! {
    /// The module hosts whose root is on the bar, held weakly: the keyboard
    /// path asks "what does Return do on this widget?", and a rebuilt bar must
    /// not answer with the previous one's commands.
    static ACTIVATABLE: RefCell<Vec<ModuleHost>> = const { RefCell::new(Vec::new()) };
}

/// Make HOST reachable from the bar's keyboard path.
pub fn register_activatable(host: &ModuleHost) {
    ACTIVATABLE.with(|hosts| {
        let mut hosts = hosts.borrow_mut();

        hosts.retain(|host| host.is_alive());
        hosts.push(host.clone());
    });
}

/// Run the click command of the module that owns WIDGET, if it has one.
pub fn activate(widget: &gtk::Widget) -> bool {
    let mut owner = Some(widget.clone());

    while let Some(candidate) = owner {
        let found = ACTIVATABLE.with(|hosts| {
            hosts
                .borrow()
                .iter()
                .find(|host| {
                    host.root()
                        .is_some_and(|root| root.upcast_ref::<gtk::Widget>() == &candidate)
                })
                .cloned()
        });

        if let Some(host) = found {
            return host.activate();
        }

        owner = candidate.parent();
    }

    false
}

/// True while any bar popover is on screen.
///
/// An auto-hiding bar asks this before it disappears: a menu the reader is in
/// the middle of was opened from the bar, so the pointer having left the bar is
/// not the same as the reader having left the menu.
pub fn any_popover_open() -> bool {
    BAR_POPOVERS.with(|popovers| {
        let mut popovers = popovers.borrow_mut();

        popovers.retain(|held| held.upgrade().is_some());
        popovers
            .iter()
            .filter_map(|held| held.upgrade())
            .any(|popover| popover.is_visible())
    })
}

/// Make `button` the only bar item with a popover open.
///
/// `GtkMenuButton` toggles its popover from the class handler of "clicked",
/// which runs before a handler connected here could; a capture-phase gesture
/// on the press is early enough to get the previous popover out of the way, and
/// it denies the sequence so the button still handles the click itself.
pub fn attach_exclusive_popover(button: &impl IsA<gtk::Widget>, popover: &gtk::Popover) {
    watch_popover(popover);

    let gesture = gtk::GestureClick::new();
    let popover = popover.clone();

    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    gesture.connect_pressed(move |gesture, _, _, _| {
        close_popovers(Some(&popover));
        gesture.set_state(gtk::EventSequenceState::Denied);
    });
    button.add_controller(gesture);
}

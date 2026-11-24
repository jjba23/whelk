// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The desktop's context menu.
//!
//! A right click on the empty desktop used to offer the wallpapers and nothing
//! else.  This menu is the shell's answer to "what can I do from here": open a
//! launcher mode, the clipboard, the notification centre or the settings; move
//! the wallpaper; flip the shell's own switches; lock, sleep, restart, shut
//! down or leave; reload the configuration; hide the cards; quit.
//!
//! The rows are a value before they are a widget.  [`sections`] is a pure
//! function of [`DesktopMenuState`], so what the menu offers, what each row
//! says and which command each row carries are all testable without a display.
//! The GTK half only draws the sections and hands a [`DesktopCommand`] back to
//! the shell.
//!
//! The menu is its own layer-shell surface, cut to the card, for the same
//! reason the wallpaper menu is: a compositor that blurs a surface blurs its
//! geometry, and a fullscreen overlay would blur the whole output.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BarSettings;
use crate::power;
use crate::style;
use crate::wallpapermenu;

const MENU_CSS: &str = "
.whelk-desktop-menu {
  padding: 8px 10px 6px 10px;
  border-radius: 14px;
}
.whelk-desktop-menu-header {
  margin-bottom: 2px;
}
.whelk-desktop-menu-title {
  font-weight: 800;
  letter-spacing: 0.2px;
}
.whelk-desktop-menu-close {
  padding: 0 4px;
  min-height: 20px;
  min-width: 20px;
}
.whelk-desktop-menu-section {
  margin-top: 7px;
  opacity: 0.55;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.7px;
}
.whelk-desktop-menu-note {
  opacity: 0.45;
  font-size: 10px;
  margin-bottom: 1px;
}
.whelk-desktop-menu-row {
  padding: 4px 8px;
  min-height: 28px;
  border-radius: 8px;
}
.whelk-desktop-menu-row:hover {
  background-color: alpha(currentColor, 0.08);
}
.whelk-desktop-menu-label {
  font-size: 13px;
}
.whelk-desktop-menu-detail {
  opacity: 0.55;
  font-size: 11px;
  font-feature-settings: \"tnum\";
}
.whelk-desktop-menu-hint {
  opacity: 0.45;
  font-size: 10px;
  margin-top: 4px;
}
";

/// One thing a desktop-menu row asks the shell to do.
///
/// This is the whole contract between the menu and the shell: the menu never
/// reaches into a bar, a launcher or a lock screen itself, and a new row is a
/// variant, a label and an arm in the shell's handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopCommand {
    /// Open the launcher in a named mode (`app`, `run`, `file`, `calc`, …).
    Launcher(&'static str),
    Clipboard,
    Notifications,
    Settings,
    About,
    SessionMenu,
    Lock,
    /// A session verb whose command chain already knows how this machine
    /// powers off (see [`crate::power`]).
    Power(power::Action),
    NightLight,
    Dnd,
    Inhibit,
    Reload,
    Quit,
    WallpaperNext,
    WallpaperPrevious,
    WallpaperShuffle,
    WallpaperPicker,
    /// The thumbnail collection, which is the wallpaper menu's own card.
    WallpaperCollection,
    ToggleWidgets,
}

/// The values the menu reads when it opens.  The shell hands over what is true
/// now, so a switch flipped from the bar or the command line is reflected the
/// next time the desktop is right-clicked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopMenuState {
    pub night_light: bool,
    pub dnd: bool,
    pub inhibited: bool,
    pub widgets_visible: bool,
    pub widget_count: usize,
    /// The current picture's file name, when the engine has one.
    pub wallpaper: Option<String>,
}

/// One clickable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRow {
    /// Stable identity, also the CSS object name a test can look for.
    pub id: &'static str,
    pub label: String,
    /// What the row says on its right: the state of a switch, or the current
    /// picture.  Empty when the row needs no second half.
    pub detail: String,
    pub command: DesktopCommand,
}

/// A titled group of rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSection {
    pub title: &'static str,
    /// A quiet line under the title, such as the current wallpaper's name.
    pub note: String,
    pub rows: Vec<MenuRow>,
}

fn row(
    id: &'static str,
    label: &str,
    detail: impl Into<String>,
    command: DesktopCommand,
) -> MenuRow {
    MenuRow {
        id,
        label: label.to_owned(),
        detail: detail.into(),
        command,
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

/// Build the menu the state asks for.  The order is the order a person looks
/// for things in: what opens, the wallpaper, the switches, the way out, the
/// shell itself.
pub fn sections(state: &DesktopMenuState) -> Vec<MenuSection> {
    let wallpaper_note = state
        .wallpaper
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or("no picture selected")
        .to_owned();
    let widgets_detail = if state.widgets_visible {
        format!("{} shown", state.widget_count)
    } else {
        format!("{} hidden", state.widget_count)
    };

    vec![
        MenuSection {
            title: "Open",
            note: String::new(),
            rows: vec![
                row(
                    "applications",
                    "Applications…",
                    "",
                    DesktopCommand::Launcher("app"),
                ),
                row("run", "Run a Command…", "", DesktopCommand::Launcher("run")),
                row(
                    "files",
                    "Search Files…",
                    "",
                    DesktopCommand::Launcher("file"),
                ),
                row(
                    "calculator",
                    "Calculator…",
                    "",
                    DesktopCommand::Launcher("calc"),
                ),
                row(
                    "clipboard",
                    "Clipboard History",
                    "",
                    DesktopCommand::Clipboard,
                ),
                row(
                    "notifications",
                    "Notification Centre",
                    "",
                    DesktopCommand::Notifications,
                ),
                row("settings", "Settings…", "", DesktopCommand::Settings),
            ],
        },
        MenuSection {
            title: "Wallpaper",
            note: wallpaper_note,
            rows: vec![
                row(
                    "wallpaper-next",
                    "Next Wallpaper",
                    "",
                    DesktopCommand::WallpaperNext,
                ),
                row(
                    "wallpaper-previous",
                    "Previous Wallpaper",
                    "",
                    DesktopCommand::WallpaperPrevious,
                ),
                row(
                    "wallpaper-shuffle",
                    "Shuffle Now",
                    "",
                    DesktopCommand::WallpaperShuffle,
                ),
                row(
                    "wallpaper-collection",
                    "Choose from Collection…",
                    "",
                    DesktopCommand::WallpaperCollection,
                ),
                row(
                    "wallpaper-picker",
                    "Open Picker…",
                    "",
                    DesktopCommand::WallpaperPicker,
                ),
            ],
        },
        MenuSection {
            title: "Switches",
            note: String::new(),
            rows: vec![
                row(
                    "night-light",
                    "Night Light",
                    on_off(state.night_light),
                    DesktopCommand::NightLight,
                ),
                row(
                    "dnd",
                    "Do Not Disturb",
                    on_off(state.dnd),
                    DesktopCommand::Dnd,
                ),
                row(
                    "inhibit",
                    "Keep Awake",
                    if state.inhibited { "Holding" } else { "Off" },
                    DesktopCommand::Inhibit,
                ),
                row(
                    "widgets",
                    "Desktop Widgets",
                    widgets_detail,
                    DesktopCommand::ToggleWidgets,
                ),
            ],
        },
        MenuSection {
            title: "Session",
            note: String::new(),
            rows: vec![
                row("lock", "Lock Screen", "", DesktopCommand::Lock),
                row(
                    "suspend",
                    "Sleep",
                    "",
                    DesktopCommand::Power(power::Action::Suspend),
                ),
                row(
                    "restart",
                    "Restart…",
                    "",
                    DesktopCommand::Power(power::Action::Restart),
                ),
                row(
                    "shut-down",
                    "Shut Down…",
                    "",
                    DesktopCommand::Power(power::Action::ShutDown),
                ),
                row(
                    "log-out",
                    "Log Out…",
                    "",
                    DesktopCommand::Power(power::Action::LogOut),
                ),
                row(
                    "session-menu",
                    "Session Menu…",
                    "",
                    DesktopCommand::SessionMenu,
                ),
            ],
        },
        MenuSection {
            title: "Shell",
            note: String::new(),
            rows: vec![
                row("reload", "Reload Configuration", "", DesktopCommand::Reload),
                row("about", "About This System…", "", DesktopCommand::About),
                row("quit", "Quit whelk", "", DesktopCommand::Quit),
            ],
        },
    ]
}

thread_local! {
    static CURRENT: RefCell<Option<DesktopMenu>> = const { RefCell::new(None) };
}

/// Install, read or dismiss the menu the desktop surface talks to.
pub fn install(menu: Option<DesktopMenu>) {
    CURRENT.with(|current| *current.borrow_mut() = menu);
}

pub fn current() -> Option<DesktopMenu> {
    CURRENT.with(|current| current.borrow().clone())
}

/// Close the menu if one is open.  The desktop surface calls this for a click
/// that is not a right click, and every other surface that takes the click
/// away.
pub fn hide_current() {
    if let Some(menu) = current() {
        menu.hide();
    }
}

/// Where the menu was opened, shared with its row handlers without giving
/// them a reference back to the whole menu.
type Origin = Rc<RefCell<Option<(gtk::gdk::Monitor, (i32, i32))>>>;

struct Inner {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    /// The rows live in a scroller with a natural height: a phone-sized
    /// output clips the list at its bottom instead of hanging a card off the
    /// screen.
    scroller: gtk::ScrolledWindow,
    body: gtk::Box,
    css: RefCell<Option<gtk::CssProvider>>,
    state: Rc<dyn Fn() -> DesktopMenuState>,
    on_command: Rc<dyn Fn(DesktopCommand)>,
    /// Where the menu was opened, so the thumbnail collection can open at the
    /// same place when that row is chosen.
    origin: Origin,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(css) = self.css.borrow_mut().take()
            && let Some(display) = gtk::gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }

        self.window.set_child(None::<&gtk::Widget>);
        self.window.destroy();
    }
}

#[derive(Clone)]
pub struct DesktopMenu {
    inner: Rc<Inner>,
}

impl DesktopMenu {
    pub fn new(
        application: &gtk::Application,
        style: &BarSettings,
        state: Rc<dyn Fn() -> DesktopMenuState>,
        on_command: Rc<dyn Fn(DesktopCommand)>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("whelk desktop menu")
            .build();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Desktop"));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        let scroller = gtk::ScrolledWindow::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let hint = gtk::Label::new(Some("Click a row · Esc closes"));

        window.set_widget_name(&style::widget_id("desktop-menu"));
        window.add_css_class("whelk-surface");
        window.set_decorated(false);
        window.set_resizable(false);
        card.add_css_class("whelk-desktop-menu");
        header.add_css_class("whelk-desktop-menu-header");
        title.add_css_class("whelk-desktop-menu-title");
        close.add_css_class("flat");
        close.add_css_class("whelk-desktop-menu-close");
        close.set_tooltip_text(Some("Close (Esc)"));
        hint.add_css_class("whelk-desktop-menu-hint");
        hint.set_halign(gtk::Align::End);
        spacer.set_hexpand(true);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_kinetic_scrolling(true);
        scroller.set_child(Some(&body));

        header.append(&title);
        header.append(&spacer);
        header.append(&close);
        card.append(&header);
        card.append(&scroller);
        card.append(&hint);
        window.set_child(Some(&card));

        let menu = Self {
            inner: Rc::new(Inner {
                window,
                card,
                scroller,
                body,
                css: RefCell::new(None),
                state,
                on_command,
                origin: Rc::new(RefCell::new(None)),
            }),
        };

        menu.configure_surface();
        menu.set_style(style);
        menu.connect_controls(&close);

        menu
    }

    fn configure_surface(&self) {
        let window = self.inner.window.clone();

        if crate::ui::layer_shell_available() {
            window.init_layer_shell();
            window.set_namespace(Some("whelk-rust-desktop-menu"));
            window.set_layer(Layer::Overlay);
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
            window.set_exclusive_zone(0);
            // A menu is a keyboard surface while it is open: Escape, Tab and
            // the arrow keys must reach it instead of the window underneath.
            window.set_keyboard_mode(KeyboardMode::Exclusive);
        }

        window.present();
        window.set_visible(false);
    }

    fn connect_controls(&self, close: &gtk::Button) {
        // The handlers are owned by widgets inside the window, so they hold it
        // weakly: a strong clone would be a window -> child -> closure ->
        // window cycle that no teardown could free.
        let window = self.inner.window.downgrade();

        close.connect_clicked(move |_| {
            if let Some(window) = window.upgrade() {
                window.set_visible(false);
            }
        });

        let window = self.inner.window.downgrade();
        let keys = gtk::EventControllerKey::new();

        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                if let Some(window) = window.upgrade() {
                    window.set_visible(false);
                }

                return gtk::glib::Propagation::Stop;
            }

            gtk::glib::Propagation::Proceed
        });
        self.inner.window.add_controller(keys);
    }

    pub fn set_style(&self, style: &BarSettings) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };

        if let Some(previous) = self.inner.css.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &previous);
        }

        let mut styled = style.clone();

        if styled.radius < 14 {
            styled.radius = 14;
        }

        let widget_id = style::widget_id("desktop-menu");
        let generated = style::css(&widget_id, &styled, "whelk-desktop-menu", None);
        let css = gtk::CssProvider::new();

        css.load_from_string(&format!("{MENU_CSS}\n{generated}"));
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *self.inner.css.borrow_mut() = Some(css);
    }

    pub fn is_visible(&self) -> bool {
        self.inner.window.is_visible()
    }

    pub fn hide(&self) {
        self.inner.window.set_visible(false);
    }

    /// Open the menu at a point on `monitor`, in output-relative pixels.
    pub fn show_at_point(&self, monitor: &gtk::gdk::Monitor, point: (i32, i32)) {
        let state = (self.inner.state)();

        self.render(&state);

        let geometry = monitor.geometry();
        let output = (geometry.width(), geometry.height());

        self.inner
            .scroller
            .set_max_content_height((output.1 - 120).max(160));

        let measured = self.measure_card();
        let card = (measured.0, measured.1.min((output.1 - 24).max(120)));
        let corner = wallpapermenu::placement_near_point(output, card, point);

        *self.inner.origin.borrow_mut() = Some((monitor.clone(), corner));

        self.place(card, corner, Some(monitor));
    }

    /// Draw the sections the state describes, replacing whatever the last
    /// opening drew.
    fn render(&self, state: &DesktopMenuState) {
        while let Some(child) = self.inner.body.first_child() {
            child.unparent();
        }

        for section in sections(state) {
            let title = gtk::Label::new(Some(section.title));
            let title = {
                title.set_xalign(0.0);
                title.add_css_class("whelk-desktop-menu-section");
                title
            };

            self.inner.body.append(&title);

            if !section.note.is_empty() {
                let note = gtk::Label::new(Some(&section.note));
                let note = {
                    note.set_xalign(0.0);
                    note.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                    note.add_css_class("whelk-desktop-menu-note");
                    note
                };

                self.inner.body.append(&note);
            }

            for entry in section.rows {
                self.inner.body.append(&self.row(&entry));
            }
        }
    }

    fn row(&self, entry: &MenuRow) -> gtk::Button {
        let button = gtk::Button::new();
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::new(Some(&entry.label));
        let detail = gtk::Label::new(Some(&entry.detail));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        label.set_xalign(0.0);
        label.add_css_class("whelk-desktop-menu-label");
        label.set_hexpand(false);
        detail.set_xalign(1.0);
        detail.set_halign(gtk::Align::End);
        detail.add_css_class("whelk-desktop-menu-detail");
        detail.set_visible(!entry.detail.is_empty());
        spacer.set_hexpand(true);
        line.append(&label);
        line.append(&spacer);
        line.append(&detail);
        button.set_child(Some(&line));
        button.add_css_class("flat");
        button.add_css_class("whelk-desktop-menu-row");
        button.set_widget_name(entry.id);

        let window = self.inner.window.downgrade();
        let on_command = Rc::clone(&self.inner.on_command);
        let command = entry.command.clone();
        let origin = Rc::clone(&self.inner.origin);

        button.connect_clicked(move |_| {
            if let Some(window) = window.upgrade() {
                window.set_visible(false);
            }

            if command == DesktopCommand::WallpaperCollection {
                let origin = origin.borrow().clone();

                if let Some((monitor, point)) = origin
                    && let Some(menu) = wallpapermenu::current()
                {
                    menu.show_at_point(&monitor, point);
                }

                return;
            }

            on_command(command.clone());
        });

        button
    }

    fn measure_card(&self) -> (i32, i32) {
        let card = &self.inner.card;

        card.set_size_request(-1, -1);

        let (_, width, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
        let width = width.max(1);
        let (_, height, _, _) = card.measure(gtk::Orientation::Vertical, width);

        (width, height.max(1))
    }

    fn place(&self, card: (i32, i32), corner: (i32, i32), monitor: Option<&gtk::gdk::Monitor>) {
        let window = &self.inner.window;

        self.inner.card.set_size_request(card.0, card.1);
        window.set_default_size(card.0, card.1);

        if crate::ui::layer_shell_available() {
            window.set_margin(Edge::Left, corner.0);
            window.set_margin(Edge::Top, corner.1);

            if let Some(monitor) = monitor {
                window.set_monitor(Some(monitor));
            }
        }

        window.set_visible(true);
        window.present();
    }
}

#[cfg(test)]
mod tests {
    use super::{DesktopCommand, DesktopMenuState, sections};

    fn state() -> DesktopMenuState {
        DesktopMenuState {
            night_light: false,
            dnd: true,
            inhibited: false,
            widgets_visible: true,
            widget_count: 13,
            wallpaper: Some("harbour.jpg".to_owned()),
        }
    }

    #[test]
    fn the_menu_offers_every_group_with_its_commands() {
        let sections = sections(&state());
        let titles: Vec<&str> = sections.iter().map(|section| section.title).collect();

        assert_eq!(
            titles,
            vec!["Open", "Wallpaper", "Switches", "Session", "Shell"]
        );

        let commands: Vec<DesktopCommand> = sections
            .iter()
            .flat_map(|section| section.rows.iter().map(|row| row.command.clone()))
            .collect();

        for wanted in [
            DesktopCommand::Launcher("app"),
            DesktopCommand::Launcher("run"),
            DesktopCommand::Launcher("file"),
            DesktopCommand::Launcher("calc"),
            DesktopCommand::Clipboard,
            DesktopCommand::Notifications,
            DesktopCommand::Settings,
            DesktopCommand::WallpaperNext,
            DesktopCommand::WallpaperPrevious,
            DesktopCommand::WallpaperShuffle,
            DesktopCommand::WallpaperCollection,
            DesktopCommand::WallpaperPicker,
            DesktopCommand::NightLight,
            DesktopCommand::Dnd,
            DesktopCommand::Inhibit,
            DesktopCommand::ToggleWidgets,
            DesktopCommand::Lock,
            DesktopCommand::Power(crate::power::Action::Suspend),
            DesktopCommand::Power(crate::power::Action::Restart),
            DesktopCommand::Power(crate::power::Action::ShutDown),
            DesktopCommand::Power(crate::power::Action::LogOut),
            DesktopCommand::SessionMenu,
            DesktopCommand::Reload,
            DesktopCommand::About,
            DesktopCommand::Quit,
        ] {
            assert!(commands.contains(&wanted), "the menu lacks {wanted:?}");
        }
    }

    #[test]
    fn every_row_has_a_unique_identity() {
        let sections = sections(&state());
        let mut ids: Vec<&str> = sections
            .iter()
            .flat_map(|section| section.rows.iter().map(|row| row.id))
            .collect();
        let total = ids.len();

        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), total, "a CSS object name must name one row");
    }

    #[test]
    fn the_switches_say_what_they_are_set_to() {
        let sections = sections(&state());
        let detail = |id: &str| {
            sections
                .iter()
                .flat_map(|section| section.rows.iter())
                .find(|row| row.id == id)
                .map(|row| row.detail.as_str())
                .unwrap_or("")
        };

        assert_eq!(detail("night-light"), "Off");
        assert_eq!(detail("dnd"), "On");
        assert_eq!(detail("inhibit"), "Off");
        assert_eq!(detail("widgets"), "13 shown");
        assert_eq!(
            sections[1].note, "harbour.jpg",
            "the wallpaper section names the picture on screen"
        );
    }

    #[test]
    fn hiding_the_cards_is_said_in_words() {
        let mut state = state();

        state.widgets_visible = false;
        state.widget_count = 5;

        let sections = sections(&state);
        let widgets = sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .find(|row| row.id == "widgets")
            .expect("the widgets row");

        assert_eq!(widgets.detail, "5 hidden");
    }

    #[test]
    fn a_shell_with_no_picture_says_so_instead_of_nothing() {
        let mut state = state();

        state.wallpaper = None;

        assert_eq!(sections(&state)[1].note, "no picture selected");
    }
}

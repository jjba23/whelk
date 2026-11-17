// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `tasks` bar module: every open window as a button, grouped by workspace.
//!
//! A bar with this reading is a taskbar, which is the half a `window` module
//! cannot be: that one names the focused window, and a taskbar has to show the
//! ones that are not focused.  The session layer already knows how to focus and
//! close a window; what it did not have is the list, so this module adds a list
//! command and a parser for each session:
//!
//! | session   | list                          | focus                          | close                          |
//! |-----------+-------------------------------+--------------------------------+--------------------------------|
//! | niri      | `niri msg -j windows`         | `niri msg action focus-window --id` | `niri msg action close-window --id` |
//! | sway      | `swaymsg -t get_tree`         | `swaymsg [con_id=…] focus`     | `swaymsg [con_id=…] kill`      |
//! | Hyprland  | `hyprctl clients -j`          | `hyprctl dispatch focuswindow address:` | `hyprctl dispatch closewindow address:` |
//! | X11       | `wmctrl -lp`                  | `wmctrl -i -a`                 | `wmctrl -i -c`                 |
//!
//! Clicking a button focuses its window and the middle button closes it, which
//! is the same vocabulary the modules already share.  The parsing and the
//! grouping are pure, so they are tested without a display; the module only
//! paints what they return.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::command;
use crate::format;
use crate::json::Json;
use crate::session::Session;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 2;
const DEFAULT_MAX_LENGTH: i32 = 28;

/// One window the session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWindow {
    /// The session's own handle: niri's id, sway's con id, Hyprland's address,
    /// or wmctrl's window id.  It is what a focus or close command needs.
    pub id: String,
    pub title: String,
    pub app_id: String,
    /// The workspace number the session sorts by; 0 when it does not say.
    pub workspace: i32,
    pub active: bool,
}

impl TaskWindow {
    /// The word a bar can show for this window: the title, or the application
    /// when the title is empty.
    pub fn label(&self, max_length: i32) -> String {
        let text = if self.title.trim().is_empty() {
            self.app_id.trim()
        } else {
            self.title.trim()
        };

        format::truncate_utf8(text, max_length)
    }
}

/// The command that lists the session's windows, or `None` for a session with
/// no list verb.
pub fn list_command(session: Session) -> Option<&'static str> {
    Some(match session {
        Session::Niri => "niri msg -j windows",
        Session::Sway => "swaymsg -t get_tree",
        Session::Hyprland => "hyprctl clients -j",
        Session::X11 => "wmctrl -lp 2>/dev/null",
        Session::River | Session::Wayland => return None,
    })
}

/// The command that focuses `id`.
pub fn focus_command(session: Session, id: &str) -> Option<String> {
    let id = shell_quote(id);

    Some(match session {
        Session::Niri => format!("niri msg action focus-window --id {id}"),
        Session::Sway => format!("swaymsg '[con_id={id}]' focus"),
        Session::Hyprland => format!("hyprctl dispatch focuswindow address:{id}"),
        Session::X11 => format!("wmctrl -i -a {id}"),
        Session::River | Session::Wayland => return None,
    })
}

/// The command that closes `id`.
pub fn close_command(session: Session, id: &str) -> Option<String> {
    let id = shell_quote(id);

    Some(match session {
        Session::Niri => format!("niri msg action close-window --id {id}"),
        Session::Sway => format!("swaymsg '[con_id={id}]' kill"),
        Session::Hyprland => format!("hyprctl dispatch closewindow address:{id}"),
        Session::X11 => format!("wmctrl -i -c {id}"),
        Session::River | Session::Wayland => return None,
    })
}

/// Parse a session's window list.  An answer this module does not understand is
/// an empty list, not an error: a bar must not break because a compositor
/// changed a field.
pub fn parse(session: Session, text: &str) -> Vec<TaskWindow> {
    match session {
        Session::Niri => niri(text),
        Session::Sway => sway(text),
        Session::Hyprland => hyprland(text),
        Session::X11 => x11(text),
        Session::River | Session::Wayland => Vec::new(),
    }
}

/// `niri msg -j windows`: a flat array, each with its id, title, app id,
/// workspace and focus flag.
fn niri(text: &str) -> Vec<TaskWindow> {
    let Ok(Json::Array(windows)) = Json::parse(text) else {
        return Vec::new();
    };

    windows
        .iter()
        .filter_map(|window| {
            let id = window.get("id")?.as_f64()? as i64;

            Some(TaskWindow {
                id: id.to_string(),
                title: window.get_str("title").unwrap_or_default().to_owned(),
                app_id: window.get_str("app_id").unwrap_or_default().to_owned(),
                workspace: window.get_number("workspace_id", 0.0) as i32,
                active: window.get_bool("is_focused", false),
            })
        })
        .collect()
}

/// `hyprctl clients -j`: the address, the title and class, and the workspace
/// object's id; the focused client is the one at the top of the focus history.
fn hyprland(text: &str) -> Vec<TaskWindow> {
    let Ok(Json::Array(clients)) = Json::parse(text) else {
        return Vec::new();
    };
    let mut windows: Vec<TaskWindow> = clients
        .iter()
        .filter_map(|client| {
            let address = client.get_str("address")?;

            Some(TaskWindow {
                id: address.to_owned(),
                title: client.get_str("title").unwrap_or_default().to_owned(),
                app_id: client.get_str("class").unwrap_or_default().to_owned(),
                workspace: client
                    .get("workspace")
                    .map(|workspace| workspace.get_number("id", 0.0) as i32)
                    .unwrap_or(0),
                active: client.get_number("focusHistoryID", f64::MAX) == 0.0,
            })
        })
        .collect();

    windows.sort_by_key(|window| (!window.active, window.workspace, window.id.clone()));

    windows
}

/// `swaymsg -t get_tree`: a tree of containers; the leaves are the windows, and
/// the workspace is the container that names it.
fn sway(text: &str) -> Vec<TaskWindow> {
    let Ok(tree) = Json::parse(text) else {
        return Vec::new();
    };
    let mut windows = Vec::new();

    walk_sway(&tree, 0, &mut windows);
    windows
}

fn walk_sway(node: &Json, workspace: i32, windows: &mut Vec<TaskWindow>) {
    let kind = node.get_str("type").unwrap_or_default();
    let workspace = if kind == "workspace" {
        node.get_number("num", 0.0) as i32
    } else {
        workspace
    };

    let is_window = kind == "con" || kind == "floating_con";

    if is_window {
        // A container with children is a split, not a window: only a leaf has a
        // surface.
        windows.push(TaskWindow {
            id: (node.get_number("id", 0.0) as i64).to_string(),
            title: node.get_str("name").unwrap_or_default().to_owned(),
            app_id: node.get_str("app_id").unwrap_or_default().to_owned(),
            workspace,
            active: node.get_bool("focused", false),
        });
    }

    if let Some(Json::Array(children)) = node.get("nodes") {
        for child in children {
            walk_sway(child, workspace, windows);
        }
    }

    if let Some(Json::Array(children)) = node.get("floating_nodes") {
        for child in children {
            walk_sway(child, workspace, windows);
        }
    }
}

/// `wmctrl -lp`: `id desktop pid host title`.
fn x11(text: &str) -> Vec<TaskWindow> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();

            if fields.len() < 5 {
                return None;
            }

            let id = fields[0];
            let workspace = fields[1];
            let title = fields[4..].join(" ");

            if !id.starts_with("0x") {
                return None;
            }

            Some(TaskWindow {
                id: id.to_owned(),
                title: title.trim().to_owned(),
                app_id: String::new(),
                workspace: workspace.parse().unwrap_or(0) + 1,
                active: false,
            })
        })
        .collect()
}

/// The windows grouped by workspace, workspaces in order.
pub fn group(windows: &[TaskWindow]) -> Vec<(i32, Vec<&TaskWindow>)> {
    let mut groups: BTreeMap<i32, Vec<&TaskWindow>> = BTreeMap::new();

    for window in windows {
        groups.entry(window.workspace).or_default().push(window);
    }

    groups.into_iter().collect()
}

/// `value` for a command line: untouched when it is plainly an id, quoted when
/// it could be read as shell syntax.
fn shell_quote(value: &str) -> String {
    let plain = !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | ':' | '/' | '+' | '-' | '#')
        });

    if plain {
        value.to_owned()
    } else {
        gtk::glib::shell_quote(value).to_string_lossy().into_owned()
    }
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let session = context.session;
    let max_length = module
        .integer("max-length")
        .unwrap_or(i64::from(DEFAULT_MAX_LENGTH)) as i32;
    let hide_empty = module.boolean("hide-empty-workspaces?").unwrap_or(true);
    let show_workspace = module.boolean("show-workspaces?").unwrap_or(true);
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    let empty = gtk::Label::new(None);

    empty.add_css_class("whelk-task-empty");
    empty.set_visible(false);
    root.append(&empty);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);

    row.add_css_class("whelk-tasks");
    root.append(&row);

    let built = Rc::new(RefCell::new(TaskState {
        row,
        empty,
        session,
        max_length,
        hide_empty,
        show_workspace,
        windows: Vec::new(),
    }));
    let update_state = Rc::clone(&built);

    host.start(move |host| update(host, &update_state));

    Some(root)
}

struct TaskState {
    row: gtk::Box,
    empty: gtk::Label,
    session: Session,
    max_length: i32,
    hide_empty: bool,
    show_workspace: bool,
    windows: Vec<TaskWindow>,
}

fn update(host: &ModuleHost, state: &Rc<RefCell<TaskState>>) {
    let Some(command_line) = list_command(state.borrow().session) else {
        return;
    };
    let weak = Rc::downgrade(state);

    host.run_command(command_line, move |text| {
        let Some(state) = weak.upgrade() else {
            return;
        };
        let windows = {
            let state = state.borrow();

            parse(state.session, text.as_deref().unwrap_or_default())
        };

        paint(&state, windows);
    });
}

fn paint(state: &Rc<RefCell<TaskState>>, windows: Vec<TaskWindow>) {
    let (row, empty, max_length, hide_empty, show_workspace) = {
        let state = state.borrow();

        (
            state.row.clone(),
            state.empty.clone(),
            state.max_length,
            state.hide_empty,
            state.show_workspace,
        )
    };

    while let Some(child) = row.first_child() {
        row.remove(&child);
    }

    empty.set_visible(windows.is_empty());
    row.set_visible(!windows.is_empty());

    for (workspace, group) in group(&windows) {
        if show_workspace && !(hide_empty && group.is_empty()) {
            let label = gtk::Label::new(Some(&workspace.to_string()));

            label.add_css_class("whelk-task-workspace");
            label.set_tooltip_text(Some(&format!("Workspace {workspace}")));
            row.append(&label);
        }

        for window in group {
            let button = gtk::Button::new();
            let text = gtk::Label::new(Some(&window.label(max_length)));

            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            button.set_child(Some(&text));
            button.add_css_class("flat");
            button.add_css_class("whelk-task");

            if window.active {
                button.add_css_class("active");
            }

            let tooltip = if window.app_id.is_empty() {
                format!("{}\nClick: focus · Middle click: close", window.title)
            } else {
                format!(
                    "{} — {}\nClick: focus · Middle click: close",
                    window.app_id, window.title
                )
            };

            button.set_tooltip_text(Some(&tooltip));

            let id = window.id.clone();
            let clicked = Rc::downgrade(state);

            button.connect_clicked(move |_| {
                if let Some(state) = clicked.upgrade() {
                    let session = state.borrow().session;

                    if let Some(command) = focus_command(session, &id) {
                        command::spawn(&command);
                    }
                }
            });

            let id = window.id.clone();
            let middle = Rc::downgrade(state);
            let gesture = gtk::GestureClick::new();

            gesture.set_button(gtk::gdk::BUTTON_MIDDLE);
            gesture.connect_pressed(move |gesture, _, _, _| {
                let Some(state) = middle.upgrade() else {
                    return;
                };
                let session = state.borrow().session;

                gesture.set_state(gtk::EventSequenceState::Claimed);

                if let Some(command) = close_command(session, &id) {
                    command::spawn(&command);
                }
            });
            button.add_controller(gesture);

            row.append(&button);
        }
    }

    state.borrow_mut().windows = windows;
}

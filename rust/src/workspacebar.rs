// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The workspace bar, wired to the session and workspace modules.
//!
//! The C module detects the compositor on every refresh and runs its workspace
//! command; the Rust shell detects once at startup, then polls the command for
//! that session.  Both paint a row of labels rather than one reading: the row
//! is cleared and rebuilt after each answer so a workspace that disappeared
//! cannot leave a stale button behind.
//!
//! Everything that decides what a button says is pure: the spec defaults, the
//! format substitution, `hide-empty?` filtering and the focus command.  The
//! shared interval, timeout, cancellation and click/scroll lifecycle lives in
//! [`crate::barmodule`]; this file supplies the reading and its row.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::command;
use crate::format;
use crate::session::Session;
use crate::workspaces::{self, Workspace};

pub const DEFAULT_FORMAT: &str = "{index}";
pub const DEFAULT_INTERVAL_SECONDS: u32 = crate::barmodule::DEFAULT_INTERVAL_SECONDS;

/// The parts of a `workspaces` module spec that affect its labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBarSpec {
    pub format: String,
    pub hide_empty: bool,
}

impl WorkspaceBarSpec {
    pub fn from_module(module: &ModuleSpec) -> Self {
        Self {
            format: module
                .text("format")
                .unwrap_or_else(|| DEFAULT_FORMAT.to_owned()),
            hide_empty: module.boolean("hide-empty?").unwrap_or(false),
        }
    }
}

/// One painted workspace, with everything its label and click need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceButton {
    pub index: i32,
    pub label: String,
    pub active: bool,
    pub empty: bool,
    pub focus_command: Option<String>,
}

/// Parse one compositor answer into the row's buttons.  `None` means the answer
/// was not the JSON this session promises; an empty vector is a real, empty
/// workspace set.
pub fn buttons(
    session: Session,
    text: &str,
    spec: &WorkspaceBarSpec,
) -> Option<Vec<WorkspaceButton>> {
    let parsed = workspaces::parse(session, text)?;

    Some(
        parsed
            .into_iter()
            .filter(|workspace| !(spec.hide_empty && workspace.empty))
            .map(|workspace| WorkspaceButton {
                index: workspace.index,
                label: label(&spec.format, &workspace),
                active: workspace.active,
                empty: workspace.empty,
                focus_command: workspaces::focus_command(session, workspace.index),
            })
            .collect(),
    )
}

/// Substitute `{index}` then `{name}`, in the same order as the C module.  A
/// name that itself contains `{index}` is therefore left alone.
pub fn label(format: &str, workspace: &Workspace) -> String {
    let index = workspace.index.to_string();
    let text = format::subst(format, "index", &index);

    format::subst(&text, "name", &workspace.name)
}

/// Build the GTK module.  The first refresh starts immediately; the shared
/// host owns the interval timer, the one-command-at-a-time guard and the
/// cancellation of a command whose module is torn down.
pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let spec = WorkspaceBarSpec::from_module(module);
    let session = context.session;
    let (root, host) = ModuleHost::new(
        module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.start(move |host| refresh(session, &spec, host));

    Some(root)
}

fn refresh(session: Session, spec: &WorkspaceBarSpec, host: &ModuleHost) {
    let Some(command_line) = workspaces::command(session) else {
        if let Some(root) = host.root() {
            clear(&root);
        }

        return;
    };
    let spec = spec.clone();
    let callback_host = host.clone();

    host.run_command(&command_line, move |text| {
        let Some(root) = callback_host.root() else {
            return;
        };
        let Some(text) = text else {
            return;
        };
        let Some(buttons) = buttons(session, &text, &spec) else {
            eprintln!(
                "whelk-rs: workspaces: {}'s answer was not the expected JSON",
                session.name()
            );

            // A garbled answer is not an empty workspace set: keep the last
            // good row until the compositor answers again.
            return;
        };

        paint(&root, &buttons);
    });
}

fn paint(root: &gtk::Box, buttons: &[WorkspaceButton]) {
    clear(root);

    for workspace in buttons {
        root.append(&workspace_label(workspace));
    }
}

fn workspace_label(workspace: &WorkspaceButton) -> gtk::Label {
    let label = gtk::Label::new(None);

    label.set_markup(&format::markup_escape(&workspace.label));
    label.add_css_class("whelk-workspace");
    label.add_css_class(if workspace.active {
        "active"
    } else {
        "inactive"
    });

    if workspace.empty {
        label.add_css_class("empty");
    }

    if let Some(command_line) = workspace.focus_command.as_deref() {
        let gesture = gtk::GestureClick::new();

        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);

        let command_line = command_line.to_owned();

        gesture.connect_pressed(move |_, _, _, _| command::spawn(&command_line));
        label.add_controller(gesture);
        label.set_cursor_from_name(Some("pointer"));
    }

    label
}

fn clear(root: &gtk::Box) {
    while let Some(child) = root.first_child() {
        child.unparent();
    }
}

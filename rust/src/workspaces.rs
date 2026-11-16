// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Workspace parsing, ported from `util/workspaces.c`.
//!
//! niri and Hyprland both publish their workspaces as JSON, but not in the
//! same shape: niri has `idx`, `is_active`/`is_focused` and
//! `active_window_id`; Hyprland has `id`, `windows`, and the active workspace
//! is found on its monitors.  This is the translation both compositors fit
//! through, kept away from GTK so the real answer shapes are testable without
//! a display.

use crate::json::Json;
use crate::session::Session;

const HYPRLAND_MONITORS_MARKER: &str = "\nWHELK_HYPRLAND_MONITORS\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// niri's `idx`, or Hyprland's `id`.
    pub index: i32,
    /// The workspace's name, never empty (the C type promises a string).
    pub name: String,
    pub active: bool,
    pub empty: bool,
}

/// The command whose output [`parse`] understands; `None` when the session has
/// no workspace IPC.
pub fn command(session: Session) -> Option<String> {
    match session {
        Session::Niri => Some("niri msg -j workspaces".to_owned()),
        Session::Hyprland => Some(
            "hyprctl -j workspaces; echo; echo WHELK_HYPRLAND_MONITORS; echo; hyprctl -j monitors"
                .to_owned(),
        ),
        Session::Sway | Session::River | Session::X11 | Session::Wayland => None,
    }
}

/// The command that focuses `index` on this session, or `None`.
pub fn focus_command(session: Session, index: i32) -> Option<String> {
    if index < 0 {
        return None;
    }

    match session {
        Session::Niri => Some(format!("niri msg action focus-workspace {index}")),
        Session::Hyprland => Some(format!("hyprctl dispatch workspace {index}")),
        Session::Sway | Session::River | Session::X11 | Session::Wayland => None,
    }
}

/// One workspace per compositor entry, in the compositor's order.  `None` when
/// the answer was not the JSON this session promises; an empty vector is a
/// valid answer with no workspaces.
pub fn parse(session: Session, text: &str) -> Option<Vec<Workspace>> {
    if text.is_empty() {
        return None;
    }

    match session {
        Session::Niri => parse_niri(text),
        Session::Hyprland => parse_hyprland(text),
        Session::Sway | Session::River | Session::X11 | Session::Wayland => None,
    }
}

fn parse_niri(text: &str) -> Option<Vec<Workspace>> {
    let root = Json::parse(text).ok()?;
    let workspaces = root.array()?;
    let mut out = Vec::new();

    for workspace in workspaces {
        let index = workspace.get_number("idx", -1.0);

        if index < 0.0 {
            continue;
        }

        let active =
            workspace.get_bool("is_active", false) || workspace.get_bool("is_focused", false);
        let empty = workspace.get("active_window_id").is_none_or(Json::is_null);

        out.push(Workspace {
            index: index as i32,
            name: workspace.get_str("name").unwrap_or("").to_owned(),
            active,
            empty,
        });
    }

    Some(out)
}

fn parse_hyprland(text: &str) -> Option<Vec<Workspace>> {
    let (workspaces_text, monitors_text) = match text.find(HYPRLAND_MONITORS_MARKER) {
        Some(position) => (
            &text[..position],
            Some(&text[position + HYPRLAND_MONITORS_MARKER.len()..]),
        ),
        None => (text, None),
    };

    let root = Json::parse(workspaces_text).ok()?;
    let workspaces = root.array()?;
    let active = monitors_text
        .and_then(|text| Json::parse(text).ok())
        .map(|monitors| collect_active(&monitors))
        .unwrap_or_default();
    let mut out = Vec::new();

    for workspace in workspaces {
        let index = workspace.get_number("id", -1.0);

        // Special workspaces are a scratchpad, not a place on the row.
        if index < 1.0 {
            continue;
        }

        let windows = workspace.get_number("windows", 0.0);
        let index = index as i32;

        out.push(Workspace {
            index,
            name: workspace.get_str("name").unwrap_or("").to_owned(),
            active: active.contains(&(index as f64)) || workspace.get_bool("active", false),
            empty: windows <= 0.0,
        });
    }

    Some(out)
}

fn collect_active(monitors: &Json) -> Vec<f64> {
    let Some(monitors) = monitors.array() else {
        return Vec::new();
    };
    let mut active = Vec::new();

    for monitor in monitors {
        let id = monitor
            .get("activeWorkspace")
            .map_or(-1.0, |workspace| workspace.get_number("id", -1.0));

        if id >= 0.0 {
            active.push(id);
        }
    }

    active
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The session enum and its environment guess, from `util/session.h`.
//!
//! The environment is the first guess, never the verdict: `NIRI_SOCKET` and
//! `SWAYSOCK` are set for the processes a compositor spawns, and absent for one
//! started by a service unit, a login shell or another compositor's session.
//! The probing half of `whelk_session_detect()` belongs to whoever owns the
//! process transport; this module keeps the values and the names.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    Niri,
    Sway,
    Hyprland,
    River,
    X11,
    /// A Wayland compositor with no shell-facing IPC.
    Wayland,
}

impl Session {
    pub fn name(self) -> &'static str {
        match self {
            Self::Niri => "niri",
            Self::Sway => "sway",
            Self::Hyprland => "hyprland",
            Self::River => "river",
            Self::X11 => "x11",
            Self::Wayland => "wayland",
        }
    }

    /// The environment's answer alone, without touching the compositor.
    pub fn from_environment() -> Self {
        Self::from_values(
            std::env::var("NIRI_SOCKET").ok().as_deref(),
            std::env::var("SWAYSOCK").ok().as_deref(),
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref(),
            std::env::var("RIVER_SOCKET").ok().as_deref(),
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("DISPLAY").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        )
    }

    /// The testable half: the same precedence the C `from_environment` uses.
    #[allow(clippy::too_many_arguments)]
    pub fn from_values(
        niri: Option<&str>,
        sway: Option<&str>,
        hyprland: Option<&str>,
        river: Option<&str>,
        session_type: Option<&str>,
        display: Option<&str>,
        wayland: Option<&str>,
    ) -> Self {
        if nonempty(niri) {
            return Self::Niri;
        }

        if nonempty(sway) {
            return Self::Sway;
        }

        if nonempty(hyprland) {
            return Self::Hyprland;
        }

        if nonempty(river) {
            return Self::River;
        }

        if session_type.is_some_and(|value| value.eq_ignore_ascii_case("x11")) {
            return Self::X11;
        }

        if !nonempty(wayland) && nonempty(display) {
            return Self::X11;
        }

        Self::Wayland
    }
}

fn nonempty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

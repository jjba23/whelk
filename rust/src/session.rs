// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Session detection, naming and probes, ported from `util/session.c`.
//!
//! WheLK talks to the compositor the way a shell has to: through the tools that
//! exist.  The environment is the first guess, never the verdict: `NIRI_SOCKET`
//! and `SWAYSOCK` are set for the processes a compositor spawns, and absent for
//! one started by a service unit, a login shell or another compositor's
//! session.  When they are missing, the compositor is asked instead -- `niri
//! msg` finds its own default socket without the variable, and sway's socket is
//! found in the runtime directory -- because a bar that goes blank because a
//! variable was not exported is a bar that has forgotten what it can do.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// sway's CLI takes the socket from the environment, and a sway started by a
/// service unit exports `SWAYSOCK` only to the processes it spawned.  The
/// socket itself sits in the runtime directory, newest first -- the same one
/// `swaymsg` would have used.
macro_rules! sway_command {
    ($command:literal) => {
        concat!(
            "if [ -z \"$SWAYSOCK\" ]; then ",
            "SWAYSOCK=$(ls -t \"$XDG_RUNTIME_DIR\"/sway-ipc.*.sock 2>/dev/null | head -1); ",
            "[ -n \"$SWAYSOCK\" ] && export SWAYSOCK; fi; ",
            $command
        )
    };
}

const NIRI_PROBE: &str = "niri msg -j version";
const SWAY_PROBE: &str = sway_command!("swaymsg -t get_version");
const HYPRLAND_PROBE: &str = "hyprctl version";
const RIVER_PROBE: &str = "riverctl -version";

/// Cheap, side-effect free, and answerable without the environment: niri finds
/// its own default socket by path, sway's is found by the prefix above.
const PROBES: [(Session, &str, &str); 4] = [
    (Session::Niri, NIRI_PROBE, "niri msg -j version answered"),
    (Session::Sway, SWAY_PROBE, "swaymsg -t get_version answered"),
    (
        Session::Hyprland,
        HYPRLAND_PROBE,
        "hyprctl version answered",
    ),
    (Session::River, RIVER_PROBE, "riverctl -version answered"),
];

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

/// A session and the evidence that named it.  `evidence` is `None` only when
/// the environment did not name a compositor; that is the case detection has to
/// probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub session: Session,
    pub evidence: Option<&'static str>,
}

impl Detection {
    /// The environment's answer alone, without touching the compositor.
    pub fn from_environment() -> Self {
        Self::from_values(
            env_var("NIRI_SOCKET").as_deref(),
            env_var("SWAYSOCK").as_deref(),
            env_var("HYPRLAND_INSTANCE_SIGNATURE").as_deref(),
            env_var("RIVER_SOCKET").as_deref(),
            env_var("XDG_SESSION_TYPE").as_deref(),
            env_var("DISPLAY").as_deref(),
            env_var("WAYLAND_DISPLAY").as_deref(),
        )
    }

    /// The testable half: the same precedence the C `from_environment` uses,
    /// with the name of the variable that decided it.
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
            return Self {
                session: Session::Niri,
                evidence: Some("NIRI_SOCKET"),
            };
        }

        if nonempty(sway) {
            return Self {
                session: Session::Sway,
                evidence: Some("SWAYSOCK"),
            };
        }

        // Hyprland exports its instance signature to everything it starts;
        // river relies on the probe, because it publishes no socket variable of
        // its own.
        if nonempty(hyprland) {
            return Self {
                session: Session::Hyprland,
                evidence: Some("HYPRLAND_INSTANCE_SIGNATURE"),
            };
        }

        if nonempty(river) {
            return Self {
                session: Session::River,
                evidence: Some("RIVER_SOCKET"),
            };
        }

        if session_type.is_some_and(|value| value.eq_ignore_ascii_case("x11")) {
            return Self {
                session: Session::X11,
                evidence: Some("XDG_SESSION_TYPE=x11"),
            };
        }

        if !nonempty(wayland) && nonempty(display) {
            return Self {
                session: Session::X11,
                evidence: Some("DISPLAY without WAYLAND_DISPLAY"),
            };
        }

        Self {
            session: Session::Wayland,
            evidence: None,
        }
    }

    /// Ask each compositor in turn when the environment named none.  An
    /// environment answer, even X11 or Wayland, is already a verdict and is
    /// returned untouched.
    pub fn probe<F>(self, mut answers: F) -> Self
    where
        F: FnMut(&str) -> bool,
    {
        if self.evidence.is_some() {
            return self;
        }

        for &(session, command, evidence) in &PROBES {
            if answers(command) {
                return Self {
                    session,
                    evidence: Some(evidence),
                };
            }
        }

        self
    }
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
        Detection::from_environment().session
    }

    /// The environment's answer as a plain session, without its evidence.
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
        Detection::from_values(niri, sway, hyprland, river, session_type, display, wayland).session
    }

    /// The compositor's answer from a probe, for callers that supply their own
    /// transport (and for the tests).  Wayland when nothing answers.
    pub fn from_probe<F>(answers: F) -> Self
    where
        F: FnMut(&str) -> bool,
    {
        Detection {
            session: Self::Wayland,
            evidence: None,
        }
        .probe(answers)
        .session
    }

    /// The command that proves this compositor is reachable, or `None` for
    /// sessions that have nothing to ask.
    pub fn probe_command(self) -> Option<&'static str> {
        PROBES
            .iter()
            .find(|(session, _, _)| *session == self)
            .map(|(_, command, _)| *command)
    }

    /// Detect once, by the environment and then by asking the compositor;
    /// logged either way, so a session that ends up unsupported says so.
    pub fn detect() -> Self {
        static DETECTED: OnceLock<Session> = OnceLock::new();

        *DETECTED.get_or_init(|| {
            let detected =
                Detection::from_environment().probe(|command| command_answers(command, 400));

            match detected.evidence {
                Some(evidence) => eprintln!(
                    "whelk-rs: session {}, from {}",
                    detected.session.name(),
                    evidence
                ),
                None => eprintln!(
                    "whelk-rs: session: no compositor answered (niri msg, swaymsg, \
                     hyprctl, riverctl); the window and menu modules stay empty \
                     until one does"
                ),
            }

            detected.session
        })
    }
}

/// Run `command` through a shell and report whether it exited zero.  Output is
/// discarded: the exit status is the whole answer, and a compositor that hangs
/// must not hang the bar.
pub fn command_answers(command: &str, timeout_ms: u64) -> bool {
    if command.is_empty() {
        return false;
    }

    let mut child = match Command::new(probe_shell())
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();

                return false;
            }
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();

            return false;
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The probe's shell is looked up rather than assumed: a Guix System and the
/// package's build sandbox have no `/bin/sh`, only the store's bash.
fn probe_shell() -> PathBuf {
    static SHELL: OnceLock<PathBuf> = OnceLock::new();

    SHELL
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("PATH") {
                for directory in std::env::split_paths(&path) {
                    let candidate = directory.join("sh");

                    if is_executable(&candidate) {
                        return candidate;
                    }
                }
            }

            PathBuf::from("/bin/sh")
        })
        .clone()
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn nonempty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Session detection, naming, probes and focused-window knowledge, ported
//! from `util/session.c`.
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

use crate::json::Json;

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

const X11_FOCUSED_WINDOW: &str = concat!(
    "id=$(xprop -root _NET_ACTIVE_WINDOW 2>/dev/null | sed 's/.* //'); ",
    "[ -n \"$id\" ] && [ \"$id\" != 0x0 ] && ",
    "printf 'WHELK_ACTIVE_WINDOW=%s\\n' \"$id\" && ",
    "xprop -id \"$id\" WM_NAME WM_CLASS _NET_WM_PID _GTK_UNIQUE_BUS_NAME ",
    "_GTK_APPLICATION_OBJECT_PATH _GTK_WINDOW_OBJECT_PATH ",
    "_GTK_MENUBAR_OBJECT_PATH _GTK_APP_MENU_OBJECT_PATH 2>/dev/null"
);

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
                    "whelk: session {}, from {}",
                    detected.session.name(),
                    evidence
                ),
                None => eprintln!(
                    "whelk: session: no compositor answered (niri msg, swaymsg, \
                     hyprctl, riverctl); the window and menu modules stay empty \
                     until one does"
                ),
            }

            detected.session
        })
    }
}

/// The focused window's title and application id, with either half empty when
/// the session only reports the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedWindow {
    pub title: String,
    pub app_id: String,
}

/// The focused-window verbs a session can carry out.  A `None` field is a verb
/// the session has no reliable command for; the menu leaves it out rather than
/// showing an entry that does nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowActions {
    pub close: Option<String>,
    pub fullscreen: Option<String>,
    pub maximize: Option<String>,
    pub float_window: Option<String>,
    pub move_left: Option<String>,
    pub move_right: Option<String>,
    pub move_up: Option<String>,
    pub move_down: Option<String>,
}

/// The GTK paths an X11 window publishes, and the window id they belong to.
/// Wayland does not expose another client's properties, so this is empty there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusedPaths {
    pub window_id: u32,
    pub bus_name: Option<String>,
    pub app_path: Option<String>,
    pub window_path: Option<String>,
    pub menubar_path: Option<String>,
    pub appmenu_path: Option<String>,
}

impl Session {
    /// The command that prints the focused window in the shape this session's
    /// parser understands; `None` when the session cannot answer.
    pub fn focused_window_command(self) -> Option<String> {
        Some(
            match self {
                Self::Niri => "niri msg -j focused-window",
                Self::Sway => sway_command!("swaymsg -t get_tree"),
                Self::Hyprland => "hyprctl activewindow -j",
                Self::River | Self::Wayland => return None,
                Self::X11 => X11_FOCUSED_WINDOW,
            }
            .to_owned(),
        )
    }

    /// The command that closes the focused window; `None` when there is none.
    pub fn close_window_command(self) -> Option<String> {
        Some(
            match self {
                Self::Niri => "niri msg action close-window",
                Self::Sway => sway_command!("swaymsg kill"),
                Self::Hyprland => "hyprctl dispatch killactive",
                Self::River => "riverctl close",
                Self::X11 => {
                    "wmctrl -c :ACTIVE: 2>/dev/null || xdotool getactivewindow windowclose \
                     2>/dev/null"
                }
                Self::Wayland => return None,
            }
            .to_owned(),
        )
    }

    /// The command that toggles the focused window's full-screen state; `None`
    /// when the session has no such action.
    pub fn fullscreen_window_command(self) -> Option<String> {
        Some(
            match self {
                Self::Niri => "niri msg action fullscreen-window",
                Self::Sway => sway_command!("swaymsg fullscreen toggle"),
                Self::Hyprland => "hyprctl dispatch fullscreen 0",
                Self::River => "riverctl toggle-fullscreen",
                Self::X11 => {
                    "wmctrl -r :ACTIVE: -b toggle,fullscreen 2>/dev/null || xdotool \
                     getactivewindow key F11 2>/dev/null"
                }
                Self::Wayland => return None,
            }
            .to_owned(),
        )
    }

    /// Every focused-window verb this session can carry out.
    pub fn window_actions(self) -> WindowActions {
        let mut actions = WindowActions {
            close: self.close_window_command(),
            fullscreen: self.fullscreen_window_command(),
            ..WindowActions::default()
        };

        match self {
            Self::Niri => {
                actions.maximize = Some("niri msg action maximize-column".to_owned());
                actions.float_window = Some("niri msg action toggle-window-floating".to_owned());
                actions.move_left = Some("niri msg action move-window-left".to_owned());
                actions.move_right = Some("niri msg action move-window-right".to_owned());
                actions.move_up = Some("niri msg action move-window-up".to_owned());
                actions.move_down = Some("niri msg action move-window-down".to_owned());
            }
            Self::Sway => {
                actions.float_window = Some(sway_command!("swaymsg floating toggle").to_owned());
                actions.move_left = Some(sway_command!("swaymsg move left").to_owned());
                actions.move_right = Some(sway_command!("swaymsg move right").to_owned());
                actions.move_up = Some(sway_command!("swaymsg move up").to_owned());
                actions.move_down = Some(sway_command!("swaymsg move down").to_owned());
            }
            Self::Hyprland => {
                actions.maximize = Some("hyprctl dispatch fullscreen 1".to_owned());
                actions.float_window = Some("hyprctl dispatch togglefloating".to_owned());
                actions.move_left = Some("hyprctl dispatch movewindow l".to_owned());
                actions.move_right = Some("hyprctl dispatch movewindow r".to_owned());
                actions.move_up = Some("hyprctl dispatch movewindow u".to_owned());
                actions.move_down = Some("hyprctl dispatch movewindow d".to_owned());
            }
            Self::River => {
                // riverctl floats the view, but its `move' takes a pixel delta
                // rather than a direction, so the directional verbs stay unset.
                actions.float_window = Some("riverctl toggle-float".to_owned());
            }
            Self::X11 => {
                actions.maximize = Some(
                    "wmctrl -r :ACTIVE: -b toggle,maximized_vert,maximized_horz 2>/dev/null"
                        .to_owned(),
                );
            }
            Self::Wayland => {}
        }

        actions
    }

    /// The program that types a key chord into the focused window: `wtype` on
    /// Wayland, `xdotool` on X11, or `None` when it is not installed.
    pub fn shortcut_tool(self) -> Option<String> {
        let wanted = if self == Self::X11 {
            "xdotool"
        } else {
            "wtype"
        };

        program_in_path(wanted)
    }

    /// Understands niri's focused-window JSON, sway's window tree, the
    /// `WM_NAME`/`WM_CLASS` lines of xprop and river-status's focused-view
    /// title.  `None` when no window was found.
    pub fn parse_focused(self, text: &str) -> Option<FocusedWindow> {
        if text.is_empty() {
            return None;
        }

        match self {
            Self::Niri => parse_niri(text),
            Self::Sway => parse_sway(text),
            Self::Hyprland => parse_hyprland(text),
            Self::River => parse_river(text),
            Self::X11 => parse_x11(text),
            Self::Wayland => None,
        }
    }

    /// The focused application's process id, or 0 when the session does not
    /// report one.  AT-SPI matches an application by pid first, because a name
    /// can be translated and a pid cannot.
    pub fn focused_pid(self, text: &str) -> i32 {
        if text.is_empty() {
            return 0;
        }

        if self == Self::X11 {
            const PREFIX: &str = "_NET_WM_PID(CARDINAL) = ";

            return text.find(PREFIX).map_or(0, |position| {
                parse_i64_prefix(&text[position + PREFIX.len()..]) as i32
            });
        }

        if !matches!(self, Self::Niri | Self::Sway | Self::Hyprland) {
            return 0;
        }

        let Ok(root) = Json::parse(text) else {
            return 0;
        };

        if self == Self::Sway {
            sway_focused_pid(&root)
        } else {
            root.get_number("pid", 0.0) as i32
        }
    }

    /// On X11, GTK exports the exact bus name and menu object paths as window
    /// properties.  Other sessions leave the result empty: their protocols do
    /// not expose another client's properties.
    pub fn parse_focused_paths(self, text: &str) -> FocusedPaths {
        let mut paths = FocusedPaths::default();

        if self != Self::X11 {
            return paths;
        }

        for line in text.split('\n') {
            if let Some(value) = line.strip_prefix("WHELK_ACTIVE_WINDOW=") {
                if let Some(value) = parse_u64_base0(value) {
                    paths.window_id = value as u32;
                }
            } else if line.starts_with("_GTK_UNIQUE_BUS_NAME(") {
                paths.bus_name = xprop_quoted_value(line, 0);
            } else if line.starts_with("_GTK_APPLICATION_OBJECT_PATH(") {
                paths.app_path = xprop_quoted_value(line, 0);
            } else if line.starts_with("_GTK_WINDOW_OBJECT_PATH(") {
                paths.window_path = xprop_quoted_value(line, 0);
            } else if line.starts_with("_GTK_MENUBAR_OBJECT_PATH(") {
                paths.menubar_path = xprop_quoted_value(line, 0);
            } else if line.starts_with("_GTK_APP_MENU_OBJECT_PATH(") {
                paths.appmenu_path = xprop_quoted_value(line, 0);
            }
        }

        paths
    }
}

fn parse_niri(text: &str) -> Option<FocusedWindow> {
    let root = Json::parse(text).ok()?;
    let title = root.get_str("title");
    let app_id = root.get_str("app_id");

    if title.is_none() && app_id.is_none() {
        return None;
    }

    Some(FocusedWindow {
        title: title.unwrap_or("").to_owned(),
        app_id: app_id.unwrap_or("").to_owned(),
    })
}

/// `hyprctl activewindow -j' prints one object for the focused window.  The
/// class is the app id; initialClass is what it was at launch, which is steadier
/// for matching than a class an application can change at run time.
fn parse_hyprland(text: &str) -> Option<FocusedWindow> {
    let root = Json::parse(text).ok()?;
    let title = root.get_str("title");
    let app_id = root
        .get_str("initialClass")
        .or_else(|| root.get_str("class"));

    if title.is_none() && app_id.is_none() {
        return None;
    }

    Some(FocusedWindow {
        title: title.unwrap_or("").to_owned(),
        app_id: app_id.unwrap_or("").to_owned(),
    })
}

fn parse_sway(text: &str) -> Option<FocusedWindow> {
    let root = Json::parse(text).ok()?;

    walk_sway(&root)
}

fn walk_sway(node: &Json) -> Option<FocusedWindow> {
    if node.get_bool("focused", false) {
        let name = node.get_str("name");
        let app = node.get_str("app_id");
        let class = node
            .get("window_properties")
            .and_then(|properties| properties.get_str("class"));

        if name.is_some() || app.is_some() || class.is_some() {
            return Some(FocusedWindow {
                title: name.unwrap_or("").to_owned(),
                app_id: app
                    .filter(|app| !app.is_empty())
                    .or(class)
                    .unwrap_or("")
                    .to_owned(),
            });
        }
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(Json::array) {
            for child in children {
                if let Some(found) = walk_sway(child) {
                    return Some(found);
                }
            }
        }
    }

    None
}

/// The river-status focused_view event carries the title and nothing else.  A
/// title is still the answer the window module needs; an empty title means no
/// view is focused, which is the same "nothing to show" as a missing window.
fn parse_river(text: &str) -> Option<FocusedWindow> {
    let clean = text.trim_end_matches(|character: char| character.is_ascii_whitespace());

    if clean.is_empty() {
        return None;
    }

    Some(FocusedWindow {
        title: clean.to_owned(),
        app_id: String::new(),
    })
}

fn parse_x11(text: &str) -> Option<FocusedWindow> {
    let mut utf8_name = None;
    let mut plain_name = None;
    let mut class_name = None;
    let mut instance = None;

    for line in text.split('\n') {
        if line.starts_with("WM_NAME(UTF8_STRING)") {
            utf8_name = xprop_quoted_value(line, 0);
        } else if line.starts_with("WM_NAME(STRING)") || line.starts_with("WM_NAME(") {
            plain_name = xprop_quoted_value(line, 0);
        } else if line.starts_with("WM_CLASS") {
            instance = xprop_quoted_value(line, 0);
            class_name = xprop_quoted_value(line, 1);
        }
    }

    let title = utf8_name.or(plain_name);
    let app_id = class_name.or(instance);

    if title.is_none() && app_id.is_none() {
        return None;
    }

    Some(FocusedWindow {
        title: title.unwrap_or_default(),
        app_id: app_id.unwrap_or_default(),
    })
}

/// `"WM_CLASS(STRING) = \"navigator\", \"Firefox\""` -> the requested quoted
/// value (Firefox for index 1).
fn xprop_quoted_value(line: &str, index: usize) -> Option<String> {
    let mut rest = line;

    for seen in 0..=index {
        let start = rest.find('"')?;
        let after = &rest[start + 1..];
        let end = after.find('"')?;

        if seen == index {
            return Some(after[..end].to_owned());
        }

        rest = &after[end + 1..];
    }

    None
}

fn sway_focused_pid(node: &Json) -> i32 {
    if node.get_bool("focused", false) {
        return node.get_number("pid", 0.0) as i32;
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(Json::array) {
            for child in children {
                let pid = sway_focused_pid(child);

                if pid > 0 {
                    return pid;
                }
            }
        }
    }

    0
}

fn parse_i64_prefix(text: &str) -> i64 {
    let text = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits: String = text
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        return 0;
    }

    let value = digits.parse::<i64>().unwrap_or(i64::MAX);

    if negative { -value } else { value }
}

fn parse_u64_base0(text: &str) -> Option<u64> {
    let text = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let text = text.strip_prefix('+').unwrap_or(text);

    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        let digits: String = hex
            .chars()
            .take_while(|character| character.is_ascii_hexdigit())
            .collect();

        if digits.is_empty() {
            return None;
        }

        u64::from_str_radix(&digits, 16).ok()
    } else if let Some(octal) = text.strip_prefix('0') {
        let digits: String = octal
            .chars()
            .take_while(|character| matches!(character, '0'..='7'))
            .collect();

        if digits.is_empty() {
            Some(0)
        } else {
            u64::from_str_radix(&digits, 8).ok()
        }
    } else {
        let digits: String = text
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();

        if digits.is_empty() {
            None
        } else {
            digits.parse().ok()
        }
    }
}

/// The first executable named `name` on `PATH`, as an owned absolute path.
/// A menu's shortcut tool and an explicitly configured `#:shortcut-tool` both
/// use this check, so a missing tool becomes an absent row rather than a click
/// that does nothing.
pub fn program_in_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;

    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
        .map(|candidate| candidate.to_string_lossy().into_owned())
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

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! `whelk --status`: a read-only account of what the shell can see.
//!
//! The question behind it is "why is the bar blank where I expected a
//! reading", and the three answers are almost always the same three: this
//! session is not the one the module speaks to, the program it shells out to is
//! not on `PATH`, or the system bus has no `login1`.  The report says all three
//! once and then goes module by module, so a missing `nmcli` is one line rather
//! than a card that quietly stays empty.
//!
//! Nothing here changes the shell: no D-Bus call that is not a read, no timer,
//! no window.  It is safe to run from a script, an ssh session or a bug report,
//! which is why `--json` exists.

use std::path::{Path, PathBuf};

use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::bar::BarSettings;
use crate::barfactory;
use crate::logind::LOGIND_NAME;
use crate::session::{self, Detection, Session};

/// The programs the shell reaches for at run time, and the alternatives that
/// stand in for each other.
pub const TOOLS: &[(&str, &[&str])] = &[
    ("curl", &["curl"]),
    ("wtype", &["wtype"]),
    ("wl-copy", &["wl-copy"]),
    ("nmcli", &["nmcli"]),
    ("bluetoothctl", &["bluetoothctl"]),
    ("wpctl/pactl", &["wpctl", "pactl"]),
    ("brightnessctl", &["brightnessctl"]),
    (
        "systemd-inhibit/elogind-inhibit",
        &["systemd-inhibit", "elogind-inhibit"],
    ),
];

/// The report, already decided: the live probes are in [`collect`], and every
/// string here is what the two renderers print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub version: String,
    pub config: String,
    pub session: SessionReport,
    pub logind: LogindReport,
    pub tools: Vec<ToolReport>,
    pub modules: Vec<ModuleReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    pub session: Session,
    /// The variable that named the session, or the probe that found it.
    pub evidence: Option<String>,
    pub session_type: Option<String>,
    pub desktop: Option<String>,
    pub wayland_display: Option<String>,
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogindReport {
    /// `systemd-logind` owns `org.freedesktop.login1`.
    Systemd { process: String, pid: u32 },
    /// `elogind` owns it: the same interface, without systemd.
    Elogind { process: String, pid: u32 },
    /// Something owns the name, but it is neither daemon whelk knows.
    Other { process: String, pid: u32 },
    /// The bus or the name could not be reached, with the reason.
    Unreachable(String),
}

impl LogindReport {
    /// Whether a caller may expect `org.freedesktop.login1` to answer.
    pub fn answers(&self) -> bool {
        !matches!(self, Self::Unreachable(_))
    }

    pub fn owner(&self) -> &'static str {
        match self {
            Self::Systemd { .. } => "systemd",
            Self::Elogind { .. } => "elogind",
            Self::Other { .. } => "other",
            Self::Unreachable(_) => "unreachable",
        }
    }
}

/// One program (or one group of alternatives) and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReport {
    pub label: &'static str,
    pub candidates: &'static [&'static str],
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleReport {
    pub bar: String,
    /// `left`, `center`, `right` or `overflow`.
    pub section: &'static str,
    pub kind: String,
    pub state: ModuleState,
}

/// What the check could say about one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleState {
    /// Everything this check can see is there; the note names what it found.
    Ok(String),
    /// A precondition is missing, and the note is the reason the reading stays
    /// blank.
    Blank(String),
    /// A live service would have to be asked (a player, a tray host); the note
    /// says which, because a read-only report must not pretend to know.
    Unchecked(String),
}

impl ModuleState {
    pub fn verdict(&self) -> &'static str {
        match self {
            Self::Ok(_) => "ok",
            Self::Blank(_) => "blank",
            Self::Unchecked(_) => "unchecked",
        }
    }

    pub fn note(&self) -> &str {
        match self {
            Self::Ok(note) | Self::Blank(note) | Self::Unchecked(note) => note,
        }
    }
}

/// The facts a module verdict is decided from.  Everything is a closure or a
/// plain value so the policy can be tested without a PATH, a bus or a /sys.
pub struct Environment<'a> {
    pub session: Session,
    pub logind: bool,
    pub program: &'a dyn Fn(&str) -> Option<String>,
    pub path_exists: &'a dyn Fn(&str) -> bool,
}

/// What one kind of module needs before it can show a reading.
struct Need {
    /// Any one of these programs has to be on `PATH`.
    programs: &'static [&'static str],
    /// A filesystem path the module reads.
    path: Option<&'static str>,
    /// The session has to be one of these.
    sessions: &'static [Session],
    /// `org.freedesktop.login1` has to answer on the system bus.
    logind: bool,
    /// A bus service that only a live query could confirm.
    bus: Option<&'static str>,
    /// What the module shows when it needs nothing in particular.
    note: &'static str,
}

const fn nothing(note: &'static str) -> Need {
    Need {
        programs: &[],
        path: None,
        sessions: &[],
        logind: false,
        bus: None,
        note,
    }
}

const fn program(programs: &'static [&'static str]) -> Need {
    Need {
        programs,
        ..nothing("")
    }
}

const fn reads(path: &'static str) -> Need {
    Need {
        path: Some(path),
        ..nothing("")
    }
}

const fn session(sessions: &'static [Session]) -> Need {
    Need {
        sessions,
        ..nothing("")
    }
}

const fn logind() -> Need {
    Need {
        logind: true,
        ..nothing("")
    }
}

const fn service(note: &'static str) -> Need {
    Need {
        bus: Some(note),
        ..nothing("")
    }
}

/// The precondition table.  One row per kind in `barfactory`, so a new module
/// has to say what it needs here (a test keeps the two lists the same length).
fn need_for(kind: &str) -> Option<Need> {
    Some(match kind {
        "audio" => program(&["wpctl", "pactl"]),
        "backlight" => reads("/sys/class/backlight"),
        "battery" => reads("/sys/class/power_supply"),
        "bluetooth" => program(&["bluetoothctl"]),
        "character" => nothing("the character picker"),
        "clock" => nothing("the local time"),
        "clipboard" => nothing("whelk's clipboard history"),
        "cpu" => reads("/proc/stat"),
        "custom" => nothing("its #:exec command"),
        "dnd" => nothing("whelk's do-not-disturb state"),
        "inhibit" => logind(),
        "keyboard" => reads("/sys/class/leds"),
        "language" => session(&[Session::Niri]),
        "memory" => reads("/proc/meminfo"),
        "menu" => nothing("the focused application's menus"),
        "mpris" => service("an org.mpris.MediaPlayer2 player on the session bus"),
        "notifications" => nothing("whelk's notification store"),
        "power-profile" => program(&["powerprofilesctl"]),
        "scheme" => nothing("its Guile procedure's widget"),
        "start" => nothing("the shell menu"),
        "tasks" => session(&[
            Session::Niri,
            Session::Sway,
            Session::Hyprland,
            Session::X11,
        ]),
        "tray" => service("an org.kde.StatusNotifierWatcher host on the session bus"),
        "wallpaper" => nothing("the wallpaper menu"),
        "wifi" => program(&["nmcli"]),
        "window" => session(&[
            Session::Niri,
            Session::Sway,
            Session::Hyprland,
            Session::River,
            Session::X11,
        ]),
        "workspaces" => session(&[Session::Niri, Session::Hyprland]),
        _ => return None,
    })
}

/// Why this module is blank, or what it found instead.
pub fn judge(kind: &str, environment: &Environment) -> ModuleState {
    let Some(need) = need_for(kind) else {
        return ModuleState::Blank(format!("unknown module kind '{kind}'"));
    };

    if !need.sessions.is_empty() && !need.sessions.contains(&environment.session) {
        let names: Vec<&str> = need.sessions.iter().map(|session| session.name()).collect();

        return ModuleState::Blank(format!(
            "{} only; this session is {}",
            names.join(" or "),
            environment.session.name()
        ));
    }

    if need.logind && !environment.logind {
        return ModuleState::Blank(format!("{LOGIND_NAME} is not on the system bus"));
    }

    if let Some(path) = need.path
        && !(environment.path_exists)(path)
    {
        return ModuleState::Blank(format!("{path} is not there"));
    }

    if !need.programs.is_empty() {
        return match need
            .programs
            .iter()
            .find_map(|name| (environment.program)(name).map(|path| format!("{name} at {path}")))
        {
            Some(found) => ModuleState::Ok(found),
            None => ModuleState::Blank(format!("no {} on PATH", need.programs.join(" or "))),
        };
    }

    if let Some(service) = need.bus {
        return ModuleState::Unchecked(format!("needs {service}"));
    }

    if !need.sessions.is_empty() {
        return ModuleState::Ok(format!("{} session", environment.session.name()));
    }

    if need.logind {
        return ModuleState::Ok(format!("{LOGIND_NAME} answered"));
    }

    if let Some(path) = need.path {
        return ModuleState::Ok(format!("reads {path}"));
    }

    ModuleState::Ok(need.note.to_owned())
}

/// Decide the whole report from live probes.
pub fn collect(bars: &[BarSettings], config: &Path) -> Report {
    let detection =
        Detection::from_environment().probe(|command| session::command_answers(command, 400));
    let logind = logind_report();
    let tools = TOOLS
        .iter()
        .map(|(label, candidates)| ToolReport {
            label,
            candidates,
            path: candidates
                .iter()
                .find_map(|name| session::program_in_path(name)),
        })
        .collect();
    let environment = Environment {
        session: detection.session,
        logind: logind.answers(),
        program: &|name| session::program_in_path(name),
        path_exists: &|path| Path::new(path).exists(),
    };
    let mut modules = Vec::new();

    for bar in bars {
        for (section, configured) in [
            ("left", &bar.left),
            ("center", &bar.center),
            ("right", &bar.right),
            ("overflow", &bar.overflow),
        ] {
            for module in configured {
                modules.push(ModuleReport {
                    bar: bar.name.clone(),
                    section,
                    kind: module.kind.clone(),
                    state: judge(&module.kind, &environment),
                });
            }
        }
    }

    Report {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        config: config.display().to_string(),
        session: SessionReport {
            session: detection.session,
            evidence: detection.evidence.map(|evidence| evidence.to_owned()),
            session_type: std::env::var("XDG_SESSION_TYPE").ok(),
            desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            display: std::env::var("DISPLAY").ok(),
        },
        logind,
        tools,
        modules,
    }
}

/// Who owns `org.freedesktop.login1`, asked of the bus and then of `/proc`.
///
/// `systemd-logind` and `elogind` publish the same interface, so the name alone
/// does not say which; the process behind the name does.  A machine with no
/// system bus, or no logind, is a fact worth printing rather than an error.
fn logind_report() -> LogindReport {
    let connection = match gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        Ok(connection) => connection,
        Err(error) => return LogindReport::Unreachable(format!("no system bus ({error})")),
    };
    let owner = match call_string(&connection, "GetNameOwner", (LOGIND_NAME,).to_variant()) {
        Ok(owner) => owner,
        Err(error) => {
            return LogindReport::Unreachable(format!("{LOGIND_NAME} has no owner ({error})"));
        }
    };
    let pid = match connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            Some(&(owner.as_str(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2_000,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|reply| reply.get::<(u32,)>())
    {
        Some((pid,)) => pid,
        None => return LogindReport::Unreachable("the owner publishes no process id".to_owned()),
    };

    owner_from_process(&read_process_name(pid), pid)
}

fn call_string(
    connection: &gio::DBusConnection,
    method: &str,
    parameters: gtk::glib::Variant,
) -> Result<String, String> {
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            2_000,
            gio::Cancellable::NONE,
        )
        .map_err(|error| error.message().to_owned())
        .and_then(|reply| {
            reply
                .get::<(String,)>()
                .map(|(name,)| name)
                .ok_or_else(|| "the reply was not a bus name".to_owned())
        })
}

/// The daemon's `comm`, which is `systemd-logind` on systemd and `elogind` on
/// an elogind machine.
fn read_process_name(pid: u32) -> String {
    std::fs::read_to_string(PathBuf::from(format!("/proc/{pid}/comm")))
        .map(|name| name.trim().to_owned())
        .unwrap_or_default()
}

/// The names `systemd-logind` and `elogind` are told apart by.
pub fn owner_from_process(process: &str, pid: u32) -> LogindReport {
    let process = process.to_owned();

    if process.contains("elogind") {
        LogindReport::Elogind { process, pid }
    } else if process.contains("systemd") {
        LogindReport::Systemd { process, pid }
    } else if process.is_empty() {
        LogindReport::Unreachable(format!("the owner (pid {pid}) has no name"))
    } else {
        LogindReport::Other { process, pid }
    }
}

impl Report {
    /// The report as a person reads it.
    pub fn text(&self) -> String {
        let mut output = format!("whelk {}\nconfig: {}\n", self.version, self.config);

        output.push_str(&format!("session: {}", self.session_line()));
        output.push('\n');

        if let Some(environment) = self.environment_line() {
            output.push_str(&format!("environment: {environment}\n"));
        }

        output.push_str(&format!("logind: {}\n", self.logind_line()));
        output.push_str("tools:\n");

        let width = self
            .tools
            .iter()
            .map(|tool| tool.label.chars().count())
            .max()
            .unwrap_or(0);

        for tool in &self.tools {
            match &tool.path {
                Some(path) => output.push_str(&format!(
                    "  {:<width$}  {path}\n",
                    tool.label,
                    width = width
                )),
                None => output.push_str(&format!(
                    "  {:<width$}  missing ({})\n",
                    tool.label,
                    tool.candidates.join(" or "),
                    width = width
                )),
            }
        }

        output.push_str("modules:\n");

        for module in &self.modules {
            output.push_str(&format!(
                "  {}/{}/{}  {}: {}\n",
                module.bar,
                module.section,
                module.kind,
                module.state.verdict(),
                module.state.note()
            ));
        }

        output
    }

    fn session_line(&self) -> String {
        match &self.session.evidence {
            Some(evidence) => format!("{} ({})", self.session.session.name(), evidence),
            None => format!(
                "{} (no compositor answered; ways it can be blank below)",
                self.session.session.name()
            ),
        }
    }

    fn environment_line(&self) -> Option<String> {
        let mut parts = Vec::new();

        if let Some(value) = &self.session.session_type {
            parts.push(format!("XDG_SESSION_TYPE={value}"));
        }

        if let Some(value) = &self.session.desktop {
            parts.push(format!("XDG_CURRENT_DESKTOP={value}"));
        }

        if let Some(value) = &self.session.wayland_display {
            parts.push(format!("WAYLAND_DISPLAY={value}"));
        }

        if let Some(value) = &self.session.display {
            parts.push(format!("DISPLAY={value}"));
        }

        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    fn logind_line(&self) -> String {
        match &self.logind {
            LogindReport::Systemd { process, pid } => format!("{process} (pid {pid})"),
            LogindReport::Elogind { process, pid } => format!("{process} (pid {pid})"),
            LogindReport::Other { process, pid } => {
                format!("{process} (pid {pid}; neither systemd nor elogind)")
            }
            LogindReport::Unreachable(reason) => format!("unreachable ({reason})"),
        }
    }

    /// The report as a program reads it.
    pub fn json(&self) -> String {
        let mut output = String::from("{\n");

        field(&mut output, 1, "version", &json_string(&self.version), true);
        field(&mut output, 1, "config", &json_string(&self.config), true);
        output.push_str("  \"session\": {\n");
        field(
            &mut output,
            2,
            "name",
            &json_string(self.session.session.name()),
            true,
        );
        field(
            &mut output,
            2,
            "evidence",
            &json_option(self.session.evidence.as_deref()),
            true,
        );
        field(
            &mut output,
            2,
            "type",
            &json_option(self.session.session_type.as_deref()),
            true,
        );
        field(
            &mut output,
            2,
            "desktop",
            &json_option(self.session.desktop.as_deref()),
            true,
        );
        field(
            &mut output,
            2,
            "wayland_display",
            &json_option(self.session.wayland_display.as_deref()),
            true,
        );
        field(
            &mut output,
            2,
            "display",
            &json_option(self.session.display.as_deref()),
            false,
        );
        output.push_str("  },\n");

        output.push_str("  \"logind\": {\n");
        field(
            &mut output,
            2,
            "owner",
            &json_string(self.logind.owner()),
            true,
        );

        match &self.logind {
            LogindReport::Systemd { process, pid }
            | LogindReport::Elogind { process, pid }
            | LogindReport::Other { process, pid } => {
                field(&mut output, 2, "process", &json_string(process), true);
                field(&mut output, 2, "pid", &pid.to_string(), false);
            }
            LogindReport::Unreachable(reason) => {
                field(&mut output, 2, "reason", &json_string(reason), false);
            }
        }

        output.push_str("  },\n");
        output.push_str("  \"tools\": [\n");

        for (index, tool) in self.tools.iter().enumerate() {
            output.push_str("    {\n");
            field(&mut output, 3, "name", &json_string(tool.label), true);
            field(
                &mut output,
                3,
                "candidates",
                &json_array(tool.candidates),
                true,
            );
            field(
                &mut output,
                3,
                "path",
                &json_option(tool.path.as_deref()),
                false,
            );
            output.push_str(if index + 1 == self.tools.len() {
                "    }\n"
            } else {
                "    },\n"
            });
        }

        output.push_str("  ],\n");
        output.push_str("  \"modules\": [\n");

        for (index, module) in self.modules.iter().enumerate() {
            output.push_str("    {\n");
            field(&mut output, 3, "bar", &json_string(&module.bar), true);
            field(
                &mut output,
                3,
                "section",
                &json_string(module.section),
                true,
            );
            field(&mut output, 3, "kind", &json_string(&module.kind), true);
            field(
                &mut output,
                3,
                "state",
                &json_string(module.state.verdict()),
                true,
            );
            field(
                &mut output,
                3,
                "note",
                &json_string(module.state.note()),
                false,
            );
            output.push_str(if index + 1 == self.modules.len() {
                "    }\n"
            } else {
                "    },\n"
            });
        }

        output.push_str("  ]\n}\n");

        output
    }
}

fn field(output: &mut String, depth: usize, name: &str, value: &str, comma: bool) {
    let indent = "  ".repeat(depth);

    output.push_str(&format!(
        "{indent}\"{name}\": {value}{}\n",
        if comma { "," } else { "" }
    ));
}

fn json_array(values: &[&str]) -> String {
    let items: Vec<String> = values.iter().map(|value| json_string(value)).collect();

    format!("[{}]", items.join(", "))
}

fn json_option(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

/// A JSON string: the two mandatory escapes, the shorthands, and `\u00XX` for
/// the other control characters.
pub fn json_string(value: &str) -> String {
    let mut output = String::from("\"");

    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }

    output.push('"');

    output
}

/// The kinds the status report knows how to judge.  Exposed so the test that
/// compares it with the factory table does not have to duplicate the list.
pub fn known_kinds() -> impl Iterator<Item = &'static str> {
    barfactory::kinds()
}

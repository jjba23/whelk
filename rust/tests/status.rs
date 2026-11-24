// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! `whelk --status`: the module verdicts, the two renderings and the logind
//! owner, all decided from facts the test supplies.

use whelk_rs::json::Json;
use whelk_rs::session::Session;
use whelk_rs::status::{
    self, Environment, LogindReport, ModuleReport, ModuleState, Report, SessionReport, ToolReport,
};

/// An environment where nothing is on PATH and nothing is in /sys.
fn bare_environment<'a>(
    session: Session,
    program: &'a dyn Fn(&str) -> Option<String>,
    path_exists: &'a dyn Fn(&str) -> bool,
) -> Environment<'a> {
    Environment {
        session,
        logind: false,
        program,
        path_exists,
    }
}

fn judge_in(session: Session, logind: bool, kind: &str) -> ModuleState {
    let program = |_name: &str| None;
    let exists = |_path: &str| false;
    let mut environment = bare_environment(session, &program, &exists);
    environment.logind = logind;

    status::judge(kind, &environment)
}

#[test]
fn a_missing_program_is_the_blank_reason() {
    assert_eq!(
        judge_in(Session::Niri, true, "wifi"),
        ModuleState::Blank("no nmcli on PATH".to_owned())
    );
    assert_eq!(
        judge_in(Session::Niri, true, "audio"),
        ModuleState::Blank("no wpctl or pactl on PATH".to_owned())
    );
    assert_eq!(
        judge_in(Session::Niri, true, "power-profile"),
        ModuleState::Blank("no powerprofilesctl on PATH".to_owned())
    );
}

#[test]
fn a_program_on_path_is_named_with_where_it_was_found() {
    let program = |name: &str| (name == "nmcli").then(|| "/usr/bin/nmcli".to_owned());
    let exists = |_path: &str| false;
    let environment = bare_environment(Session::Niri, &program, &exists);

    assert_eq!(
        status::judge("wifi", &environment),
        ModuleState::Ok("nmcli at /usr/bin/nmcli".to_owned())
    );
}

#[test]
fn a_module_for_another_compositor_says_which_sessions_it_has() {
    assert_eq!(
        judge_in(Session::Sway, true, "workspaces"),
        ModuleState::Blank("niri or hyprland only; this session is sway".to_owned())
    );
    assert_eq!(
        judge_in(Session::Niri, true, "language"),
        ModuleState::Ok("niri session".to_owned())
    );
    assert_eq!(
        judge_in(Session::Wayland, true, "window"),
        ModuleState::Blank(
            "niri or sway or hyprland or river or x11 only; this session is wayland".to_owned()
        )
    );
}

#[test]
fn a_missing_sysfs_path_is_the_blank_reason() {
    assert_eq!(
        judge_in(Session::Niri, true, "battery"),
        ModuleState::Blank("/sys/class/power_supply is not there".to_owned())
    );

    let program = |_name: &str| None;
    let exists = |path: &str| path == "/proc/stat";
    let environment = bare_environment(Session::Niri, &program, &exists);

    assert_eq!(
        status::judge("cpu", &environment),
        ModuleState::Ok("reads /proc/stat".to_owned())
    );
}

#[test]
fn inhibit_needs_logind() {
    assert_eq!(
        judge_in(Session::Niri, false, "inhibit"),
        ModuleState::Blank("org.freedesktop.login1 is not on the system bus".to_owned())
    );
    assert_eq!(
        judge_in(Session::Niri, true, "inhibit"),
        ModuleState::Ok("org.freedesktop.login1 answered".to_owned())
    );
}

#[test]
fn live_services_are_named_rather_than_guessed() {
    assert_eq!(
        judge_in(Session::Niri, true, "mpris"),
        ModuleState::Unchecked(
            "needs an org.mpris.MediaPlayer2 player on the session bus".to_owned()
        )
    );
    assert_eq!(
        judge_in(Session::Niri, true, "tray"),
        ModuleState::Unchecked(
            "needs an org.kde.StatusNotifierWatcher host on the session bus".to_owned()
        )
    );
}

#[test]
fn every_factory_kind_has_a_verdict() {
    let mut seen = 0;

    for kind in status::known_kinds() {
        seen += 1;

        let state = judge_in(Session::Niri, true, kind);

        assert!(
            !matches!(&state, ModuleState::Blank(reason) if reason.contains("unknown module kind")),
            "{kind} has no status entry"
        );
    }

    assert_eq!(seen, 27, "the factory table changed size");
}

#[test]
fn an_unknown_kind_is_not_a_silent_pass() {
    assert_eq!(
        judge_in(Session::Niri, true, "weatherbar"),
        ModuleState::Blank("unknown module kind 'weatherbar'".to_owned())
    );
}

#[test]
fn the_logind_process_name_says_which_daemon_owns_the_name() {
    assert_eq!(
        status::owner_from_process("systemd-logind", 1336),
        LogindReport::Systemd {
            process: "systemd-logind".to_owned(),
            pid: 1336
        }
    );
    assert_eq!(
        status::owner_from_process("elogind", 412),
        LogindReport::Elogind {
            process: "elogind".to_owned(),
            pid: 412
        }
    );
    assert_eq!(
        status::owner_from_process("mock-logind", 7),
        LogindReport::Other {
            process: "mock-logind".to_owned(),
            pid: 7
        }
    );
    assert!(matches!(
        status::owner_from_process("", 7),
        LogindReport::Unreachable(_)
    ));
    assert!(
        LogindReport::Systemd {
            process: "systemd-logind".to_owned(),
            pid: 1
        }
        .answers()
    );
    assert!(!LogindReport::Unreachable("no system bus".to_owned()).answers());
}

fn example_report() -> Report {
    Report {
        version: "0.46.0".to_owned(),
        config: "/home/joe/.config/whelk/settings.scm".to_owned(),
        session: SessionReport {
            session: Session::Niri,
            evidence: Some("NIRI_SOCKET".to_owned()),
            session_type: Some("wayland".to_owned()),
            desktop: Some("niri".to_owned()),
            wayland_display: Some("wayland-1".to_owned()),
            display: None,
        },
        logind: LogindReport::Systemd {
            process: "systemd-logind".to_owned(),
            pid: 1336,
        },
        tools: vec![
            ToolReport {
                label: "curl",
                candidates: &["curl"],
                path: Some("/usr/bin/curl".to_owned()),
            },
            ToolReport {
                label: "wtype",
                candidates: &["wtype"],
                path: None,
            },
        ],
        modules: vec![
            ModuleReport {
                bar: "main".to_owned(),
                section: "left",
                kind: "clock".to_owned(),
                state: ModuleState::Ok("the local time".to_owned()),
            },
            ModuleReport {
                bar: "main".to_owned(),
                section: "right",
                kind: "wifi".to_owned(),
                state: ModuleState::Blank("no nmcli on PATH".to_owned()),
            },
        ],
    }
}

#[test]
fn the_text_report_names_each_fact_on_its_own_line() {
    let text = example_report().text();

    assert!(text.starts_with("whelk 0.46.0\n"), "{text}");
    assert!(
        text.contains("config: /home/joe/.config/whelk/settings.scm\n"),
        "{text}"
    );
    assert!(text.contains("session: niri (NIRI_SOCKET)\n"), "{text}");
    assert!(
        text.contains("environment: XDG_SESSION_TYPE=wayland · XDG_CURRENT_DESKTOP=niri · WAYLAND_DISPLAY=wayland-1\n"),
        "{text}"
    );
    assert!(
        text.contains("logind: systemd-logind (pid 1336)\n"),
        "{text}"
    );
    assert!(text.contains("  curl   /usr/bin/curl\n"), "{text}");
    assert!(text.contains("  wtype  missing (wtype)\n"), "{text}");
    assert!(
        text.contains("  main/left/clock  ok: the local time\n"),
        "{text}"
    );
    assert!(
        text.contains("  main/right/wifi  blank: no nmcli on PATH\n"),
        "{text}"
    );
}

#[test]
fn the_json_report_parses_and_carries_the_same_facts() {
    let text = example_report().json();
    let json = Json::parse(&text).expect("status json parses");

    assert_eq!(json.get_str("version"), Some("0.46.0"));
    assert_eq!(
        json.get("session")
            .and_then(|session| session.get_str("name")),
        Some("niri")
    );
    assert_eq!(
        json.get("session")
            .and_then(|session| session.get_str("evidence")),
        Some("NIRI_SOCKET")
    );
    assert_eq!(
        json.get("logind")
            .and_then(|logind| logind.get_str("owner")),
        Some("systemd")
    );
    assert_eq!(
        json.get("logind")
            .and_then(|logind| logind.get_str("process")),
        Some("systemd-logind")
    );

    let tools = json
        .get("tools")
        .and_then(|tools| tools.array())
        .expect("tools array");

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].get_str("name"), Some("curl"));
    assert_eq!(tools[0].get_str("path"), Some("/usr/bin/curl"));
    assert_eq!(tools[1].get("path").map(Json::is_null), Some(true));
    assert_eq!(
        tools[0]
            .get("candidates")
            .and_then(|candidates| candidates.array())
            .map(|candidates| candidates.len()),
        Some(1)
    );

    let modules = json
        .get("modules")
        .and_then(|modules| modules.array())
        .expect("modules array");

    assert_eq!(modules.len(), 2);
    assert_eq!(modules[1].get_str("kind"), Some("wifi"));
    assert_eq!(modules[1].get_str("state"), Some("blank"));
    assert_eq!(modules[1].get_str("note"), Some("no nmcli on PATH"));
}

#[test]
fn json_strings_escape_quotes_controls_and_backslashes() {
    assert_eq!(status::json_string("plain"), "\"plain\"");
    assert_eq!(status::json_string("a\"b"), "\"a\\\"b\"");
    assert_eq!(status::json_string("a\\b"), "\"a\\\\b\"");
    assert_eq!(status::json_string("a\nb\tc"), "\"a\\nb\\tc\"");
    assert_eq!(status::json_string("\u{7}"), "\"\\u0007\"");

    let mut report = example_report();

    report.modules[0].state = ModuleState::Blank("its \"#:exec\" ran\naway".to_owned());

    let json = Json::parse(&report.json()).expect("escaped status json parses");

    assert_eq!(
        json.get("modules")
            .and_then(|modules| modules.array())
            .and_then(|modules| modules[0].get_str("note")),
        Some("its \"#:exec\" ran\naway")
    );
}

#[test]
fn a_session_with_no_evidence_says_so() {
    let mut report = example_report();

    report.session.evidence = None;
    report.session.session_type = None;
    report.session.desktop = None;
    report.session.wayland_display = None;

    let text = report.text();

    assert!(text.contains("no compositor answered"), "{text}");
    assert!(!text.contains("environment:"), "{text}");
}

#[test]
fn an_unreachable_logind_is_a_reason_not_an_empty_line() {
    let mut report = example_report();

    report.logind = LogindReport::Unreachable("no system bus".to_owned());

    assert!(
        report
            .text()
            .contains("logind: unreachable (no system bus)"),
        "{}",
        report.text()
    );

    let json = Json::parse(&report.json()).expect("status json parses");

    assert_eq!(
        json.get("logind")
            .and_then(|logind| logind.get_str("owner")),
        Some("unreachable")
    );
    assert_eq!(
        json.get("logind")
            .and_then(|logind| logind.get_str("reason")),
        Some("no system bus")
    );
}

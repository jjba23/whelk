// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::time::{Duration, Instant};

use whelk_rs::session::{Detection, Session, command_answers};

#[test]
fn environment_precedence() {
    assert_eq!(
        Session::from_values(
            Some("/niri"),
            Some("/sway"),
            Some("hypr"),
            None,
            None,
            None,
            None
        ),
        Session::Niri
    );
    assert_eq!(
        Session::from_values(None, Some("/sway"), Some("hypr"), None, None, None, None),
        Session::Sway
    );
    assert_eq!(
        Session::from_values(None, None, Some("hypr"), Some("/river"), None, None, None),
        Session::Hyprland
    );
    assert_eq!(
        Session::from_values(None, None, None, Some("/river"), None, None, None),
        Session::River
    );
}

#[test]
fn x11_guesses() {
    assert_eq!(
        Session::from_values(None, None, None, None, Some("x11"), Some(":0"), None),
        Session::X11
    );
    assert_eq!(
        Session::from_values(None, None, None, None, Some("X11"), Some(":0"), None),
        Session::X11
    );
    assert_eq!(
        Session::from_values(None, None, None, None, None, Some(":0"), None),
        Session::X11
    );
    // A Wayland display wins over a leftover DISPLAY.
    assert_eq!(
        Session::from_values(None, None, None, None, None, Some(":0"), Some("wayland-0")),
        Session::Wayland
    );
}

#[test]
fn empty_variables_do_not_count() {
    assert_eq!(
        Session::from_values(Some(""), Some(""), Some(""), Some(""), None, None, None),
        Session::Wayland
    );
    // Only an empty string is absent; a whitespace value is still set, as in
    // the C reader (`*niri` is true for a space).
    assert_eq!(
        Session::from_values(None, Some(" "), None, None, None, None, None),
        Session::Sway
    );
}

#[test]
fn names() {
    assert_eq!(Session::Niri.name(), "niri");
    assert_eq!(Session::Sway.name(), "sway");
    assert_eq!(Session::Hyprland.name(), "hyprland");
    assert_eq!(Session::River.name(), "river");
    assert_eq!(Session::X11.name(), "x11");
    assert_eq!(Session::Wayland.name(), "wayland");
}

#[test]
fn environment_evidence() {
    assert_eq!(
        Detection::from_values(Some("/niri"), None, None, None, None, None, None),
        Detection {
            session: Session::Niri,
            evidence: Some("NIRI_SOCKET"),
        }
    );
    assert_eq!(
        Detection::from_values(None, Some("/sway"), None, None, None, None, None),
        Detection {
            session: Session::Sway,
            evidence: Some("SWAYSOCK"),
        }
    );
    assert_eq!(
        Detection::from_values(None, None, Some("hypr"), None, None, None, None),
        Detection {
            session: Session::Hyprland,
            evidence: Some("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    );
    assert_eq!(
        Detection::from_values(None, None, None, Some("/river"), None, None, None),
        Detection {
            session: Session::River,
            evidence: Some("RIVER_SOCKET"),
        }
    );
    assert_eq!(
        Detection::from_values(None, None, None, None, Some("x11"), Some(":0"), None),
        Detection {
            session: Session::X11,
            evidence: Some("XDG_SESSION_TYPE=x11"),
        }
    );
    assert_eq!(
        Detection::from_values(None, None, None, None, None, Some(":0"), None),
        Detection {
            session: Session::X11,
            evidence: Some("DISPLAY without WAYLAND_DISPLAY"),
        }
    );
    assert_eq!(
        Detection::from_values(None, None, None, None, None, None, Some("wayland-1")),
        Detection {
            session: Session::Wayland,
            evidence: None,
        }
    );
}

#[test]
fn probe_detection() {
    assert_eq!(
        Session::from_probe(|command| command.starts_with("niri msg")),
        Session::Niri
    );
    assert_eq!(
        Session::from_probe(|command| command.contains("swaymsg")),
        Session::Sway
    );
    assert_eq!(
        Session::from_probe(|command| command == "hyprctl version"),
        Session::Hyprland
    );
    assert_eq!(
        Session::from_probe(|command| command == "riverctl -version"),
        Session::River
    );
    assert_eq!(Session::from_probe(|_| false), Session::Wayland);

    // The first probe that answers wins, in the C table's order.
    assert_eq!(Session::from_probe(|_| true), Session::Niri);

    let probed = Detection {
        session: Session::Wayland,
        evidence: None,
    }
    .probe(|command| command.contains("swaymsg"));

    assert_eq!(probed.session, Session::Sway);
    assert_eq!(probed.evidence, Some("swaymsg -t get_version answered"));
}

#[test]
fn an_environment_answer_skips_the_probe() {
    let detected = Detection {
        session: Session::X11,
        evidence: Some("DISPLAY without WAYLAND_DISPLAY"),
    }
    .probe(|command| panic!("probed {command} despite the environment"));

    assert_eq!(detected.session, Session::X11);
    assert_eq!(detected.evidence, Some("DISPLAY without WAYLAND_DISPLAY"));
}

#[test]
fn probe_commands() {
    let niri = Session::Niri.probe_command().expect("niri probe");
    let sway = Session::Sway.probe_command().expect("sway probe");

    assert!(niri.contains("niri msg -j version"));
    assert!(sway.contains("swaymsg -t get_version"));
    // sway's socket is discovered when the variable is missing, so that the
    // same command works in a session that never exported it.
    assert!(sway.contains("sway-ipc.*.sock"));
    assert!(
        Session::Hyprland
            .probe_command()
            .is_some_and(|command| command.contains("hyprctl version"))
    );
    assert!(
        Session::River
            .probe_command()
            .is_some_and(|command| command.contains("riverctl -version"))
    );
    assert_eq!(Session::X11.probe_command(), None);
    assert_eq!(Session::Wayland.probe_command(), None);
}

#[test]
fn a_command_answers_by_its_exit_status() {
    assert!(command_answers("exit 0", 2000));
    assert!(!command_answers("exit 3", 2000));
    assert!(!command_answers("/nonexistent/probe", 2000));
    assert!(!command_answers("", 2000));
}

#[test]
fn command_times_out() {
    let started = Instant::now();

    assert!(!command_answers("sleep 5", 300));

    // The wait is bounded, so a hung compositor costs the probe's timeout and
    // not the bar's first paint.
    assert!(started.elapsed() < Duration::from_secs(2));
}

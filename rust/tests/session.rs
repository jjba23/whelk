// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::time::{Duration, Instant};

use whelk_rs::session::{Detection, FocusedPaths, FocusedWindow, Session, command_answers};

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

#[test]
fn focused_window_commands() {
    let niri = Session::Niri
        .focused_window_command()
        .expect("niri focused window");
    let sway = Session::Sway
        .focused_window_command()
        .expect("sway focused window");
    let hyprland = Session::Hyprland
        .focused_window_command()
        .expect("hyprland focused window");
    let x11 = Session::X11
        .focused_window_command()
        .expect("x11 focused window");

    assert!(niri.contains("niri msg -j focused-window"));
    assert!(sway.contains("swaymsg -t get_tree"));
    assert!(sway.contains("sway-ipc.*.sock"));
    assert!(hyprland.contains("hyprctl activewindow -j"));
    assert!(x11.contains("xprop"));
    assert!(x11.contains("_NET_ACTIVE_WINDOW"));
    assert!(x11.contains("_GTK_UNIQUE_BUS_NAME"));
    assert!(x11.contains("_GTK_MENUBAR_OBJECT_PATH"));
    assert_eq!(Session::River.focused_window_command(), None);
    assert_eq!(Session::Wayland.focused_window_command(), None);

    let niri_close = Session::Niri.close_window_command().expect("niri close");
    let sway_close = Session::Sway.close_window_command().expect("sway close");
    let hypr_close = Session::Hyprland
        .close_window_command()
        .expect("hyprland close");
    let river_close = Session::River.close_window_command().expect("river close");
    let x11_close = Session::X11.close_window_command().expect("x11 close");

    assert!(niri_close.contains("close-window"));
    assert!(sway_close.contains("kill"));
    assert!(hypr_close.contains("killactive"));
    assert!(river_close.contains("riverctl close"));
    assert!(x11_close.contains("wmctrl"));
    assert_eq!(Session::Wayland.close_window_command(), None);

    let niri_full = Session::Niri
        .fullscreen_window_command()
        .expect("niri fullscreen");
    let sway_full = Session::Sway
        .fullscreen_window_command()
        .expect("sway fullscreen");
    let hypr_full = Session::Hyprland
        .fullscreen_window_command()
        .expect("hyprland fullscreen");
    let river_full = Session::River
        .fullscreen_window_command()
        .expect("river fullscreen");
    let x11_full = Session::X11
        .fullscreen_window_command()
        .expect("x11 fullscreen");

    assert!(niri_full.contains("fullscreen-window"));
    assert!(sway_full.contains("fullscreen toggle"));
    assert!(hypr_full.contains("fullscreen 0"));
    assert!(river_full.contains("toggle-fullscreen"));
    assert!(x11_full.contains("toggle,fullscreen"));
    assert_eq!(Session::Wayland.fullscreen_window_command(), None);
}

#[test]
fn window_actions_are_per_session() {
    let niri = Session::Niri.window_actions();

    assert_eq!(niri.close.as_deref(), Some("niri msg action close-window"));
    assert!(
        niri.maximize
            .as_deref()
            .is_some_and(|command| command.contains("maximize-column"))
    );
    assert!(
        niri.float_window
            .as_deref()
            .is_some_and(|command| command.contains("toggle-window-floating"))
    );
    assert!(
        niri.move_left
            .as_deref()
            .is_some_and(|command| command.contains("move-window-left"))
    );
    assert!(
        niri.move_down
            .as_deref()
            .is_some_and(|command| command.contains("move-window-down"))
    );

    let sway = Session::Sway.window_actions();

    assert!(
        sway.close
            .as_deref()
            .is_some_and(|command| command.contains("swaymsg kill"))
    );
    assert!(
        sway.float_window
            .as_deref()
            .is_some_and(|command| command.contains("floating toggle"))
    );
    assert!(
        sway.move_right
            .as_deref()
            .is_some_and(|command| command.contains("move right"))
    );
    assert_eq!(sway.maximize, None);

    let hyprland = Session::Hyprland.window_actions();

    assert!(
        hyprland
            .maximize
            .as_deref()
            .is_some_and(|command| command.contains("fullscreen 1"))
    );
    assert!(
        hyprland
            .float_window
            .as_deref()
            .is_some_and(|command| command.contains("togglefloating"))
    );
    assert!(
        hyprland
            .move_up
            .as_deref()
            .is_some_and(|command| command.contains("movewindow u"))
    );

    let river = Session::River.window_actions();

    assert_eq!(river.float_window.as_deref(), Some("riverctl toggle-float"));
    assert_eq!(river.maximize, None);
    assert_eq!(river.move_left, None);

    let x11 = Session::X11.window_actions();

    assert!(
        x11.close
            .as_deref()
            .is_some_and(|command| command.contains("wmctrl -c"))
    );
    assert!(
        x11.maximize
            .as_deref()
            .is_some_and(|command| command.contains("maximized_vert"))
    );
    assert_eq!(x11.float_window, None);
    assert_eq!(x11.move_left, None);

    let wayland = Session::Wayland.window_actions();

    assert_eq!(wayland.close, None);
    assert_eq!(wayland.fullscreen, None);
    assert_eq!(wayland.maximize, None);
}

#[test]
fn parse_focused_niri() {
    let text = r#"{"id": 5, "title": "Home", "app_id": "org.gnome.Nautilus"}"#;

    assert_eq!(
        Session::Niri.parse_focused(text),
        Some(FocusedWindow {
            title: "Home".to_owned(),
            app_id: "org.gnome.Nautilus".to_owned(),
        })
    );
    assert_eq!(Session::Niri.parse_focused(r#"{"id": null}"#), None);
    assert_eq!(Session::Niri.parse_focused("not json"), None);
}

#[test]
fn parse_focused_hyprland() {
    let text = r#"{"address": "0x55", "title": "Home", "class": "nautilus", "initialClass": "org.gnome.Nautilus", "pid": 4242}"#;

    assert_eq!(
        Session::Hyprland.parse_focused(text),
        Some(FocusedWindow {
            title: "Home".to_owned(),
            app_id: "org.gnome.Nautilus".to_owned(),
        })
    );
    assert_eq!(
        Session::Hyprland.parse_focused(r#"{"title": "Term", "class": "foot"}"#),
        Some(FocusedWindow {
            title: "Term".to_owned(),
            app_id: "foot".to_owned(),
        })
    );
    assert_eq!(Session::Hyprland.parse_focused("{}"), None);
    assert_eq!(Session::River.parse_focused(""), None);
}

#[test]
fn parse_focused_sway() {
    let tree = r#"{"name": "root", "focused": false, "nodes": [{"name": "1", "nodes": [{"name": "Notes", "app_id": "org.gnome.TextEditor", "focused": true}], "floating_nodes": []}], "floating_nodes": []}"#;

    assert_eq!(
        Session::Sway.parse_focused(tree),
        Some(FocusedWindow {
            title: "Notes".to_owned(),
            app_id: "org.gnome.TextEditor".to_owned(),
        })
    );

    let x11_window = r#"{"nodes": [{"name": "Report", "app_id": null, "focused": true, "window_properties": {"class": "LibreOffice"}}], "floating_nodes": []}"#;

    assert_eq!(
        Session::Sway.parse_focused(x11_window),
        Some(FocusedWindow {
            title: "Report".to_owned(),
            app_id: "LibreOffice".to_owned(),
        })
    );
    assert_eq!(Session::Sway.parse_focused("{}"), None);
}

#[test]
fn parse_focused_river() {
    assert_eq!(
        Session::River.parse_focused("Report.odt\n"),
        Some(FocusedWindow {
            title: "Report.odt".to_owned(),
            app_id: String::new(),
        })
    );
    assert_eq!(Session::River.parse_focused(""), None);
    assert_eq!(Session::River.parse_focused("\n"), None);
}

#[test]
fn parse_focused_x11() {
    let xprop = "WM_NAME(UTF8_STRING) = \"Mozilla Firefox\"\nWM_CLASS(STRING) = \"Navigator\", \"Firefox\"\n";

    assert_eq!(
        Session::X11.parse_focused(xprop),
        Some(FocusedWindow {
            title: "Mozilla Firefox".to_owned(),
            app_id: "Firefox".to_owned(),
        })
    );
    assert_eq!(Session::X11.parse_focused(""), None);
    assert_eq!(Session::X11.parse_focused("xprop: no such atom"), None);
}

#[test]
fn parse_focused_unsupported_session() {
    assert_eq!(
        Session::Wayland.parse_focused(r#"{"title": "anything"}"#),
        None
    );
}

#[test]
fn parse_focused_pid() {
    let niri = r#"{"id": 5, "title": "Home", "app_id": "org.gnome.Nautilus", "pid": 4846}"#;
    let sway = r#"{"nodes": [{"name": "1", "nodes": [{"focused": true, "pid": 911}]}], "floating_nodes": []}"#;

    assert_eq!(Session::Niri.focused_pid(niri), 4846);
    assert_eq!(
        Session::Hyprland.focused_pid(r#"{"title": "Home", "pid": 4242}"#),
        4242
    );
    assert_eq!(Session::Sway.focused_pid(sway), 911);
    assert_eq!(
        Session::X11.focused_pid("_NET_WM_PID(CARDINAL) = 1234\nWM_NAME(UTF8_STRING) = \"x\"\n"),
        1234
    );
    assert_eq!(Session::X11.focused_pid("no pid"), 0);
    assert_eq!(Session::Niri.focused_pid(""), 0);
}

#[test]
fn parse_x11_gtk_paths() {
    let xprop = "WHELK_ACTIVE_WINDOW=0x3a00007\n\
                 WM_NAME(UTF8_STRING) = \"Report\"\n\
                 WM_CLASS(STRING) = \"libreoffice\", \"LibreOffice\"\n\
                 _GTK_UNIQUE_BUS_NAME(UTF8_STRING) = \"org.libreoffice.LibreOffice\"\n\
                 _GTK_APPLICATION_OBJECT_PATH(UTF8_STRING) = \"/org/libreoffice/App\"\n\
                 _GTK_WINDOW_OBJECT_PATH(UTF8_STRING) = \"/org/libreoffice/Window/1\"\n\
                 _GTK_MENUBAR_OBJECT_PATH(UTF8_STRING) = \"/org/libreoffice/App/Menubar\"\n\
                 _GTK_APP_MENU_OBJECT_PATH(UTF8_STRING) = \"/org/libreoffice/App/AppMenu\"\n";
    let paths = Session::X11.parse_focused_paths(xprop);

    assert_eq!(paths.window_id, 0x3a00007);
    assert_eq!(
        paths.bus_name.as_deref(),
        Some("org.libreoffice.LibreOffice")
    );
    assert_eq!(paths.app_path.as_deref(), Some("/org/libreoffice/App"));
    assert_eq!(
        paths.window_path.as_deref(),
        Some("/org/libreoffice/Window/1")
    );
    assert_eq!(
        paths.menubar_path.as_deref(),
        Some("/org/libreoffice/App/Menubar")
    );
    assert_eq!(
        paths.appmenu_path.as_deref(),
        Some("/org/libreoffice/App/AppMenu")
    );

    // A window that exports no GTK properties still parses, with no paths.
    assert_eq!(
        Session::X11.parse_focused_paths("WM_CLASS(STRING) = \"x\", \"XTerm\"\n"),
        FocusedPaths::default()
    );

    // Wayland does not expose another client's window properties.
    assert_eq!(
        Session::Wayland.parse_focused_paths(xprop),
        FocusedPaths::default()
    );
}

#[test]
fn shortcut_tool_lookup_is_optional() {
    for (session, wanted) in [(Session::X11, "xdotool"), (Session::Wayland, "wtype")] {
        if let Some(tool) = session.shortcut_tool() {
            assert!(tool.ends_with(wanted));
            assert!(std::path::Path::new(&tool).is_file());
        }
    }
}

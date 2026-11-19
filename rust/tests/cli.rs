// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_whelk")
}

fn scheme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scheme")
}

fn run(args: &[&str], config: Option<&Path>) -> std::process::Output {
    let mut command = Command::new(binary());
    command.env("WHELK_SCHEME_DIR", scheme_dir());
    command.args(args);

    if let Some(config) = config {
        command.arg("--config").arg(config);
    }

    command.output().expect("run whelk")
}

#[test]
fn version_does_not_need_a_display_or_a_config() {
    let output = run(&["--version"], None);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("whelk {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_the_forwarded_command_surface() {
    let output = run(&["--help"], None);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    for option in [
        "--launcher",
        "--clipboard-status",
        "--menu",
        "--notifications",
        "--night-light",
        "--idle-status",
        "--network",
        "--lock",
        "--reload",
        "--quit",
    ] {
        assert!(stdout.contains(option), "help omitted {option}: {stdout}");
    }
}

#[test]
fn check_config_reads_scheme_and_reports_the_policy() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("settings.scm");
    std::fs::write(
        &config,
        r#"
(configure-shell!
 #:bars (list (bar #:name 'main))
 #:wallpaper (wallpaper-settings #:enabled? #t
                                #:paths (list "/walls")
                                #:interval 300
                                #:animation 'slide-left
                                #:fit 'contain))
"#,
    )
    .expect("write settings");

    let output = run(&["--check-config"], Some(&config));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("wallpaper: enabled"));
    assert!(stdout.contains("slide-left"));
    assert!(stdout.contains("300"));
}

#[test]
fn check_config_walks_the_configured_wallpaper_collection() {
    let directory = tempfile::tempdir().expect("tempdir");
    let walls = directory.path().join("walls");
    std::fs::create_dir_all(&walls).expect("create walls");
    std::fs::write(walls.join("one.png"), b"test").expect("write image");
    std::fs::write(walls.join("movie.mp4"), b"test").expect("write video");

    let config = directory.path().join("settings.scm");
    std::fs::write(
        &config,
        format!(
            r#"
(configure-shell!
 #:wallpaper (wallpaper-settings #:enabled? #t
                                #:paths (list "{}")
                                #:video? #f))
"#,
            walls.display()
        ),
    )
    .expect("write settings");

    let output = run(&["--check-config"], Some(&config));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("wallpapers: 1"), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("current: {}", walls.join("one.png").display())),
        "stdout: {stdout}"
    );
}

#[test]
fn a_missing_config_still_reports_the_default_policy() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("missing.scm");

    let output = run(&["--check-config"], Some(&config));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("wallpaper: off"));
}

#[test]
fn a_broken_config_exits_with_a_clear_error() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("settings.scm");
    std::fs::write(&config, "(configure-shell! #:bars (").expect("write settings");

    let output = run(&["--check-config"], Some(&config));

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wallpaper settings"));
}

/// A run with a private `XDG_STATE_HOME`, so a saved GUI override (a bar's
/// theme, say) cannot win over the settings file under test.
fn run_with(args: &[&str], config: &Path, state: &Path) -> std::process::Output {
    let mut command = Command::new(binary());

    command.env("WHELK_SCHEME_DIR", scheme_dir());
    command.env("XDG_STATE_HOME", state);
    command.args(args);
    command.arg("--config").arg(config);
    command.output().expect("run whelk")
}

#[test]
fn print_css_prints_the_resolved_bar_sheet() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = directory.path().join("settings.scm");
    std::fs::write(
        &config,
        r#"
(configure-shell!
 #:bars (list (bar #:name 'main
                    #:theme 'tinted-2024
                    #:padding 10
                    #:spacing 8
                    #:css ".whelk-bar .mine { font-weight: 700; }")))
"#,
    )
    .expect("write settings");
    let state = directory.path().join("state");
    std::fs::create_dir_all(&state).expect("state dir");

    let output = run_with(&["--print-css"], &config, &state);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The theme is resolved before the sheet is generated, so the printed
    // radius is the palette's rather than the bar's own default of 0.
    assert!(
        stdout.contains("/* whelk") && stdout.contains("bar \"main\", theme tinted-2024"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("window#whelk-main .whelk-bar {"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("border-radius: 16px;"), "stdout: {stdout}");
    // The metrics come from the bar: ends from #:padding, the module inset from
    // half of #:spacing.
    assert!(
        stdout.contains("window#whelk-main .whelk-bar { padding: 0 10px; }"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("padding: 0 4px;"), "stdout: {stdout}");
    // The configuration's own rules are last, as the shell appends them.
    assert!(
        stdout.contains("/* The configuration's own rules */"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(".whelk-bar .mine { font-weight: 700; }"),
        "stdout: {stdout}"
    );
}

fn status_config(directory: &Path) -> PathBuf {
    let config = directory.join("settings.scm");

    std::fs::write(
        &config,
        r#"
(configure-shell!
 #:bars (list (bar #:name 'main
                    #:left (list (clock-module))
                    #:right (list (wifi-module) (mpris-module)))))
"#,
    )
    .expect("write settings");

    config
}

#[test]
fn status_prints_the_session_logind_tools_and_each_module() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = status_config(directory.path());
    let output = run_with(&["--status"], &config, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.starts_with(&format!("whelk {}\n", env!("CARGO_PKG_VERSION"))),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("config: {}\n", config.display())),
        "{stdout}"
    );
    assert!(stdout.contains("session: "), "{stdout}");
    assert!(stdout.contains("logind: "), "{stdout}");
    assert!(stdout.contains("tools:\n"), "{stdout}");
    // The tools the report promises, whether or not this machine has them.
    for tool in ["curl", "nmcli", "systemd-inhibit/elogind-inhibit"] {
        assert!(stdout.contains(tool), "tools omitted {tool}: {stdout}");
    }
    // One line per configured module, naming its bar and section.
    assert!(stdout.contains("main/left/clock  ok: "), "{stdout}");
    assert!(stdout.contains("main/right/wifi  "), "{stdout}");
    assert!(
        stdout.contains("main/right/mpris  unchecked: needs an org.mpris"),
        "{stdout}"
    );
}

#[test]
fn status_json_carries_the_same_facts() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config = status_config(directory.path());
    let output = run_with(&["--status", "--json"], &config, directory.path());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = whelk_rs::json::Json::parse(&stdout).expect("status --json is JSON");

    assert_eq!(json.get_str("version"), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(
        json.get("config").and_then(|config| config.as_str()),
        Some(config.display().to_string().as_str())
    );
    assert!(
        json.get("session")
            .and_then(|session| session.get_str("name"))
            .is_some(),
        "{stdout}"
    );
    assert!(
        matches!(
            json.get("logind")
                .and_then(|logind| logind.get_str("owner")),
            Some("systemd" | "elogind" | "other" | "unreachable")
        ),
        "{stdout}"
    );
    assert_eq!(
        json.get("tools")
            .and_then(|tools| tools.array())
            .map(|tools| tools.len()),
        Some(8)
    );

    let modules = json
        .get("modules")
        .and_then(|modules| modules.array())
        .expect("modules array");

    assert_eq!(modules.len(), 3);
    assert_eq!(modules[0].get_str("kind"), Some("clock"));
    assert_eq!(modules[0].get_str("state"), Some("ok"));
    assert_eq!(modules[2].get_str("state"), Some("unchecked"));
}

#[test]
fn json_without_status_is_refused() {
    let output = Command::new(binary())
        .env("WHELK_SCHEME_DIR", scheme_dir())
        .arg("--json")
        .output()
        .expect("run whelk");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--json needs --status"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

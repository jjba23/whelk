// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::PathBuf;
use std::process::ExitCode;

use whelk_rs::bar::BarSettings;
use whelk_rs::clipboard;
use whelk_rs::collection::collect;
use whelk_rs::config::ConfigLoader;
use whelk_rs::hasp;
use whelk_rs::rotation::Playlist;
use whelk_rs::settings::default_config_path;
use whelk_rs::{cli, frost, status, style, supervisor, ui};

const USAGE: &str = "\
whelk -- a Wayland shell, GTK4 and GNU Guile Scheme

Usage: whelk [OPTIONS]

Options:
  --config PATH          Read this settings file (default: $XDG_CONFIG_HOME/whelk/settings.scm)
  --check-config         Load the configuration, print the wallpaper policy and exit
  --check-lock           Ask the installed hasp how this machine would lock and
                         authenticate, then exit
  --print-css            Print the CSS generated for each configured bar and exit
  --status               Report the session, logind, the runtime tools and why
                         each configured module is blank
  --json                 With --status, print the report as JSON
  --launcher, -l         Open the launcher in the running shell
  --launcher-mode MODE   Launcher mode: app, window, run, calc, file, web, menu,
                         ssh, quit, clipboard, menuitem or session
  --clipboard            Open the clipboard picker
  --clipboard-status     Print clipboard availability, count and bytes
  --clipboard-pause      Pause or resume clipboard capture
  --clipboard-clear      Delete the whole clipboard history
  --clipboard-clear-unpinned
                         Delete every clipboard entry that is not pinned
  --clipboard-restart    Restart the clipboard listener
  --menu COMMAND, -m     Open a menu from label<TAB>command lines
  --session-menu         Open the built-in session menu
  --settings             Open the settings window
  --wallpaper-picker     Open the wallpaper picker
  --notifications, -n    Open or close the notification centre
  --dnd                  Turn do-not-disturb on or off
  --night-light          Turn the night light on or off
  --inhibit              Take or release the logind idle inhibitor
  --inhibit-reason TEXT  Why the inhibitor is held
  --idle-status          Explain the idle timer, inhibitor and DND
  --network PAGE         Show the network panel: wifi or bluetooth
  --lock                 Ask the running shell to lock the screen
  --reload, -r           Re-read the settings file
  --quit, -q             Stop the running shell
  --version, -v, -V      Print the version and exit
  -h, --help             Print this help and exit

The action options are forwarded to the running shell through GApplication; a
second whelk does not start a second bar.  `--config' belongs to the first
process.  Locking is optional: whelk runs the separate `hasp' program, found
through HASP_BIN or PATH, and keeps running when it is absent.
";

fn main() -> ExitCode {
    if std::env::args().any(|argument| argument == "--clipboard-store") {
        return ExitCode::from(clipboard::helper().clamp(0, 255) as u8);
    }

    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("whelk: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let options = cli::Options::parse(std::env::args().skip(1))?;

    if options.help {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }

    if options.version {
        println!("whelk {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    if options.json && !options.status {
        return Err("--json needs --status".to_owned());
    }

    if options.check_lock {
        // The lock screen and its authentication belong to hasp; its report
        // is the one that names the library, stack and fallback the lock will
        // actually use.
        print!("{}", hasp::check()?);

        return Ok(ExitCode::SUCCESS);
    }

    // The logind trigger starts this hidden entry point directly.  A public
    // `--lock` is a request to the running shell, so starting a lock process
    // from one can never forward back into another lock process.
    if options.lock_now {
        return hasp::lock_now();
    }

    let config_path = options.config.unwrap_or_else(default_config_path);
    let mut loader = ConfigLoader::new(config_path.clone(), scheme_paths());

    if options.check_config {
        let loaded = loader.load()?;

        println!("whelk {}", env!("CARGO_PKG_VERSION"));
        println!("config: {}", config_path.display());
        println!("{}", loaded.wallpaper.summary());

        for bar in &loaded.bars {
            println!("{}", bar.summary());
        }

        println!("idle lock-after: {} s", loaded.idle_lock_after);
        println!("{}", loaded.clipboard.summary());
        println!("shell menu: {} row(s)", loaded.menu.len());

        // Exercise the same walk the wallpaper engine will use.  A file the
        // walker cannot see is a configuration problem worth reporting here,
        // before the shell starts.
        let wallpapers = collect(
            &loaded.wallpaper.paths,
            loaded.wallpaper.current.as_deref(),
            loaded.wallpaper.video,
        );
        println!("wallpapers: {}", wallpapers.len());

        let mut playlist = Playlist::new(wallpapers, loaded.wallpaper.video, 0x9e37_79b9_7f4a_7c15);
        playlist.resolve(loaded.wallpaper.current.as_deref());

        if let Some(current) = playlist.selected() {
            println!("current: {}", current.display());
        }

        return Ok(ExitCode::SUCCESS);
    }

    if options.status {
        let loaded = loader.load()?;
        let report = status::collect(&loaded.bars, &config_path);

        if options.json {
            print!("{}", report.json());
        } else {
            print!("{}", report.text());
        }

        return Ok(ExitCode::SUCCESS);
    }

    if options.print_css {
        let loaded = loader.load()?;

        // The finish is part of the skin, so it is installed before the sheet
        // is generated: a printed bar with `#:frost` has its gradients and its
        // shadow, exactly as the shell would paint them.
        frost::install(loaded.frost.clone());

        print_css(&loaded.bars);

        return Ok(ExitCode::SUCCESS);
    }

    let label = loader.label();

    // The supervisor forks before GTK or Guile is initialized.  The child
    // re-enters here with WHELK_SUPERVISED and runs the UI in this process.
    if let Some(status) = supervisor::run()? {
        return Ok(ExitCode::from(status.clamp(0, 255) as u8));
    }

    Ok(ui::run(loader, label).into())
}

/// The skin `--print-css` prints: one bar's generated sheet per configured
/// bar, under a comment naming what it belongs to.  It is the same output
/// `ui::install_css` loads, minus the structural rules the shell compiles in.
fn print_css(bars: &[BarSettings]) {
    let widget_ids = style::unique_widget_ids(bars);

    for (index, bar) in bars.iter().enumerate() {
        if index > 0 {
            println!();
        }

        println!(
            "/* whelk {}: bar \"{}\", theme {} */",
            env!("CARGO_PKG_VERSION"),
            bar.name,
            bar.theme.as_deref().unwrap_or("none")
        );
        print!(
            "{}",
            style::css(&widget_ids[index], bar, "whelk-bar", bar.css.as_deref())
        );
    }
}

fn scheme_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(directory) = std::env::var_os("WHELK_SCHEME_DIR") {
        candidates.push(PathBuf::from(directory));
    }

    // Only paths the user or the installer chose.  A relative `scheme' and
    // the build-time CARGO_MANIFEST_DIR are attacker-writable in packaged
    // builds: either can point at a directory the user did not install.
    // `maak run` exports WHELK_SCHEME_DIR for the development tree.

    // A staged or custom-prefix install keeps the modules beside its binary.
    if let Ok(executable) = std::env::current_exe()
        && let Some(bin) = executable.parent()
        && let Some(prefix) = bin.parent()
    {
        candidates.push(prefix.join("share/guile/site/3.0"));
    }

    candidates.push(PathBuf::from("/usr/share/guile/site/3.0"));

    let mut paths = Vec::new();

    // The widget project owns `(bibelot theme)`; its installed tree shares the
    // Guile site directory, and its development tree is found through
    // BIBELOT_SCHEME_DIR or the debug-build manifest directory.  A release
    // tarball carries a pinned copy under rust/vendor/bibelot.
    candidates.extend(bibelot::theme::scheme_paths());

    for candidate in candidates {
        let directory = std::fs::canonicalize(&candidate).unwrap_or(candidate);

        if directory.is_dir() && !paths.contains(&directory) {
            paths.push(directory);
        }
    }

    paths
}

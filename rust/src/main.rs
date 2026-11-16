// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use whelk_rs::collection::collect;
use whelk_rs::guile::Guile;
use whelk_rs::rotation::Playlist;
use whelk_rs::settings::{WallpaperSettings, default_config_path};
use whelk_rs::ui;

const USAGE: &str = "\
whelk-rs -- the Rust bootstrap for the whelk Wayland shell

Usage: whelk-rs [OPTIONS]

Options:
  --config PATH     Read this settings file (default: $XDG_CONFIG_HOME/whelk/settings.scm)
  --check-config    Load the configuration, print the wallpaper policy and exit
  --version, -V     Print the Rust bootstrap version and exit
  -h, --help        Print this help and exit

The C shell remains installed as `whelk`; this binary is the migration
bootstrap and uses a separate application id.
";

#[derive(Debug, Default)]
struct Options {
    config: Option<PathBuf>,
    check_config: bool,
    version: bool,
    help: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("whelk-rs: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let options = parse_args(std::env::args().skip(1))?;

    if options.help {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }

    if options.version {
        println!("whelk-rs {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    let config_path = options.config.unwrap_or_else(default_config_path);
    let settings = load_settings(&config_path)?;

    if options.check_config {
        println!("whelk-rs {}", env!("CARGO_PKG_VERSION"));
        println!("config: {}", config_path.display());
        println!("{}", settings.summary());

        // Exercise the same walk the wallpaper engine will use.  A file the
        // walker cannot see is a configuration problem worth reporting here,
        // before the shell starts.
        let wallpapers = collect(&settings.paths, settings.current.as_deref(), settings.video);
        println!("wallpapers: {}", wallpapers.len());

        let mut playlist = Playlist::new(wallpapers, settings.video, 0x9e37_79b9_7f4a_7c15);
        playlist.resolve(settings.current.as_deref());

        if let Some(current) = playlist.selected() {
            println!("current: {}", current.display());
        }

        return Ok(ExitCode::SUCCESS);
    }

    let label = if config_path.is_file() {
        config_path.display().to_string()
    } else {
        format!("{} (missing; using defaults)", config_path.display())
    };

    Ok(ui::run(settings, label).into())
}

fn load_settings(path: &Path) -> Result<WallpaperSettings, String> {
    if !path.is_file() {
        return Ok(WallpaperSettings::default());
    }

    let guile = Guile::init(&scheme_paths())
        .map_err(|error| format!("wallpaper settings could not be loaded: {error}"))?;

    guile
        .load_wallpaper(path)
        .map_err(|error| format!("wallpaper settings could not be loaded: {error}"))
}

fn scheme_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(directory) = std::env::var_os("WHELK_SCHEME_DIR") {
        candidates.push(PathBuf::from(directory));
    }

    candidates.push(PathBuf::from("scheme"));
    candidates.push(PathBuf::from("../scheme"));

    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest).join("../scheme"));
    }

    candidates.push(PathBuf::from("/usr/share/guile/site/3.0"));

    let mut paths = Vec::new();

    for candidate in candidates {
        let directory = std::fs::canonicalize(&candidate).unwrap_or(candidate);

        if directory.is_dir() && !paths.contains(&directory) {
            paths.push(directory);
        }
    }

    paths
}

fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut arguments = arguments;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => options.help = true,
            "--version" | "-V" => options.version = true,
            "--check-config" => options.check_config = true,
            "--config" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--config needs a path".to_owned())?;
                options.config = Some(PathBuf::from(value));
            }
            _ if argument.starts_with("--config=") => {
                let value = &argument["--config=".len()..];

                if value.is_empty() {
                    return Err("--config needs a path".to_owned());
                }

                options.config = Some(PathBuf::from(value));
            }
            _ => return Err(format!("unknown option {argument}\n\n{USAGE}")),
        }
    }

    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn config_forms_are_equivalent() {
        let split = parse_args(["--config".to_owned(), "/tmp/a.scm".to_owned()].into_iter())
            .expect("split form");
        let joined =
            parse_args(["--config=/tmp/a.scm".to_owned()].into_iter()).expect("joined form");

        assert_eq!(split.config, joined.config);
        assert_eq!(
            split.config.as_deref(),
            Some(std::path::Path::new("/tmp/a.scm"))
        );
    }

    #[test]
    fn unknown_options_are_errors() {
        assert!(parse_args(["--nope".to_owned()].into_iter()).is_err());
        assert!(parse_args(["--config".to_owned()].into_iter()).is_err());
    }
}

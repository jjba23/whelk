// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The small, dependency-free packaging task.
//!
//! This is the project's install entry point: build `whelk`, stage the binary,
//! the Guile modules, the desktop entry and the icon tree under a prefix.  It
//! runs from the workspace root, so its target and the shared data files have
//! one location.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const BINARY: &str = "whelk";
const APP_ID: &str = "org.jointhefreeworld.whelk";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);

    match arguments.next().as_deref() {
        None | Some("help") | Some("--help") | Some("-h") => {
            usage();
            Ok(())
        }
        Some("install") => install(arguments.collect()),
        Some("dist") => dist(arguments.collect()),
        Some(command) => Err(format!(
            "unknown command {command:?}; try `cargo xtask help`"
        )),
    }
}

fn usage() {
    println!(
        "Usage: cargo xtask <install|dist>\n\
         \n\
         install [--prefix DIR] [--destdir DIR] [--debug] [--no-build]\n\
           build the Rust shell and stage whelk-rs, the shared Scheme modules\n\
           and the Rust README; the C `whelk` binary is not installed or replaced\n\
         dist [--version VERSION] [--output DIR]\n\
           write a source archive from Git HEAD\n\
         \n\
         --prefix DIR   install prefix (default: /usr/local)\n\
         --destdir DIR  staging root, as used by package recipes (or $DESTDIR)\n\
         --debug        build and install the debug profile\n\
         --no-build     use the existing binary in the target directory"
    );
}

fn install(arguments: Vec<String>) -> Result<(), String> {
    let mut prefix = PathBuf::from("/usr/local");
    let mut destdir = env::var_os("DESTDIR").map(PathBuf::from);
    let mut debug = false;
    let mut build = true;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--prefix" => {
                prefix = PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--prefix needs a directory".to_owned())?,
                );
            }
            "--destdir" => {
                destdir = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--destdir needs a directory".to_owned())?,
                ));
            }
            "--debug" => debug = true,
            "--no-build" => build = false,
            other if other.starts_with("--prefix=") => {
                prefix = PathBuf::from(&other["--prefix=".len()..]);
            }
            other if other.starts_with("--destdir=") => {
                destdir = Some(PathBuf::from(&other["--destdir=".len()..]));
            }
            other => return Err(format!("unknown install option {other:?}")),
        }
    }

    let workspace = workspace_root()?;
    let profile = if debug { "debug" } else { "release" };
    let target = workspace.join("target").join(profile);

    if build {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut command = Command::new(cargo);

        command
            .current_dir(&workspace)
            .args(["build", "--locked", "--bin", BINARY]);

        if !debug {
            command.arg("--release");
        }

        let status = command
            .status()
            .map_err(|error| format!("could not run cargo: {error}"))?;

        if !status.success() {
            return Err(format!("cargo build failed with {status}"));
        }
    }

    let binary = target.join(BINARY);

    if !binary.is_file() {
        return Err(format!(
            "{} is missing; run without --no-build first",
            binary.display()
        ));
    }

    let root = install_root(&prefix, destdir.as_deref());

    install_file(&binary, &root.join("bin").join(BINARY), 0o755)?;
    copy_directory(
        &workspace.join("scheme/whelk"),
        &root.join("share/guile/site/3.0/whelk"),
    )?;
    install_file(
        &workspace.join("rust/README.md"),
        &root.join("share/doc/whelk/README.md"),
        0o644,
    )?;
    install_file(
        &workspace.join("README.org"),
        &root.join("share/doc/whelk/README.org"),
        0o644,
    )?;
    install_text(
        &root
            .join("share/applications")
            .join(format!("{APP_ID}.desktop")),
        &desktop_entry(),
        0o644,
    )?;
    copy_directory(&workspace.join("data/icons"), &root.join("share/icons"))?;

    println!(
        "installed {} to {}",
        BINARY,
        root.join("bin").join(BINARY).display()
    );
    println!(
        "installed Scheme modules to {}",
        root.join("share/guile/site/3.0/whelk").display()
    );
    println!(
        "installed the Rust desktop entry to {}",
        root.join("share/applications")
            .join(format!("{APP_ID}.desktop"))
            .display()
    );
    println!(
        "installed whelk; set WHELK_SCHEME_DIR={} if this prefix is not on \
         Guile's load path",
        root.join("share/guile/site/3.0").display()
    );

    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no parent workspace directory".to_owned())
}

/// The checkout, rather than the Rust workspace: `git archive` must see the
/// tree too.  The workspace root is that checkout.
fn repository_root() -> Result<PathBuf, String> {
    workspace_root()
}

fn dist(arguments: Vec<String>) -> Result<(), String> {
    let mut version = env!("CARGO_PKG_VERSION").to_owned();
    let mut output = PathBuf::from(".");
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" => {
                version = arguments
                    .next()
                    .ok_or_else(|| "--version needs a value".to_owned())?;
            }
            "--output" => {
                output = PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--output needs a directory".to_owned())?,
                );
            }
            other => return Err(format!("unknown dist option {other:?}")),
        }
    }

    let repository = repository_root()?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()
            .map_err(|error| format!("could not read the current directory: {error}"))?
            .join(output)
    };

    fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let archive = output.join(format!("whelk-{version}.tar.gz"));
    let archive_path = archive
        .to_str()
        .ok_or_else(|| "the archive path is not UTF-8".to_owned())?;
    let prefix = format!("--prefix=whelk-{version}/");
    let status = Command::new("git")
        .current_dir(&repository)
        .args([
            "archive",
            "--format=tar.gz",
            &prefix,
            "-o",
            archive_path,
            "HEAD",
        ])
        .status()
        .map_err(|error| format!("could not run git: {error}"))?;

    if !status.success() {
        return Err(format!("git archive failed with {status}"));
    }

    println!("wrote {}", archive.display());

    Ok(())
}

/// The installed desktop entry.  It points at the canonical whelk application
/// id and the icon tree the install stages beside it.
fn desktop_entry() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Whelk\n\
         GenericName=Wayland Shell\n\
         Comment=A Wayland shell -- status bar, launcher and desktop widgets, configured in Guile Scheme\n\
         Exec={BINARY}\n\
         Icon=org.jointhefreeworld.whelk\n\
         Terminal=false\n\
         Categories=System;Utility;\n\
         Keywords=bar;panel;status;shell;wayland;guile;scheme;\n\
         StartupNotify=false\n\
         StartupWMClass={APP_ID}\n"
    )
}

fn install_text(destination: &Path, contents: &str, mode: u32) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::write(destination, contents)
        .map_err(|error| format!("could not write {}: {error}", destination.display()))?;
    set_mode(destination, mode)
}

fn install_root(prefix: &Path, destdir: Option<&Path>) -> PathBuf {
    match destdir {
        Some(destdir) => {
            let relative = prefix.strip_prefix("/").unwrap_or(prefix);
            destdir.join(relative)
        }
        None => prefix.to_path_buf(),
    }
}

fn install_file(source: &Path, destination: &Path, mode: u32) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    fs::copy(source, destination).map_err(|error| {
        format!(
            "could not copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;

    set_mode(destination, mode)
}

fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| format!("could not set permissions on {}: {error}", path.display()))?;
    }

    #[cfg(not(unix))]
    let _ = mode;

    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let entries = fs::read_dir(source)
        .map_err(|error| format!("could not read {}: {error}", source.display()))?;

    fs::create_dir_all(destination)
        .map_err(|error| format!("could not create {}: {error}", destination.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not read {}: {error}", source.display()))?;
        let path = entry.path();
        let target = destination.join(entry.file_name());

        if path.is_dir() {
            copy_directory(&path, &target)?;
        } else {
            install_file(&path, &target, 0o644)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cargo_install_ships_a_desktop_entry_for_the_installed_binary() {
        let entry = desktop_entry();

        assert!(entry.ends_with('\n'));
        assert!(entry.contains("Exec=whelk\n"));
        assert!(entry.contains("Icon=org.jointhefreeworld.whelk\n"));
        assert!(entry.contains("Name=Whelk\n"));
        assert!(entry.contains("StartupWMClass=org.jointhefreeworld.whelk\n"));
    }
}

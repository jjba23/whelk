// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The optional boundary to the separate `hasp` lock screen.
//!
//! WheLK does not link hasp.  A shell can be installed and run without it; the
//! lock actions report how to install it, and a lock request starts the
//! program.  This keeps the two projects independently buildable and lets a
//! distribution package each on its own schedule.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::sync::OnceLock;

use crate::posix;

/// The program named by `HASP_BIN`, a sibling of this binary, or `hasp` on
/// `$PATH`, in that order.
fn binary() -> PathBuf {
    if let Some(path) = std::env::var_os("HASP_BIN") {
        return PathBuf::from(path);
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let sibling = directory.join("hasp");

        if sibling.is_file() {
            return sibling;
        }
    }

    PathBuf::from("hasp")
}

fn missing(binary: &PathBuf, error: std::io::Error) -> String {
    format!(
        "{} could not be started: {error}; install hasp or set HASP_BIN",
        binary.display()
    )
}

/// `hasp --check`, printed by `whelk --check-lock`.
pub fn check() -> Result<String, String> {
    let binary = binary();
    let output = Command::new(&binary)
        .arg("--check")
        .output()
        .map_err(|error| missing(&binary, error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        return Err(format!(
            "hasp --check failed with {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();

    if !output.stderr.is_empty() {
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    Ok(text)
}

/// Start a lock screen as a detached hasp process.
///
/// The child is reaped on a helper thread; the lock itself is another process,
/// so the shell can exit without taking the lock screen with it.
pub fn lock() {
    let binary = binary();
    let mut command = Command::new(&binary);

    command
        .arg("--lock")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => eprintln!("whelk: {}", missing(&binary, error)),
    }
}

/// Run the hidden `--lock-now` entry point in the foreground.
pub fn lock_now() -> Result<ExitCode, String> {
    let binary = binary();
    let status = Command::new(&binary)
        .arg("--lock-now")
        .status()
        .map_err(|error| missing(&binary, error))?;

    Ok(ExitCode::from(
        status.code().unwrap_or(1).clamp(0, 255) as u8
    ))
}

/// Start one `hasp --watch` process for this shell's lifetime.
///
/// The watcher follows logind and starts a separate `hasp --lock-now` for each
/// trigger.  A parent-death signal keeps it from surviving a shell restart.
pub fn watch_logind() {
    static STARTED: OnceLock<()> = OnceLock::new();

    if STARTED.set(()).is_err() {
        return;
    }

    let binary = binary();
    let mut command = Command::new(&binary);

    command
        .arg("--watch")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    posix::die_with_parent(&mut command);

    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => eprintln!(
            "whelk: {}; logind will not start the lock screen",
            missing(&binary, error)
        ),
    }
}

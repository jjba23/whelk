// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Running programs, ported from `util/command.c`.
//!
//! A bar polls programs (wpctl, niri, playerctl) and launches them (a
//! launcher, a mixer, a screenshot tool).  Both go through a shell so a
//! configuration can write a pipeline, and both are asynchronous: a compositor
//! whose bar freezes while `playerctl` waits on a stopped player is not a
//! compositor any more.

use std::ffi::OsStr;
use std::process::{Command, Stdio};

use gtk::gio;
use gtk::glib;
use gtk4 as gtk;

use crate::format;

/// Runs `command_line` asynchronously.  Standard error is discarded, because a
/// missing optional tool is a normal answer and not something to paint on the
/// console every two seconds.
///
/// The callback receives the program's standard output with trailing line
/// endings removed, or `None` when the program could not be started.
pub fn run<F>(command_line: &str, callback: F)
where
    F: FnOnce(Option<String>) + 'static,
{
    run_text(command_line, None, move |output, _status| callback(output));
}

/// The same as [`run`], for a program that asks its question on standard input:
/// a secret goes here and never through the command line, where `ps` would
/// publish it for as long as the program runs.  An empty input leaves the
/// program with an empty stdin, not an open pipe that never ends.
pub fn run_input<F>(command_line: &str, input: Option<&str>, callback: F)
where
    F: FnOnce(Option<String>) + 'static,
{
    run_text(command_line, input, move |output, _status| callback(output));
}

/// [`run_input`], with the exit status.  A program's answer is not always a
/// sentence to show, and its words are in whatever language the machine
/// speaks; "did it fail?" is the status, and it needs no translating.
pub fn run_input_status<F>(command_line: &str, input: Option<&str>, callback: F)
where
    F: FnOnce(Option<String>, i32) + 'static,
{
    run_text(command_line, input, callback);
}

/// [`run`], for output that is not lines of text.  The menu-script protocol
/// separates a row's fields with NUL bytes, which a Rust `String` cannot carry,
/// so this is the one reader that keeps every byte.
pub fn run_bytes<F>(command_line: &str, callback: F)
where
    F: FnOnce(Option<Vec<u8>>) + 'static,
{
    if command_line.is_empty() {
        callback(None);

        return;
    }

    let process = match spawn_process(
        command_line,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    ) {
        Ok(process) => process,
        Err(error) => {
            log(&format!("could not start '{command_line}': {error}"));
            callback(None);

            return;
        }
    };

    process.communicate_async(None, gio::Cancellable::NONE, move |result| match result {
        Ok((Some(bytes), _)) => callback(Some(bytes.as_ref().to_vec())),
        Ok((None, _)) => callback(Some(Vec::new())),
        Err(error) => {
            log(&format!("reading the program's output failed: {error}"));
            callback(None);
        }
    });
}

/// Return a command line that runs `command_line` with `WHELK_RETV` set
/// explicitly: `"0"` when a menu is being listed, `"1"` when one of its rows
/// was chosen.  It is written even when the value is 0, so a menu opened from
/// something that exported `WHELK_RETV=1` cannot list itself in accepting mode.
pub fn with_retv(command_line: &str, retv: &str) -> String {
    let retv = if retv.is_empty() { "0" } else { retv };

    format!("WHELK_RETV={retv} {command_line}")
}

/// Starts `command_line` detached, with its output going nowhere: the click
/// handlers' half of the module.  A failure is logged and otherwise ignored.
pub fn spawn(command_line: &str) {
    if command_line.is_empty() {
        return;
    }

    match Command::new("sh")
        .arg("-c")
        .arg(command_line)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // The child is detached from the bar's progress, but not from the
            // process table: a thread reaps it so a click-heavy session does
            // not accumulate zombies.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => log(&format!("could not launch '{command_line}': {error}")),
    }
}

fn run_text<F>(command_line: &str, input: Option<&str>, callback: F)
where
    F: FnOnce(Option<String>, i32) + 'static,
{
    if command_line.is_empty() {
        callback(None, -1);

        return;
    }

    let process = match spawn_process(
        command_line,
        gio::SubprocessFlags::STDOUT_PIPE
            | gio::SubprocessFlags::STDERR_SILENCE
            | gio::SubprocessFlags::STDIN_PIPE,
    ) {
        Ok(process) => process,
        Err(error) => {
            log(&format!("could not start '{command_line}': {error}"));
            callback(None, -1);

            return;
        }
    };
    let status_process = process.clone();
    // The stdin pipe is always present, so an absent or empty input still
    // writes end-of-file instead of leaving the program waiting on ours.
    let stdin_buffer = Some(input.unwrap_or("").to_owned());

    process.communicate_utf8_async(stdin_buffer, gio::Cancellable::NONE, move |result| {
        let output = match result {
            Ok((stdout, _)) => {
                let text = stdout.map(|text| text.to_string()).unwrap_or_default();

                Some(format::strip_newline(&text).to_owned())
            }
            Err(error) => {
                log(&format!("reading the program's output failed: {error}"));

                // The C callback still reports the status and an empty
                // answer when the read fails after the program started.
                Some(String::new())
            }
        };
        let status = if status_process.has_exited() {
            status_process.exit_status()
        } else {
            -1
        };

        callback(output, status);
    });
}

fn spawn_process(
    command_line: &str,
    flags: gio::SubprocessFlags,
) -> Result<gio::Subprocess, glib::Error> {
    let argv = [OsStr::new("sh"), OsStr::new("-c"), OsStr::new(command_line)];

    gio::Subprocess::newv(&argv, flags)
}

fn log(message: &str) {
    eprintln!("whelk-rs: command: {message}");
}

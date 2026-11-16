// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// A program's question, answered on its standard input.  The Wi-Fi panel's key
// travels this way so that `ps` never shows it.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use whelk_rs::command;

/// The C tests run on one process-wide main loop.  Rust's test harness runs
/// tests in parallel, and a `GMainContext` has one owner at a time, so the
/// command tests take turns owning it.
static CONTEXT_LOCK: Mutex<()> = Mutex::new(());

/// Run one asynchronous command and return what its callback received.  The C
/// tests do the same with a `GMainLoop`; the context is acquired before the
/// command starts because GIO's async bindings require their owner thread.
fn wait<T: 'static>(register: impl FnOnce(Box<dyn FnOnce(T)>)) -> T {
    let _serial = CONTEXT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = Rc::new(RefCell::new(None));
    let done = Rc::new(Cell::new(false));
    let context = gtk4::glib::MainContext::default();
    let _guard = context.acquire().expect("the main context");

    {
        let result = Rc::clone(&result);
        let done = Rc::clone(&done);

        register(Box::new(move |value| {
            *result.borrow_mut() = Some(value);
            done.set(true);
        }));
    }

    while !done.get() {
        context.iteration(true);
    }

    result
        .borrow_mut()
        .take()
        .expect("the command callback to run")
}

fn wait_status(
    register: impl FnOnce(Box<dyn FnOnce(Option<String>, i32)>),
) -> (Option<String>, i32) {
    wait(|done| register(Box::new(move |output, status| done((output, status)))))
}

fn run_and_wait(command_line: &str, input: Option<&str>) -> Option<String> {
    wait(|done| command::run_input(command_line, input, done))
}

#[test]
fn input_reaches_the_program() {
    // tr copies stdin to stdout: the answer arrived, and the newline was made
    // into the line a prompt expects.
    assert_eq!(
        run_and_wait("tr a-z A-Z", Some("correct horse\n")),
        Some("CORRECT HORSE".to_owned())
    );
}

#[test]
fn empty_input_closes_stdin() {
    // Without a key the program still gets an end of file, not a hang.
    assert_eq!(run_and_wait("wc -c", None), Some("0".to_owned()));
    assert_eq!(run_and_wait("wc -c", Some("")), Some("0".to_owned()));
}

#[test]
fn status_reports_failure() {
    let (output, status) = wait_status(|done| {
        command::run_input_status("cat >/dev/null; exit 3", Some("ignored"), done)
    });

    // The status is the answer a translated message cannot give: this is what
    // tells the Wi-Fi panel that saving the key failed.
    assert_eq!(status, 3);
    assert_eq!(output.as_deref(), Some(""));
}

#[test]
fn status_reports_success() {
    let (output, status) = wait_status(|done| command::run_input_status("cat", Some("ok\n"), done));

    assert_eq!(status, 0);
    assert_eq!(output.as_deref(), Some("ok"));
}

#[test]
fn plain_run_still_works() {
    assert_eq!(
        run_and_wait("printf 'still here'", None),
        Some("still here".to_owned())
    );
}

#[test]
fn bytes_keep_nul_separators() {
    let bytes = wait(|done| command::run_bytes("printf 'a\\000b'", done));

    assert_eq!(bytes, Some(b"a\0b".to_vec()));
}

// The menu protocol says a listing is called with WHELK_RETV=0 and a choice
// with WHELK_RETV=1.  The launcher hands the generator a shell command, so the
// variable has to be written into that line -- and written even when the value
// it wants is 0, or a menu opened from a script that exported 1 lists itself in
// the accepting mode.
//
// The inner shell does the reading, because that is what a generator is.  A
// bare `$VAR` would expand in this shell, before the assignment could apply,
// and would prove nothing about what the generator receives.

#[test]
fn menu_listing_sets_retv() {
    let command = command::with_retv("sh -c 'printf %s \"$WHELK_RETV\"'", "0");

    assert!(command.starts_with("WHELK_RETV=0 "));
    assert_eq!(run_and_wait(&command, None), Some("0".to_owned()));
}

#[test]
fn menu_choice_sets_retv() {
    let command = command::with_retv("sh -c 'printf %s \"$WHELK_RETV\"'", "1");

    assert_eq!(run_and_wait(&command, None), Some("1".to_owned()));
}

#[test]
fn retv_defaults_to_zero() {
    assert_eq!(command::with_retv("true", ""), "WHELK_RETV=0 true");
    assert_eq!(command::with_retv("", "1"), "WHELK_RETV=1 ");
}

#[test]
fn an_empty_command_does_not_spawn() {
    assert_eq!(run_and_wait("", None), None);
}

fn wait_for_pid(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(2);

    loop {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                if pid > 0 {
                    return pid;
                }
            }
        }

        assert!(
            Instant::now() < deadline,
            "the spawned command did not write a pid to {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_detached_spawn_runs_and_is_reaped() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let pid_file = directory.path().join("pid");
    let quoted = pid_file.to_string_lossy().replace('\'', "'\\''");

    command::spawn(&format!("echo $$ > '{quoted}'"));

    let pid = wait_for_pid(&pid_file);
    let process_entry = format!("/proc/{pid}");
    let deadline = Instant::now() + Duration::from_secs(3);

    // A child that was merely started and never waited on stays in /proc as a
    // zombie, so its entry disappearing is the proof that the reaper ran.
    while Path::new(&process_entry).exists() {
        assert!(
            Instant::now() < deadline,
            "child {pid} was not reaped: {process_entry} still exists"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

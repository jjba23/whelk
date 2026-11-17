// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The process supervisor, ported from the C shell.
//!
//! GTK cannot recover from a segmentation fault inside a module: the fault is
//! in this process.  The supervisor keeps the UI in a child and stays small
//! enough to restart it after an abnormal exit.  It owns no widgets and no
//! configuration.
//!
//! The parent forwards SIGTERM, SIGINT and SIGHUP to the child and waits for the
//! child's own shutdown path, so a terminal Ctrl-C and a systemd stop both
//! flush the clipboard, release the notification bus name and close the
//! surfaces.  A child crash is restarted with a short backoff, up to five rapid
//! failures; a run that lasted more than thirty seconds starts a new incident.

use std::env;
use std::ffi::OsString;
use std::io;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};

const MAX_RESTARTS: u32 = 5;
const STABLE_RUN: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);

static STOP: AtomicBool = AtomicBool::new(false);
static CHILD: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(signal: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);

    let child = CHILD.load(Ordering::SeqCst);

    if child > 0 {
        // `kill` is async-signal-safe.  The child owns its own process group,
        // so this reaches only the UI, never an application a module started.
        let _ = unsafe { libc::kill(child, signal) };
    }
}

fn install_signals() -> Result<(), String> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };

    action.sa_sigaction = on_signal as *const () as usize;
    action.sa_flags = 0; // no SA_RESTART: a wait must notice the signal

    let empty = unsafe { libc::sigemptyset(&mut action.sa_mask) };

    if empty != 0 {
        return Err(io::Error::last_os_error().to_string());
    }

    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let result = unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };

        if result != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
    }

    Ok(())
}

fn disabled(environment: &OsString) -> bool {
    !environment.is_empty() && environment != "0"
}

/// Whether this process should become the supervisor.  A supervised child and
/// `WHELK_NO_SUPERVISE` both run the UI directly, as the C shell does.
pub fn wanted() -> bool {
    if env::var_os("WHELK_NO_SUPERVISE").is_some_and(|value| disabled(&value)) {
        return false;
    }

    env::var_os("WHELK_SUPERVISED").is_none()
}

/// Run the supervisor if this process is the outer invocation.  `Ok(None)`
/// means the caller should run the UI in this process; `Ok(Some(status))` is
/// the supervisor's own exit status.
pub fn run() -> Result<Option<i32>, String> {
    if !wanted() {
        return Ok(None);
    }

    let executable = env::current_exe()
        .map_err(|error| format!("could not find the whelk executable: {error}"))?;
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let parent = unsafe { libc::getpid() };

    install_signals()?;

    let mut failures = 0u32;

    loop {
        if STOP.load(Ordering::SeqCst) {
            return Ok(Some(0));
        }

        let mut command = Command::new(&executable);

        command.args(&arguments).env("WHELK_SUPERVISED", "1");

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            // The child gets its own process group so the terminal's foreground
            // signal reaches the supervisor once, which forwards one deliberate
            // copy.  PDEATHSIG gives the child a SIGTERM if the supervisor is
            // SIGKILLed; the parent check closes the fork/prctl race.
            unsafe {
                command.pre_exec(move || {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(io::Error::last_os_error());
                    }

                    #[cfg(target_os = "linux")]
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(io::Error::last_os_error());
                    }

                    if libc::getppid() != parent {
                        libc::_exit(0);
                    }

                    Ok(())
                });
            }
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start the UI process: {error}"))?;
        let pid = child.id() as libc::pid_t;
        let started = Instant::now();

        CHILD.store(pid, Ordering::SeqCst);

        // A signal can arrive between spawn and publishing the child's pid.
        if STOP.load(Ordering::SeqCst) {
            let _ = unsafe { libc::kill(pid, libc::SIGTERM) };
        }

        let _ = unsafe { libc::setpgid(pid, pid) };

        let status = loop {
            if STOP.load(Ordering::SeqCst) {
                break None;
            }

            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => std::thread::sleep(POLL),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();

                    CHILD.store(0, Ordering::SeqCst);

                    return Err(format!("could not wait for the UI process: {error}"));
                }
            }
        };

        CHILD.store(0, Ordering::SeqCst);

        if STOP.load(Ordering::SeqCst) {
            wait_for_stop(&mut child);

            return Ok(Some(0));
        }

        let Some(status) = status else {
            wait_for_stop(&mut child);

            return Ok(Some(0));
        };

        if status.success() {
            return Ok(Some(0));
        }

        if status.code() == Some(127) {
            return Ok(Some(127));
        }

        if started.elapsed() > STABLE_RUN {
            failures = 0;
        }

        failures += 1;

        if failures >= MAX_RESTARTS {
            eprintln!(
                "whelk: the UI stopped {failures} times; leaving it stopped so a broken \
                 configuration is not restarted forever (WHELK_NO_SUPERVISE=1 runs without \
                 the supervisor)"
            );

            return Ok(Some(1));
        }

        let delay = restart_delay(failures);

        eprintln!(
            "whelk: the UI {}; restarting in {delay} s",
            describe(&status)
        );

        sleep_unless_stopped(Duration::from_secs(delay));
    }
}

/// The C shell's delay: one, two, then at most four seconds.
fn restart_delay(failures: u32) -> u64 {
    1u64 << failures.saturating_sub(1).min(2)
}

fn describe(status: &ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;

    if let Some(signal) = status.signal() {
        format!("was killed by signal {signal}")
    } else if let Some(code) = status.code() {
        format!("stopped (status {code})")
    } else {
        "stopped without an exit status".to_owned()
    }
}

fn sleep_unless_stopped(duration: Duration) {
    let deadline = Instant::now() + duration;

    while Instant::now() < deadline {
        if STOP.load(Ordering::SeqCst) {
            return;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());

        std::thread::sleep(POLL.min(remaining));
    }
}

/// Wait for the child's own shutdown, then make the stop unavoidable.  A module
/// can be stuck in code that never returns to the main loop; a kill of the main
/// process must not hang the supervisor forever.
fn wait_for_stop(child: &mut std::process::Child) {
    let deadline = Instant::now() + STOP_GRACE;

    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => std::thread::sleep(POLL),
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_restart_delay_matches_the_c_shell() {
        assert_eq!(restart_delay(1), 1);
        assert_eq!(restart_delay(2), 2);
        assert_eq!(restart_delay(3), 4);
        assert_eq!(restart_delay(8), 4);
    }

    #[test]
    fn a_long_run_is_stable_but_a_rapid_failure_is_not() {
        assert!(STABLE_RUN > Duration::from_secs(29));
        assert_eq!(MAX_RESTARTS, 5);
    }
}

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

use crate::posix::{TERMINATION_SIGNALS, install_signal_handler, process_id, signal_process};

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
        // The child owns its own process group, so this reaches only the UI,
        // never an application a module started.
        signal_process(child, signal);
    }
}

fn install_signals() -> Result<(), String> {
    install_signal_handler(on_signal, &TERMINATION_SIGNALS)
        .map_err(|error| format!("could not install termination handlers: {error}"))
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

/// What one UI process did, independent of `std::process::ExitStatus`'s
/// platform representation.  Keeping this small makes the restart policy pure
/// and testable without spawning a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Success,
    NotFound,
    Failed {
        signal: Option<i32>,
        code: Option<i32>,
    },
}

impl Outcome {
    fn from_status(status: &ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt;

        match (status.code(), status.signal()) {
            (Some(0), _) => Self::Success,
            (Some(127), _) => Self::NotFound,
            (code, signal) => Self::Failed { signal, code },
        }
    }

    fn describe(self) -> String {
        match self {
            Self::Success => "exited successfully".to_owned(),
            Self::NotFound => "could not be executed".to_owned(),
            Self::Failed {
                signal: Some(signal),
                ..
            } => format!("was killed by signal {signal}"),
            Self::Failed {
                code: Some(code), ..
            } => format!("stopped (status {code})"),
            Self::Failed { .. } => "stopped without an exit status".to_owned(),
        }
    }
}

/// What the supervisor should do after one child exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Exit(i32),
    Restart { failures: u32, delay: u64 },
}

/// The whole restart policy.  A successful child ends the supervisor; a
/// missing executable is not retried; a rapid failure increments the incident
/// count, while a run that lasted [`STABLE_RUN`] starts a fresh incident.
fn decide(outcome: Outcome, elapsed: Duration, failures: u32) -> Decision {
    match outcome {
        Outcome::Success => Decision::Exit(0),
        Outcome::NotFound => Decision::Exit(127),
        Outcome::Failed { .. } => {
            let failures = if elapsed > STABLE_RUN {
                1
            } else {
                failures.saturating_add(1)
            };

            if failures >= MAX_RESTARTS {
                Decision::Exit(1)
            } else {
                Decision::Restart {
                    failures,
                    delay: restart_delay(failures),
                }
            }
        }
    }
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
    let parent = process_id();

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
            signal_process(pid, libc::SIGTERM);
        }

        // `pre_exec` sets the group in the child; doing it again here closes
        // the race where a signal arrives before the child reaches that code.
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

        let outcome = Outcome::from_status(&status);

        match decide(outcome, started.elapsed(), failures) {
            Decision::Exit(code) => {
                if code == 1 {
                    eprintln!(
                        "whelk: the UI stopped {MAX_RESTARTS} times; leaving it stopped so a broken \
                         configuration is not restarted forever (WHELK_NO_SUPERVISE=1 runs without \
                         the supervisor)"
                    );
                }

                return Ok(Some(code));
            }
            Decision::Restart {
                failures: next_failures,
                delay,
            } => {
                failures = next_failures;
                eprintln!(
                    "whelk: the UI {}; restarting in {delay} s",
                    outcome.describe()
                );

                sleep_unless_stopped(Duration::from_secs(delay));
            }
        }
    }
}

/// The C shell's delay: one, two, then at most four seconds.
fn restart_delay(failures: u32) -> u64 {
    1u64 << failures.saturating_sub(1).min(2)
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

    fn failed(code: i32) -> Outcome {
        Outcome::Failed {
            signal: None,
            code: Some(code),
        }
    }

    #[test]
    fn outcomes_distinguish_success_missing_and_crashes() {
        use std::os::unix::process::ExitStatusExt;

        assert_eq!(
            Outcome::from_status(&ExitStatus::from_raw(0)),
            Outcome::Success
        );
        assert_eq!(
            Outcome::from_status(&ExitStatus::from_raw(127 << 8)),
            Outcome::NotFound
        );
        assert_eq!(
            Outcome::from_status(&ExitStatus::from_raw(9)),
            Outcome::Failed {
                signal: Some(9),
                code: None,
            }
        );
    }

    #[test]
    fn success_and_a_missing_executable_are_not_restarted() {
        assert_eq!(
            decide(Outcome::Success, Duration::ZERO, 0),
            Decision::Exit(0)
        );
        assert_eq!(
            decide(Outcome::NotFound, Duration::ZERO, 0),
            Decision::Exit(127)
        );
    }

    #[test]
    fn rapid_failures_back_off_then_give_up() {
        assert_eq!(
            decide(failed(1), Duration::ZERO, 0),
            Decision::Restart {
                failures: 1,
                delay: 1,
            }
        );
        assert_eq!(
            decide(failed(1), Duration::from_secs(1), 1),
            Decision::Restart {
                failures: 2,
                delay: 2,
            }
        );
        assert_eq!(
            decide(failed(1), Duration::from_secs(1), 4),
            Decision::Exit(1)
        );
    }

    #[test]
    fn a_long_run_starts_a_new_incident() {
        assert_eq!(
            decide(failed(1), STABLE_RUN + Duration::from_secs(1), 4),
            Decision::Restart {
                failures: 1,
                delay: 1,
            }
        );
    }

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

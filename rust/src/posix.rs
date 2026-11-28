// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The small POSIX boundary the shell needs.
//!
//! Keeping these calls here gives the rest of the crate a safe, documented
//! boundary instead of repeating `sigaction` setup in every process that
//! installs a handler.  The handlers themselves stay `extern "C"`: they are
//! called by the kernel, not by Rust.

use std::io;
use std::process::Command;

/// The signals a terminal, service manager or logout sends a desktop process.
pub(crate) const TERMINATION_SIGNALS: [libc::c_int; 3] =
    [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

/// Install `handler` for every signal in `signals`.
///
/// No `SA_RESTART` is requested: a blocking wait must wake up and see the
/// `AtomicBool` the handler sets, rather than resume as if nothing happened.
pub(crate) fn install_signal_handler(
    handler: extern "C" fn(libc::c_int),
    signals: &[libc::c_int],
) -> io::Result<()> {
    // SAFETY: `sigaction` is a plain C struct for which all-zero is a valid
    // initial value; every field used below is assigned before `sigaction` is
    // called.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };

    action.sa_sigaction = handler as *const () as usize;
    action.sa_flags = 0;

    // SAFETY: `action.sa_mask` points to the initialized signal set inside
    // `action`, and `sigemptyset` initializes it in place.
    if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
        return Err(io::Error::last_os_error());
    }

    for &signal in signals {
        // SAFETY: `action` is fully initialized above and remains alive for
        // the call; the old-action pointer is deliberately null.
        if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Send `signal` to `pid`, ignoring a process that has already exited.
///
/// This is used from a signal handler, so it deliberately does not allocate or
/// build an `io::Error`; the caller has no useful recovery path anyway.
pub(crate) fn signal_process(pid: libc::pid_t, signal: libc::c_int) {
    // SAFETY: `kill` takes a pid and a signal number and has no pointer
    // arguments.  It is async-signal-safe.
    unsafe {
        libc::kill(pid, signal);
    }
}

/// This process's identifier.  `getpid` cannot fail.
pub(crate) fn process_id() -> libc::pid_t {
    // SAFETY: `getpid` has no arguments and cannot fail.
    unsafe { libc::getpid() }
}

/// Ask Linux to terminate `command` when this process exits.
///
/// The watcher is a child, but the lock screens it starts are not: a shell
/// restart must not leave a second watcher behind, and a running lock screen
/// must outlive the shell that asked for it.
pub(crate) fn die_with_parent(command: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: the closure runs after `fork' in the child.  `prctl' is
        // async-signal-safe and the closure touches no Rust state or heap.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(io::Error::last_os_error());
                }

                Ok(())
            });
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The session's power verbs, from whatever the machine actually has.
//!
//! A shell runs on more than systemd.  whelk's own logind features already
//! speak D-Bus (`org.freedesktop.login1`), which elogind answers identically,
//! but a *command* a menu row or a click runs has to name a program: on Void
//! the same session may have `loginctl` from elogind, `systemctl` from systemd,
//! or neither, and suspend may come from `zzz` instead.  Every verb here is one
//! shell chain that tries the session manager first and the plain tools last,
//! so a machine without systemd still sleeps and shuts down.
//!
//! The chains are data, not policies of their own: the shell menu, the
//! launcher's session mode and the Scheme defaults all read them from here, so
//! there is one answer to "how does this machine power off" rather than three.

/// One verb a session menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lock,
    LogOut,
    Suspend,
    Hibernate,
    Restart,
    ShutDown,
}

impl Action {
    /// The command line the row runs.
    ///
    /// `loginctl` answers on both systemd and elogind, and it is the same
    /// client on both; `systemctl` is kept behind it for a systemd session with
    /// a broken bus.  The last link of each chain is a plain tool
    /// (`zzz`, `ZZZ`, `reboot`, `poweroff`) so a machine with neither manager
    /// still has a way out.  Each failure is quiet: a missing program is the
    /// normal answer on the machines that do not have it.
    pub fn command(self) -> &'static str {
        match self {
            // The lock is whelk's own, not the session's.
            Self::Lock => "whelk --lock",
            // A session without logind has no portable "end my session": the
            // compositor owns it, so the chain stops at the two loginctl
            // verbs rather than guessing at a process to signal.
            Self::LogOut => {
                "loginctl terminate-session \"${XDG_SESSION_ID:-}\" 2>/dev/null || \
                 loginctl terminate-user \"$USER\" 2>/dev/null"
            }
            Self::Suspend => "loginctl suspend 2>/dev/null || systemctl suspend 2>/dev/null || zzz",
            Self::Hibernate => {
                "loginctl hibernate 2>/dev/null || systemctl hibernate 2>/dev/null || ZZZ"
            }
            Self::Restart => {
                "loginctl reboot 2>/dev/null || systemctl reboot 2>/dev/null || reboot"
            }
            Self::ShutDown => {
                "loginctl poweroff 2>/dev/null || systemctl poweroff 2>/dev/null || poweroff"
            }
        }
    }

    /// The verb, as a log line and a test name spell it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::LogOut => "log-out",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::Restart => "restart",
            Self::ShutDown => "shut-down",
        }
    }
}

/// The order a session menu lists the ways out, after the lock.
pub const MENU: [Action; 5] = [
    Action::Suspend,
    Action::Hibernate,
    Action::Restart,
    Action::ShutDown,
    Action::LogOut,
];

#[cfg(test)]
mod tests {
    use super::{Action, MENU};

    #[test]
    fn every_verb_tries_the_session_manager_before_the_plain_tool() {
        for action in MENU {
            let command = action.command();

            assert!(
                !command.trim_start().starts_with("systemctl"),
                "{}: systemd is the second attempt, not the first: {command}",
                action.name()
            );
        }

        assert!(Action::Suspend.command().starts_with("loginctl suspend"));
        assert!(Action::Suspend.command().ends_with("zzz"));
        assert!(Action::Hibernate.command().ends_with("ZZZ"));
        assert!(Action::Restart.command().ends_with("reboot"));
        assert!(Action::ShutDown.command().ends_with("poweroff"));
    }

    #[test]
    fn a_machine_without_systemd_still_has_a_whole_chain() {
        // Each manager-first attempt is followed by at least one other way, so
        // a Void or Alpine session is not left without a row that works.
        for action in [
            Action::Suspend,
            Action::Hibernate,
            Action::Restart,
            Action::ShutDown,
        ] {
            let links: Vec<&str> = action.command().split("||").map(str::trim).collect();

            assert!(
                links.len() >= 2,
                "{} has one way only: {}",
                action.name(),
                action.command()
            );
        }
    }

    #[test]
    fn logout_does_not_guess_at_a_process_to_signal() {
        let command = Action::LogOut.command();

        assert!(command.contains("loginctl terminate-session"));
        assert!(command.contains("loginctl terminate-user"));
        assert!(
            !command.contains("pkill") && !command.contains("kill -"),
            "ending a session by signal can take the wrong session with it"
        );
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The command line, ported from `src/main.c`.
//!
//! A keybinding starts a second `whelk`, but it must not start a second shell:
//! GApplication forwards the options to the running instance.  This module is
//! the display-free half of that boundary.  It turns an argument list into a
//! [`Requests`] value, and both the local `--check-*` paths and the running
//! shell read the same encoding through [`Request`].
//!
//! The order in [`Requests::commands`] is the C handler's order.  A quit wins
//! over everything else; a menu wins over the launcher; the clipboard mode and
//! the launcher are alternatives, not two overlays.  Keeping that here means a
//! test can prove it without a session bus or a display.

use std::path::PathBuf;

/// One thing the running shell should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Quit,
    Reload,
    Settings,
    About,
    Lock,
    Menu(String),
    SessionMenu,
    Launcher(Option<String>),
    Clipboard,
    ClipboardStatus,
    ClipboardPause,
    ClipboardClear,
    ClipboardClearUnpinned,
    ClipboardRestart,
    WallpaperPicker,
    Notifications,
    Dnd,
    NightLight,
    Inhibit(Option<String>),
    IdleStatus,
    Network(Option<String>),
}

/// Everything the C command line can ask for, already separated by meaning.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Requests {
    pub quit: bool,
    pub reload: bool,
    pub settings: bool,
    pub about: bool,
    pub lock: bool,
    pub menu: Option<String>,
    pub session_menu: bool,
    pub launcher: bool,
    pub launcher_mode: Option<String>,
    pub clipboard: bool,
    pub clipboard_status: bool,
    pub clipboard_pause: bool,
    pub clipboard_clear: bool,
    pub clipboard_clear_unpinned: bool,
    pub clipboard_restart: bool,
    pub wallpaper_picker: bool,
    pub notifications: bool,
    pub dnd: bool,
    pub night_light: bool,
    pub inhibit: bool,
    pub inhibit_reason: Option<String>,
    pub idle_status: bool,
    pub network: Option<String>,
}

impl Requests {
    /// The requests in the order `src/main.c` dispatches them.  A quit ends the
    /// list; every other request may be combined, as it can in C.
    pub fn commands(&self) -> Vec<Request> {
        let mut commands = Vec::new();

        if self.quit {
            commands.push(Request::Quit);

            return commands;
        }

        if self.reload {
            commands.push(Request::Reload);
        }

        if self.settings {
            commands.push(Request::Settings);
        }

        if self.about {
            commands.push(Request::About);
        }

        if self.lock {
            commands.push(Request::Lock);
        }

        if let Some(menu) = self.menu.as_ref().filter(|menu| !menu.is_empty()) {
            commands.push(Request::Menu(menu.clone()));
        } else if self.session_menu {
            commands.push(Request::SessionMenu);
        } else if self.launcher {
            commands.push(Request::Launcher(self.launcher_mode.clone()));
        } else if self.clipboard {
            commands.push(Request::Clipboard);
        }

        if self.clipboard_status {
            commands.push(Request::ClipboardStatus);
        }

        if self.clipboard_pause {
            commands.push(Request::ClipboardPause);
        }

        if self.clipboard_clear {
            commands.push(Request::ClipboardClear);
        }

        if self.clipboard_clear_unpinned {
            commands.push(Request::ClipboardClearUnpinned);
        }

        if self.clipboard_restart {
            commands.push(Request::ClipboardRestart);
        }

        if self.wallpaper_picker {
            commands.push(Request::WallpaperPicker);
        }

        if self.notifications {
            commands.push(Request::Notifications);
        }

        if self.dnd {
            commands.push(Request::Dnd);
        }

        if self.night_light {
            commands.push(Request::NightLight);
        }

        if self.inhibit {
            commands.push(Request::Inhibit(self.inhibit_reason.clone()));
        }

        if self.idle_status {
            commands.push(Request::IdleStatus);
        }

        if let Some(page) = self.network.as_ref().filter(|page| !page.is_empty()) {
            commands.push(Request::Network(Some(page.clone())));
        }

        commands
    }
}

/// The options GApplication must know about before it can forward a second
/// invocation.  `None` means a flag; `Some` is the metavar for a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    pub long: &'static str,
    pub short: Option<char>,
    pub value: Option<&'static str>,
    pub description: &'static str,
}

pub const FORWARDED_OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        long: "config",
        short: None,
        value: Some("PATH"),
        description: "Read this settings file",
    },
    OptionSpec {
        long: "about",
        short: None,
        value: None,
        description: "Show the About whelk window",
    },
    OptionSpec {
        long: "launcher",
        short: Some('l'),
        value: None,
        description: "Show the launcher overlay",
    },
    OptionSpec {
        long: "launcher-mode",
        short: None,
        value: Some("MODE"),
        description: "Launcher mode: app, window, run, calc, file, web, menu, ssh, quit, clipboard, menuitem",
    },
    OptionSpec {
        long: "clipboard",
        short: None,
        value: None,
        description: "Show the clipboard history",
    },
    OptionSpec {
        long: "clipboard-status",
        short: None,
        value: None,
        description: "Print clipboard availability, count and size",
    },
    OptionSpec {
        long: "clipboard-pause",
        short: None,
        value: None,
        description: "Pause or resume clipboard capture",
    },
    OptionSpec {
        long: "clipboard-clear",
        short: None,
        value: None,
        description: "Delete the whole clipboard history",
    },
    OptionSpec {
        long: "clipboard-clear-unpinned",
        short: None,
        value: None,
        description: "Delete every clipboard entry that is not pinned",
    },
    OptionSpec {
        long: "clipboard-restart",
        short: None,
        value: None,
        description: "Restart the clipboard listener",
    },
    OptionSpec {
        long: "menu",
        short: Some('m'),
        value: Some("COMMAND"),
        description: "Show a menu from a command printing label<TAB>command lines",
    },
    OptionSpec {
        long: "session-menu",
        short: None,
        value: None,
        description: "Show the built-in session menu",
    },
    OptionSpec {
        long: "settings",
        short: None,
        value: None,
        description: "Open the whelk settings window",
    },
    OptionSpec {
        long: "wallpaper-picker",
        short: None,
        value: None,
        description: "Show the wallpaper picker",
    },
    OptionSpec {
        long: "notifications",
        short: Some('n'),
        value: None,
        description: "Open or close the notification centre",
    },
    OptionSpec {
        long: "dnd",
        short: None,
        value: None,
        description: "Turn do-not-disturb on or off",
    },
    OptionSpec {
        long: "night-light",
        short: None,
        value: None,
        description: "Turn the night light on or off",
    },
    OptionSpec {
        long: "inhibit",
        short: None,
        value: None,
        description: "Take or release whelk's logind idle inhibitor",
    },
    OptionSpec {
        long: "inhibit-reason",
        short: None,
        value: Some("REASON"),
        description: "Why the inhibitor is held",
    },
    OptionSpec {
        long: "idle-status",
        short: None,
        value: None,
        description: "Explain the idle timer, whelk's inhibitor and DND",
    },
    OptionSpec {
        long: "network",
        short: None,
        value: Some("PAGE"),
        description: "Show the network panel: wifi or bluetooth",
    },
    OptionSpec {
        long: "lock",
        short: None,
        value: None,
        description: "Lock the screen and require the session password",
    },
    OptionSpec {
        long: "reload",
        short: Some('r'),
        value: None,
        description: "Re-read the settings file",
    },
    OptionSpec {
        long: "quit",
        short: Some('q'),
        value: None,
        description: "Stop the running shell",
    },
];

/// The whole parsed command line.  The `local` fields exit before GTK exists;
/// `requests` is what the running application performs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    pub config: Option<PathBuf>,
    pub help: bool,
    pub version: bool,
    pub check_config: bool,
    pub check_lock: bool,
    /// The internal entry point the shell's logind trigger starts.  A public
    /// `--lock` is forwarded to the running shell, as C's is.
    pub lock_now: bool,
    pub requests: Requests,
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut arguments = arguments.peekable();

        while let Some(argument) = arguments.next() {
            if let Some(body) = argument.strip_prefix("--") {
                let (name, joined) = match body.split_once('=') {
                    Some((name, value)) => (name, Some(value.to_owned())),
                    None => (body, None),
                };

                if options.apply_long(name, joined, &mut arguments)? {
                    continue;
                }

                return Err(format!("unknown option --{name}"));
            }

            if let Some(body) = argument.strip_prefix('-').filter(|body| !body.is_empty()) {
                if options.apply_short(body, &mut arguments)? {
                    continue;
                }

                return Err(format!("unknown option -{body}"));
            }

            return Err(format!("unexpected argument {argument}"));
        }

        Ok(options)
    }

    /// Apply one `--name` (or `--name=value`); true when it was known.
    fn apply_long(
        &mut self,
        name: &str,
        joined: Option<String>,
        arguments: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        match name {
            "help" => self.help = true,
            "version" => self.version = true,
            "check-config" => self.check_config = true,
            "check-lock" => self.check_lock = true,
            "lock-now" => self.lock_now = true,
            "config" => {
                let path = value(joined, arguments, name)?;

                if path.is_empty() {
                    return Err("--config needs a path".to_owned());
                }

                self.config = Some(PathBuf::from(path));
            }
            "launcher" => self.requests.launcher = true,
            "launcher-mode" => {
                self.requests.launcher_mode = Some(value(joined, arguments, name)?);
            }
            "clipboard" => self.requests.clipboard = true,
            "clipboard-status" => self.requests.clipboard_status = true,
            "clipboard-pause" => self.requests.clipboard_pause = true,
            "clipboard-clear" => self.requests.clipboard_clear = true,
            "clipboard-clear-unpinned" => self.requests.clipboard_clear_unpinned = true,
            "clipboard-restart" => self.requests.clipboard_restart = true,
            "menu" => self.requests.menu = Some(value(joined, arguments, name)?),
            "session-menu" => self.requests.session_menu = true,
            "settings" => self.requests.settings = true,
            "about" => self.requests.about = true,
            "wallpaper-picker" => self.requests.wallpaper_picker = true,
            "notifications" => self.requests.notifications = true,
            "dnd" => self.requests.dnd = true,
            "night-light" => self.requests.night_light = true,
            "inhibit" => self.requests.inhibit = true,
            "inhibit-reason" => {
                self.requests.inhibit_reason = Some(value(joined, arguments, name)?);
            }
            "idle-status" => self.requests.idle_status = true,
            "network" => self.requests.network = Some(value(joined, arguments, name)?),
            "lock" => self.requests.lock = true,
            "reload" => self.requests.reload = true,
            "quit" => self.requests.quit = true,
            _ => return Ok(false),
        }

        Ok(true)
    }

    fn apply_short(
        &mut self,
        body: &str,
        arguments: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        let mut characters = body.chars();

        while let Some(character) = characters.next() {
            match character {
                'h' => self.help = true,
                'V' | 'v' => self.version = true,
                'l' => self.requests.launcher = true,
                'n' => self.requests.notifications = true,
                'r' => self.requests.reload = true,
                'q' => self.requests.quit = true,
                'm' => {
                    let rest: String = characters.collect();
                    let joined = (!rest.is_empty()).then_some(rest);

                    self.requests.menu = Some(value(joined, arguments, "menu")?);

                    return Ok(true);
                }
                _ => return Ok(false),
            }
        }

        Ok(true)
    }
}

fn value(
    joined: Option<String>,
    arguments: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, String> {
    joined
        .or_else(|| arguments.next())
        .ok_or_else(|| format!("--{name} needs a value"))
}

#[cfg(test)]
mod tests {
    use super::{Options, Request, Requests};

    fn parse(arguments: &[&str]) -> Options {
        Options::parse(arguments.iter().map(|argument| (*argument).to_owned()))
            .expect("parse options")
    }

    #[test]
    fn every_forwarded_option_is_accepted() {
        for option in super::FORWARDED_OPTIONS {
            let mut arguments = vec![format!("--{}", option.long)];

            if let Some(value) = option.value {
                arguments.push(value.to_owned());
            }

            let parsed = Options::parse(arguments.into_iter());
            assert!(parsed.is_ok(), "--{} was rejected", option.long);
        }
    }

    #[test]
    fn short_options_follow_the_c_shell() {
        let options = parse(&["-l", "-n", "-r", "-q"]);

        assert!(options.requests.launcher);
        assert!(options.requests.notifications);
        assert!(options.requests.reload);
        assert!(options.requests.quit);
    }

    #[test]
    fn values_are_split_or_joined() {
        assert_eq!(
            parse(&["--config", "/tmp/a.scm"]).config,
            Some("/tmp/a.scm".into())
        );
        assert_eq!(
            parse(&["--config=/tmp/a.scm"]).config,
            Some("/tmp/a.scm".into())
        );
        assert_eq!(
            parse(&["-m", "menu-command"]).requests.menu.as_deref(),
            Some("menu-command")
        );
        assert_eq!(
            parse(&["-mmenu-command"]).requests.menu.as_deref(),
            Some("menu-command")
        );
        assert_eq!(
            parse(&["--network", "wifi"]).requests.network.as_deref(),
            Some("wifi")
        );
    }

    #[test]
    fn a_quit_ends_the_dispatch() {
        let requests = Requests {
            quit: true,
            reload: true,
            notifications: true,
            ..Requests::default()
        };

        assert_eq!(requests.commands(), vec![Request::Quit]);
    }

    #[test]
    fn menu_launcher_and_clipboard_are_alternatives() {
        let requests = Requests {
            menu: Some("power-menu".to_owned()),
            launcher: true,
            clipboard: true,
            ..Requests::default()
        };
        assert_eq!(
            requests.commands(),
            vec![Request::Menu("power-menu".to_owned())]
        );

        let requests = Requests {
            launcher: true,
            launcher_mode: Some("window".to_owned()),
            clipboard: true,
            ..Requests::default()
        };
        assert_eq!(
            requests.commands(),
            vec![Request::Launcher(Some("window".to_owned()))]
        );
    }

    #[test]
    fn the_status_flags_survive_the_shared_order() {
        let requests = Requests {
            clipboard_status: true,
            idle_status: true,
            network: Some("bluetooth".to_owned()),
            ..Requests::default()
        };

        assert_eq!(
            requests.commands(),
            vec![
                Request::ClipboardStatus,
                Request::IdleStatus,
                Request::Network(Some("bluetooth".to_owned())),
            ]
        );
    }

    #[test]
    fn local_options_do_not_need_a_shell() {
        let options = parse(&["--check-lock", "--check-config", "--version", "-h"]);

        assert!(options.check_lock);
        assert!(options.check_config);
        assert!(options.version);
        assert!(options.help);
        assert!(options.requests.commands().is_empty());
    }

    #[test]
    fn the_internal_lock_entry_is_local() {
        let options = parse(&["--lock-now"]);

        assert!(options.lock_now);
        assert!(options.requests.commands().is_empty());
        assert!(parse(&["--lock"]).requests.lock);
    }

    #[test]
    fn unknown_or_incomplete_options_are_errors() {
        assert!(Options::parse(["--nope".to_owned()].into_iter()).is_err());
        assert!(Options::parse(["--config".to_owned()].into_iter()).is_err());
        assert!(Options::parse(["--config=".to_owned()].into_iter()).is_err());
        assert!(Options::parse(["--menu".to_owned()].into_iter()).is_err());
        assert!(Options::parse(["stray".to_owned()].into_iter()).is_err());
    }
}

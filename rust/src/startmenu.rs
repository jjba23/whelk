// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The shell's own menu, as a value rather than a widget tree.
//!
//! A settings file writes the menu behind the mark at the far left of the bar
//! with `(whelk menu)` units.  The bridge hands those units over as ordinary
//! Scheme values, so this module owns the walk that turns them into rows:
//! labels, separators, submenus and the one submenu a shell fills itself when
//! it opens.  Everything here is plain data, which is also what makes the
//! parsing testable without Guile or a display.

use crate::settings::SchemeValue;

/// One row of the shell menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Separator,
    Item {
        label: String,
        /// `shell:name` for something the shell does itself, a command line for
        /// everything else.  `None` is the disabled row the C bridge builds for
        /// an item whose action was empty.
        action: Option<String>,
        enabled: bool,
    },
    Submenu {
        label: String,
        /// A slot the shell fills when the submenu opens, such as
        /// `shell:recent-items`.  Ordinary submenus have `None`.
        marker: Option<String>,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    /// Turn `(whelk menu)` units into rows.
    ///
    /// The tags are the ones `(whelk menu)` writes: `item`, `separator`, and
    /// `submenu` (with `menu` accepted as the older spelling the C bridge
    /// still accepts).  An item without a label is skipped; an item with an
    /// empty action is shown disabled, because a row that cannot say what it
    /// does should not pretend to.
    pub fn from_units(units: &[SchemeValue]) -> Vec<Self> {
        let mut entries = Vec::new();

        for unit in units {
            let SchemeValue::List(parts) = unit else {
                continue;
            };
            let Some(tag) = parts.first().and_then(text) else {
                continue;
            };

            match tag.as_str() {
                "separator" => entries.push(Self::Separator),
                "item" => {
                    let Some(label) = parts
                        .get(1)
                        .and_then(text)
                        .filter(|label| !label.is_empty())
                    else {
                        continue;
                    };
                    let action = parts
                        .get(2)
                        .and_then(text)
                        .filter(|action| !action.is_empty());

                    entries.push(Self::Item {
                        label,
                        enabled: action.is_some(),
                        action,
                    });
                }
                "submenu" | "menu" => {
                    let Some(label) = parts
                        .get(1)
                        .and_then(text)
                        .filter(|label| !label.is_empty())
                    else {
                        continue;
                    };
                    let body = parts.get(2..).unwrap_or(&[]);

                    if body.len() == 1
                        && let Some(marker) = text(&body[0]).filter(|marker| !marker.is_empty())
                    {
                        entries.push(Self::Submenu {
                            label,
                            marker: Some(marker),
                            entries: Vec::new(),
                        });

                        continue;
                    }

                    entries.push(Self::Submenu {
                        label,
                        marker: None,
                        entries: Self::from_units(body),
                    });
                }
                _ => {}
            }
        }

        entries
    }

    /// The first menu, or `None` when the root holds only separators and
    /// unreadable units.
    pub fn first(entries: &[Self]) -> Option<&Self> {
        entries
            .iter()
            .find(|entry| !matches!(entry, Self::Separator))
    }

    /// The menu the default configuration ships.  This is the same list as
    /// `(default-shell-menu)` in `scheme/whelk/defaults.scm`, kept here for the
    /// no-configuration path, where Guile is never initialized.
    pub fn default_menu() -> Vec<Self> {
        let user = std::env::var("USER").unwrap_or_default();
        let log_out = if user.is_empty() {
            "Log Out…".to_owned()
        } else {
            format!("Log Out {user}…")
        };

        vec![
            Self::item("About This System", "shell:about"),
            Self::item("About whelk", "shell:about-whelk"),
            Self::item("Control Panel", "shell:settings"),
            Self::item("Software…", "gnome-software"),
            Self::Separator,
            Self::Submenu {
                label: "Recent Items".to_owned(),
                marker: Some("shell:recent-items".to_owned()),
                entries: Vec::new(),
            },
            Self::Separator,
            Self::item("Force Quit…", "shell:force-quit"),
            Self::Separator,
            // The ways out come from [`crate::power`], so the default menu is
            // the same list on systemd, elogind and a machine with neither.
            Self::item("Sleep", crate::power::Action::Suspend.command()),
            Self::item("Restart…", crate::power::Action::Restart.command()),
            Self::item("Shut Down…", crate::power::Action::ShutDown.command()),
            Self::Separator,
            Self::item("Lock Screen", crate::power::Action::Lock.command()),
            Self::item(&log_out, crate::power::Action::LogOut.command()),
            Self::Separator,
            Self::item("Search…", "shell:launcher"),
            Self::Separator,
            Self::item("Reload settings", "whelk --reload"),
            Self::item("Quit whelk", "whelk --quit"),
        ]
    }

    /// The short menu a shell with no Scheme still has: a mark that opens
    /// nothing is worse than a short menu.
    pub fn fallback_menu() -> Vec<Self> {
        vec![
            Self::item("Search…", "shell:launcher"),
            Self::item("Notifications", "shell:notifications"),
            Self::Separator,
            Self::item("Reload settings", "whelk --reload"),
            Self::item("Quit whelk", "whelk --quit"),
        ]
    }

    fn item(label: &str, action: &str) -> Self {
        Self::Item {
            label: label.to_owned(),
            action: Some(action.to_owned()),
            enabled: true,
        }
    }
}

/// What a command line from the menu means to this process.
///
/// The default menu spells the two shell controls as `whelk --reload` and
/// `whelk --quit`, because that is how a user starts them from the C shell.
/// Spawning that command from inside the Rust shell would start a second
/// shell, so those two are routed to the running application instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Reload,
    Quit,
    Run(String),
}

pub fn command_action(command: &str) -> CommandAction {
    match command.trim() {
        "whelk --reload" => CommandAction::Reload,
        "whelk --quit" => CommandAction::Quit,
        _ => CommandAction::Run(command.to_owned()),
    }
}

fn text(value: &SchemeValue) -> Option<String> {
    match value {
        SchemeValue::Text(text) | SchemeValue::Symbol(text) => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandAction, MenuEntry, command_action};
    use crate::settings::SchemeValue;

    fn symbol(text: &str) -> SchemeValue {
        SchemeValue::Symbol(text.to_owned())
    }

    fn text(text: &str) -> SchemeValue {
        SchemeValue::Text(text.to_owned())
    }

    #[test]
    fn units_keep_their_order_and_disabled_rows() {
        let entries = MenuEntry::from_units(&[
            SchemeValue::List(vec![symbol("item"), text("Open"), text("xdg-open x")]),
            SchemeValue::List(vec![symbol("separator")]),
            SchemeValue::List(vec![symbol("item"), text("Nothing"), text("")]),
            SchemeValue::List(vec![symbol("item"), text(""), text("hidden")]),
        ]);

        assert_eq!(
            entries,
            vec![
                MenuEntry::Item {
                    label: "Open".to_owned(),
                    action: Some("xdg-open x".to_owned()),
                    enabled: true,
                },
                MenuEntry::Separator,
                MenuEntry::Item {
                    label: "Nothing".to_owned(),
                    action: None,
                    enabled: false,
                },
            ]
        );
    }

    #[test]
    fn submenus_distinguish_rows_from_slots() {
        let entries = MenuEntry::from_units(&[
            SchemeValue::List(vec![
                symbol("submenu"),
                text("Recent Items"),
                text("shell:recent-items"),
            ]),
            SchemeValue::List(vec![
                symbol("submenu"),
                text("More"),
                SchemeValue::List(vec![symbol("item"), text("One"), text("one")]),
                SchemeValue::List(vec![symbol("separator")]),
                SchemeValue::List(vec![symbol("item"), text("Two"), text("two")]),
            ]),
        ]);

        assert_eq!(
            entries,
            vec![
                MenuEntry::Submenu {
                    label: "Recent Items".to_owned(),
                    marker: Some("shell:recent-items".to_owned()),
                    entries: Vec::new(),
                },
                MenuEntry::Submenu {
                    label: "More".to_owned(),
                    marker: None,
                    entries: vec![
                        MenuEntry::Item {
                            label: "One".to_owned(),
                            action: Some("one".to_owned()),
                            enabled: true,
                        },
                        MenuEntry::Separator,
                        MenuEntry::Item {
                            label: "Two".to_owned(),
                            action: Some("two".to_owned()),
                            enabled: true,
                        },
                    ],
                },
            ]
        );
    }

    #[test]
    fn the_default_menu_covers_the_shell_actions() {
        let entries = MenuEntry::default_menu();
        let labels: Vec<&str> = entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, .. } | MenuEntry::Submenu { label, .. } => {
                    Some(label.as_str())
                }
                MenuEntry::Separator => None,
            })
            .collect();

        assert!(labels.contains(&"About This System"));
        assert!(labels.contains(&"Recent Items"));
        assert!(labels.contains(&"Quit whelk"));
    }

    #[test]
    fn reload_and_quit_stay_in_this_process() {
        assert_eq!(command_action("whelk --reload"), CommandAction::Reload);
        assert_eq!(command_action(" whelk --quit "), CommandAction::Quit);
        assert_eq!(
            command_action("systemctl suspend"),
            CommandAction::Run("systemctl suspend".to_owned())
        );
    }
}

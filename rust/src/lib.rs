// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! whelk's Rust shell.
//!
//! The crate is the whole installed shell: a GTK4 layer-shell surface set, a
//! small audited Guile boundary, the process supervisor and the policy modules
//! behind every bar reading.  The C implementation has been removed; the
//! configuration language and its data files are unchanged.

#![deny(unsafe_op_in_unsafe_fn)]

mod posix;

pub mod a11y;
pub mod about;
pub mod applets;
pub mod atspi;
pub mod audiobar;
pub mod autohide;
pub mod backlightbar;
pub mod bar;
pub mod barfactory;
pub mod barmodule;
pub mod barmonitors;
pub mod batterybar;
pub mod bluetoothbar;
pub mod calc;
pub mod characterbar;
pub mod chord;
pub mod cli;
pub mod clipboard;
pub mod clipboardbar;
pub mod clockbar;
pub mod collection;
pub mod command;
pub mod config;
pub mod cpubar;
pub mod custombar;
pub mod desktop;
pub mod desktopmenu;
pub mod dndbar;
pub mod filesearch;
pub mod format;
pub mod frost;
pub mod fuzzy;
pub mod globalmenu;
pub mod guile;
pub mod hasp;
pub mod hostinfo;
pub mod idle;
pub mod idlepolicy;
pub mod inhibitbar;
pub mod inhibitors;
pub mod json;
pub mod keyboardbar;
pub mod languagebar;
pub mod launcher;
pub mod launcherdata;
pub mod launcherkeys;
pub mod launcherpanel;
pub mod level;
pub mod logind;
pub mod memorybar;
pub mod menuapps;
pub mod menubar;
pub mod menuprobe;
pub mod menusearch;
pub mod menuwatch;
pub mod mpris;
pub mod mprisbar;
pub mod networkpanel;
pub mod nightlight;
pub mod nmcli;
pub mod notificationcenter;
pub mod notifications;
pub mod notificationsbar;
pub mod osd;
pub mod power;
pub mod powerprofilebar;
pub mod preferences;
pub mod riverstatus;
pub mod rotation;
pub mod schemebar;
pub mod session;
pub mod settings;
pub mod shortcutmaps;
pub mod signals;
pub mod sources;
pub mod startbar;
pub mod startmenu;
pub mod status;
pub mod style;
pub mod supervisor;
pub mod taskbar;
pub mod tray;
pub mod traybar;
pub mod trayicon;
pub mod traymenu;
pub mod ui;
pub mod visualizer;
pub mod wallpaper;
pub mod wallpaper_surface;
pub mod wallpaper_view;
pub mod wallpaperbar;
pub mod wallpapermenu;
pub mod wallpaperpicker;
pub mod wifibar;
pub mod windowbar;
pub mod workspacebar;
pub mod workspaces;

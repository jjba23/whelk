// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Rust half of whelk's migration.
//!
//! The C shell still builds and runs.  This crate starts the replacement from
//! the inside out: pure policy that can be tested without a display, a small
//! audited Guile boundary, and a GTK4 bootstrap that presents the shell's first
//! Rust surface.  Each layer has a stable Rust API so a later commit can move
//! callers over without rewriting the logic again.

pub mod applets;
pub mod audiobar;
pub mod backlightbar;
pub mod bar;
pub mod barfactory;
pub mod barmodule;
pub mod barmonitors;
pub mod batterybar;
pub mod bluetoothbar;
pub mod calc;
pub mod chord;
pub mod clockbar;
pub mod collection;
pub mod command;
pub mod config;
pub mod cpubar;
pub mod custombar;
pub mod dndbar;
pub mod filesearch;
pub mod format;
pub mod fuzzy;
pub mod guile;
pub mod hostinfo;
pub mod idle;
pub mod idlepolicy;
pub mod inhibitbar;
pub mod inhibitors;
pub mod json;
pub mod keyboardbar;
pub mod languagebar;
pub mod launcher;
pub mod launcherkeys;
pub mod level;
pub mod lockauth;
pub mod locker;
pub mod lockpolicy;
pub mod logind;
pub mod memorybar;
pub mod menuprobe;
pub mod nightlight;
pub mod nmcli;
pub mod notifications;
pub mod notificationsbar;
pub mod osd;
pub mod powerprofilebar;
pub mod rotation;
pub mod session;
pub mod settings;
pub mod style;
pub mod trayicon;
pub mod ui;
pub mod wallpaper;
pub mod wallpaper_surface;
pub mod wallpaper_view;
pub mod wifibar;
pub mod windowbar;
pub mod workspacebar;
pub mod workspaces;

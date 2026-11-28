// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The logind D-Bus names whelk reads.
//!
//! Locking is hasp's: its `logind` module subscribes to `Lock`,
//! `PrepareForSleep` and the session's `Active=false`, and takes the sleep
//! delay.  The shell keeps only the names its own work needs -- the inhibitor
//! it holds, the session it reports, and the property change that clears the
//! clipboard on lock.

pub const LOGIND_NAME: &str = "org.freedesktop.login1";
pub const MANAGER_PATH: &str = "/org/freedesktop/login1";
pub const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
pub const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
/// `PropertiesChanged` is emitted by the standard properties interface, not by
/// the session interface whose property changed.
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

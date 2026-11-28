// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! Floating desktop widget cards.
//!
//! `bibelot` owns the widget catalogue, the readings behind each kind, the
//! placement and state file, and the GTK4 surfaces that draw the cards.  A host
//! shell supplies the application, the card's generated skin and the settings
//! file path; this crate supplies everything else.
//!
//! The card's look is configurable from Scheme: `(bibelot theme)` returns the
//! small set of structural values, and `bibelot::Theme` is the Rust side of the
//! same contract.  A host that already runs Guile can read that module and pass
//! the resulting [`Theme`] in; [`theme::Guile`] initializes a runtime for a host
//! that does not.

pub mod batterybar;
pub mod command;
pub mod cpubar;
pub mod desktopwidgets;
pub mod format;
pub mod frost;
pub mod guile;
pub mod json;
pub mod memorybar;
pub mod mpris;
pub mod skin;
pub mod sources;
pub mod style;
pub mod theme;
pub mod visualizer;
pub mod widgets;
pub mod widgetstate;

pub use desktopwidgets::Widgets;
pub use skin::Skin;
pub use theme::Theme;
pub use widgets::{WidgetConfig, WidgetKind};

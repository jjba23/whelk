// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! WheLK's half of the standalone launcher's host boundary.
//!
//! The launcher crate owns the overlay and its search.  This adapter gives it
//! the shell's clipboard history and focused menu snapshot, and turns the first
//! bar's palette into the launcher's generated skin.

use std::rc::Rc;

use crate::bar::BarSettings;
use crate::clipboard::{self, Clipboard};
use crate::menusearch;
use crate::style;
use whelk_launcher::host::{
    ClipboardPort, ClipboardRow, ClipboardStatus, LauncherHost, MenuRow, MenuSnapshot,
};
use whelk_launcher::style::{LauncherSettings, LauncherStyle};

pub struct WhelkLauncherHost {
    clipboard: Option<Clipboard>,
}

impl WhelkLauncherHost {
    pub fn new(clipboard: Option<Clipboard>) -> Self {
        Self { clipboard }
    }
}

impl LauncherHost for WhelkLauncherHost {
    fn css(&self, style: &LauncherStyle, widget_id: &str) -> String {
        let mut bar = BarSettings {
            foreground: style.foreground.clone(),
            background: style.background.clone(),
            accent: style.accent.clone(),
            border_color: style.border_color.clone(),
            radius: style.radius.max(16),
            opacity: style.opacity,
            blur: style.blur,
            shine: style.shine,
            reflection: style.reflection,
            flare: style.flare,
            font: style.font.clone(),
            font_size: style.font_size.clone(),
            icon_font: style.icon_font.clone(),
            css: style.css.clone(),
            ..BarSettings::default()
        };

        bar.radius = bar.radius.max(16);

        style::css(widget_id, &bar, "whelk-launcher", None)
    }

    fn clipboard(&self) -> Option<Rc<dyn ClipboardPort>> {
        self.clipboard
            .clone()
            .map(|clipboard| Rc::new(clipboard) as Rc<dyn ClipboardPort>)
    }

    fn menu(&self) -> MenuSnapshot {
        let snapshot = menusearch::snapshot();
        let rows = snapshot
            .items
            .iter()
            .enumerate()
            .map(|(id, item)| MenuRow {
                id,
                label: item.label.clone(),
                path: item.path.clone(),
                accel: item.accel.clone(),
            })
            .collect();

        MenuSnapshot {
            app_name: snapshot.app_name,
            rows,
        }
    }

    fn activate_menu(&self, id: usize) {
        if let Some(item) = menusearch::snapshot().items.get(id) {
            menusearch::activate(item);
        }
    }
}

impl ClipboardPort for Clipboard {
    fn rows(&self) -> Vec<ClipboardRow> {
        self.entries()
            .into_iter()
            .map(|entry| ClipboardRow {
                id: entry.id.clone(),
                label: clipboard::entry_label(&entry),
                detail: clipboard::entry_detail(&entry),
                preview: clipboard::thumbnail_png(&entry),
                pinned: entry.pinned,
            })
            .collect()
    }

    fn status(&self) -> ClipboardStatus {
        match self.status() {
            clipboard::Status::Ready => ClipboardStatus::Ready,
            clipboard::Status::Paused => ClipboardStatus::Paused,
            clipboard::Status::Unavailable => ClipboardStatus::Unavailable,
        }
    }

    fn copy(&self, text: &str) -> bool {
        Clipboard::copy(self, text)
    }

    fn restore(&self, id: &str) -> bool {
        Clipboard::restore(self, id)
    }

    fn toggle_pin(&self, id: &str) -> bool {
        Clipboard::toggle_pin(self, id)
    }

    fn remove(&self, id: &str) -> bool {
        Clipboard::remove(self, id)
    }

    fn undo_remove(&self) -> bool {
        Clipboard::undo_remove(self)
    }

    fn watch(&self, callback: Rc<dyn Fn()>) -> Box<dyn std::any::Any> {
        Box::new(Clipboard::on_changed(self, move || callback()))
    }
}

/// The first bar's palette, with the launcher's own geometry.
pub fn launcher_style(bar: &BarSettings, settings: LauncherSettings) -> LauncherStyle {
    LauncherStyle {
        foreground: bar.foreground.clone(),
        background: bar.background.clone(),
        accent: bar.accent.clone(),
        border_color: bar.border_color.clone(),
        radius: bar.radius,
        opacity: bar.opacity,
        blur: bar.blur,
        shine: bar.shine,
        reflection: bar.reflection,
        flare: bar.flare,
        font: bar.font.clone(),
        font_size: bar.font_size.clone(),
        icon_font: bar.icon_font.clone(),
        css: bar.css.clone(),
        settings,
    }
}

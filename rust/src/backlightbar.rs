// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The panel brightness, ported from `modules/backlight.c`.
//!
//! A machine without a backlight hides the module rather than showing a zero.
//! The module's level detector raises the shell's brightness HUD only when the
//! percentage actually changes.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::level::Level;
use crate::osd::Osd;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;
const BACKLIGHT_ROOT: &str = "/sys/class/backlight";

/// The brightness as 0..100, or `None` when the panel or its files are absent.
pub fn percent_at(root: &Path, device: Option<&str>) -> Option<i32> {
    let name = match device.filter(|value| !value.is_empty()) {
        Some(device) => device.to_owned(),
        None => first_backlight(root)?,
    };
    let directory = root.join(name);
    let brightness = read_number(&directory.join("brightness"))?;
    let maximum = read_number(&directory.join("max_brightness"))?;

    if !brightness.is_finite() || !maximum.is_finite() || maximum <= 0.0 {
        return None;
    }

    let percent = ((brightness / maximum) * 100.0 + 0.5) as i32;

    Some(percent.clamp(0, 100))
}

pub fn percent(device: Option<&str>) -> Option<i32> {
    percent_at(Path::new(BACKLIGHT_ROOT), device)
}

fn first_backlight(root: &Path) -> Option<String> {
    let mut entries = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect::<Vec<PathBuf>>();

    entries.sort();

    entries.into_iter().find_map(|path| {
        let has_brightness = path.join("brightness").is_file();
        let has_maximum = path.join("max_brightness").is_file();

        (has_brightness && has_maximum)
            .then(|| path.file_name()?.to_str().map(str::to_owned))
            .flatten()
    })
}

fn read_number(path: &Path) -> Option<f64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let device = module.text("device").filter(|value| !value.is_empty());
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();

    let level = Rc::new(RefCell::new(Level::default()));

    host.start(move |host| update(device.as_deref(), host, &module, &level));

    Some(root)
}

fn update(
    device: Option<&str>,
    host: &ModuleHost,
    module: &ModuleSpec,
    level: &Rc<RefCell<Level>>,
) {
    let Some(percent) = percent(device) else {
        host.set_visible(false);
        host.set_text("");

        return;
    };

    host.set_visible(true);

    let value = percent.to_string();
    let values = [("percent", value.as_str())];

    host.present("{icon}", &values);

    if level.borrow_mut().update(percent, false) {
        let icon = module.text("icon").unwrap_or_else(|| "\u{f185}".to_owned());

        Osd::show_current(&icon, "Brightness", percent);
    }
}

// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! One battery pack, ported from `modules/battery.c` and the battery half of
//! `util/sysfs.c`.
//!
//! Only a pack that is filling is quiet: while charging the percentage is
//! hidden unless `#:show-capacity-when-charging?` asks for it.  A full pack or
//! one plugged in but not taking power still shows its capacity and keeps the
//! warning and critical colours.

use std::fs;
use std::path::{Path, PathBuf};

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 30;
const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryReading {
    pub name: String,
    pub status: String,
    pub capacity: i32,
    pub present: bool,
}

pub fn is_charging(status: &str) -> bool {
    status == "Charging"
}

/// The class the structural stylesheet turns into a colour.  Charging wins over
/// critical, and critical over warning, exactly as in the C module.
pub fn battery_class(
    capacity: i32,
    status: &str,
    warning: i32,
    critical: i32,
) -> Option<&'static str> {
    if is_charging(status) {
        Some("charging")
    } else if capacity <= critical {
        Some("critical")
    } else if capacity <= warning {
        Some("warning")
    } else {
        None
    }
}

/// Read one pack, or the first battery in `root` when `device` is absent.
pub fn read_at(root: &Path, device: Option<&str>) -> Option<BatteryReading> {
    let name = match device.filter(|value| !value.is_empty()) {
        Some(device) => device.to_owned(),
        None => first_battery(root)?,
    };
    let directory = root.join(&name);
    let capacity = read_trimmed(&directory.join("capacity"))?;
    let status = read_trimmed(&directory.join("status")).unwrap_or_else(|| "Unknown".to_owned());
    let present = read_trimmed(&directory.join("present"))
        .map(|value| value == "1")
        .unwrap_or(true);

    Some(BatteryReading {
        name,
        status,
        capacity: capacity.parse::<i32>().unwrap_or(0).clamp(0, 100),
        present,
    })
}

pub fn read(device: Option<&str>) -> Option<BatteryReading> {
    read_at(Path::new(POWER_SUPPLY_ROOT), device)
}

fn first_battery(root: &Path) -> Option<String> {
    let mut entries = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect::<Vec<PathBuf>>();

    entries.sort();

    entries.into_iter().find_map(|path| {
        let name = path.file_name()?.to_str()?.to_owned();
        let kind = read_trimmed(&path.join("type"))?;

        (kind == "Battery" && path.join("capacity").is_file()).then_some(name)
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
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
    host.start(move |host| update(&module, device.as_deref(), host));

    Some(root)
}

fn update(module: &ModuleSpec, device: Option<&str>, host: &ModuleHost) {
    let Some(reading) = read(device) else {
        host.set_visible(false);
        host.set_text("");

        return;
    };

    if !reading.present {
        host.set_visible(false);
        host.set_text("");

        return;
    }

    host.set_visible(true);

    let warning = module.integer("warning").unwrap_or(30) as i32;
    let critical = module.integer("critical").unwrap_or(15) as i32;
    let charging = is_charging(&reading.status);
    let state_class = battery_class(reading.capacity, &reading.status, warning, critical);

    host.remove_class("warning");
    host.remove_class("critical");
    host.remove_class("charging");

    if let Some(class) = state_class {
        host.add_class(class);
    }

    let capacity = reading.capacity.to_string();
    let values = [
        ("capacity", capacity.as_str()),
        ("state", reading.status.as_str()),
        ("device", reading.name.as_str()),
    ];
    let mut format = module
        .text("format")
        .unwrap_or_else(|| "{icon} {capacity}%".to_owned());
    let mut icon = module.text("icon").unwrap_or_default();
    let charging_icon = module.text("charging-icon");

    if charging {
        if let Some(value) = charging_icon.as_deref() {
            icon = value.to_owned();
        }

        if let Some(value) = module.text("charging-format") {
            format = value;
        } else if !module
            .boolean("show-capacity-when-charging?")
            .unwrap_or(false)
        {
            format = "{icon}".to_owned();
        }
    }

    host.present_with(&format, &icon, host.settings().tooltip.as_deref(), &values);
}

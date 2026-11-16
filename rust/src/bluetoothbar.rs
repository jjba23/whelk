// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Bluetooth adapter reading, ported from `modules/bluetooth.c`.
//!
//! The adapter's power state and the number of connected devices come from
//! bluetoothctl; connecting a device belongs to the network panel.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const COMMAND: &str = "printf 'powered:%s\\n' \"$(bluetoothctl show 2>/dev/null | \
     sed -n 's/^[[:space:]]*Powered: //p')\"; \
     printf 'connected:%s\\n' \"$(bluetoothctl devices Connected 2>/dev/null | wc -l)\"";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BluetoothReading {
    pub powered: bool,
    pub connected: i32,
}

pub fn parse(text: &str) -> BluetoothReading {
    let mut reading = BluetoothReading {
        powered: false,
        connected: 0,
    };

    for line in text.lines() {
        if let Some(powered) = line.strip_prefix("powered:") {
            reading.powered = powered == "yes";
        } else if let Some(connected) = line.strip_prefix("connected:") {
            reading.connected = connected.trim().parse().unwrap_or(0);
        }
    }

    reading
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(move |host| update(&module, host));

    Some(root)
}

fn update(module: &ModuleSpec, host: &ModuleHost) {
    let callback_host = host.clone();
    let module = module.clone();

    host.run_command(COMMAND, move |text| {
        let Some(text) = text else {
            return;
        };
        let reading = parse(&text);

        callback_host.remove_class("off");
        callback_host.remove_class("connected");

        let count = reading.connected.to_string();
        let state = if reading.powered { "on" } else { "off" };
        let values = [("count", count.as_str()), ("state", state)];
        let tooltip = callback_host.settings().tooltip.as_deref();

        if !reading.powered {
            callback_host.add_class("off");
            callback_host.present_with(
                module.text("disabled-format").as_deref().unwrap_or("off"),
                "",
                tooltip.or(Some("Bluetooth is off")),
                &values,
            );
        } else {
            if reading.connected > 0 {
                callback_host.add_class("connected");
            }

            callback_host.present_with(
                module.text("format").as_deref().unwrap_or("{icon}"),
                callback_host.settings().icon.as_deref().unwrap_or(""),
                tooltip.or(Some("{count} device(s) connected")),
                &values,
            );
        }
    });
}

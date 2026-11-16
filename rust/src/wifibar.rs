// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The Wi-Fi reading, ported from `modules/wifi.c`.
//!
//! The radio's state and the active access point are nmcli's answer; the
//! network panel that chooses a network is a separate surface and not part of
//! this reading.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::nmcli;

pub const COMMAND: &str = "printf 'radio:%s\\n' \"$(nmcli radio wifi 2>/dev/null)\"; \
     nmcli -t -f ACTIVE,SSID,SIGNAL dev wifi 2>/dev/null";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiReading {
    pub enabled: bool,
    pub ssid: Option<String>,
    pub signal: i32,
}

pub fn parse(text: &str) -> WifiReading {
    let mut reading = WifiReading {
        enabled: false,
        ssid: None,
        signal: 0,
    };

    for line in text.lines() {
        if let Some(radio) = line.strip_prefix("radio:") {
            reading.enabled = radio == "enabled";
            continue;
        }

        if line.is_empty() {
            continue;
        }

        let fields = nmcli::split_terse(line);

        if fields.len() < 3 || fields[0] != "yes" {
            continue;
        }

        reading.ssid = Some(fields[1].clone());
        reading.signal = fields[2].trim().parse().unwrap_or(0);
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

        let state = if !reading.enabled {
            "off"
        } else if reading.ssid.is_some() {
            "connected"
        } else {
            "disconnected"
        };
        let signal = reading.signal.to_string();
        let ssid = reading.ssid.as_deref().unwrap_or("");
        let values = [
            ("ssid", ssid),
            ("signal", signal.as_str()),
            ("state", state),
        ];
        let tooltip = callback_host.settings().tooltip.as_deref();

        if !reading.enabled {
            callback_host.add_class("off");
            callback_host.present_with(
                module.text("disabled-format").as_deref().unwrap_or("off"),
                "",
                tooltip.or(Some("Wi-Fi is off")),
                &values,
            );
        } else if reading.ssid.is_none() {
            callback_host.present_with(
                module
                    .text("disconnected-format")
                    .as_deref()
                    .unwrap_or("no network"),
                "",
                tooltip.or(Some("Not connected")),
                &values,
            );
        } else {
            let show_ssid = module.boolean("show-ssid?").unwrap_or(false);
            let format = module
                .text("format")
                .unwrap_or_else(|| nmcli::wifi_format(show_ssid).to_owned());
            let default_tooltip = nmcli::wifi_tooltip(show_ssid);

            callback_host.add_class("connected");
            callback_host.present_with(
                &format,
                callback_host.settings().icon.as_deref().unwrap_or(""),
                tooltip.or(Some(default_tooltip)),
                &values,
            );
        }
    });
}

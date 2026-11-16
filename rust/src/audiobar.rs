// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The default-sink volume, ported from `modules/audio.c`.
//!
//! wpctl is tried first and pactl is the fallback.  The bar shows the speaker
//! glyph by default; a configuration that wants the number writes it in its
//! own `#:format`.

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const COMMAND: &str = "wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null || \
     pactl get-sink-volume @DEFAULT_SINK@ 2>/dev/null";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioReading {
    pub volume: i32,
    pub muted: bool,
}

/// Understand both wpctl's `Volume: 0.42 [MUTED]` and pactl's percentage
/// answer.  Empty or unparseable output is a zero reading, as in the C module.
pub fn parse(output: &str) -> AudioReading {
    let mut volume = None;
    let mut muted = false;

    if let Some(label) = output.find("Volume:") {
        let rest = output[label + "Volume:".len()..].trim_start();

        if let Some(level) = rest
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<f64>().ok())
        {
            volume = Some((level * 100.0 + 0.5) as i32);
            muted = output.contains("[MUTED]");
        }
    }

    // pactl also has a "Volume:" label, but its first number is a raw
    // amplitude rather than a fraction.  Falling back on the percentage keeps
    // the PulseAudio answer useful instead of showing zero.
    if volume.is_none() {
        if let Some(percent) = output.find('%') {
            let bytes = output.as_bytes();
            let mut start = percent;

            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }

            volume = Some(output[start..percent].parse().unwrap_or(0));
        }
    }

    AudioReading {
        volume: volume.unwrap_or(0).clamp(0, 150),
        muted,
    }
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
        let mut format = callback_host
            .settings()
            .format
            .as_deref()
            .unwrap_or("{icon}");
        let mut icon = callback_host.settings().icon.as_deref().unwrap_or("");
        let muted_format = module
            .text("muted-format")
            .filter(|value| !value.is_empty());
        let muted_icon = module.text("muted-icon").filter(|value| !value.is_empty());

        if reading.muted {
            if let Some(value) = muted_format.as_deref() {
                format = value;
            }
            if let Some(value) = muted_icon.as_deref() {
                icon = value;
            }
        }

        let volume = reading.volume.to_string();
        let values = [("volume", volume.as_str())];

        callback_host.present_with(
            format,
            icon,
            callback_host.settings().tooltip.as_deref(),
            &values,
        );
    });
}

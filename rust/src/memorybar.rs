// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Memory usage, ported from `modules/memory.c`.
//!
//! `/proc/meminfo` distinguishes used from available memory, so caches do not
//! make a healthy machine look full.  Swap gets the same used/total treatment.

use std::fs;

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;
use crate::format;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 5;
const PROC_MEMINFO: &str = "/proc/meminfo";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

impl MemoryInfo {
    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.available_kb)
    }

    pub fn swap_used_kb(&self) -> u64 {
        self.swap_total_kb.saturating_sub(self.swap_free_kb)
    }

    /// The rounded integer percentage used by the C module.
    pub fn percentage(&self) -> i32 {
        if self.total_kb == 0 {
            return 0;
        }

        let used = u128::from(self.used_kb());
        let total = u128::from(self.total_kb);

        ((used * 100 + total / 2) / total) as i32
    }
}

pub fn parse(text: &str) -> Option<MemoryInfo> {
    let mut info = MemoryInfo::default();
    let mut saw_total = false;

    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let value = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);

        match key {
            "MemTotal" => {
                info.total_kb = value;
                saw_total = true;
            }
            "MemAvailable" => info.available_kb = value,
            "SwapTotal" => info.swap_total_kb = value,
            "SwapFree" => info.swap_free_kb = value,
            _ => {}
        }
    }

    (saw_total && info.total_kb > 0).then_some(info)
}

pub fn read() -> Option<MemoryInfo> {
    fs::read_to_string(PROC_MEMINFO)
        .ok()
        .and_then(|text| parse(&text))
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
    host.start(move |host| update(host));

    Some(root)
}

fn update(host: &ModuleHost) {
    let Some(info) = read() else {
        host.set_text("—");

        return;
    };
    let percentage = info.percentage().to_string();
    let used = format::str_gib(info.used_kb());
    let total = format::str_gib(info.total_kb);
    let available = format::str_gib(info.available_kb);
    let swap_used = format::str_gib(info.swap_used_kb());
    let swap_total = format::str_gib(info.swap_total_kb);
    let values = [
        ("percentage", percentage.as_str()),
        ("used", used.as_str()),
        ("total", total.as_str()),
        ("avail", available.as_str()),
        ("swap-used", swap_used.as_str()),
        ("swap-total", swap_total.as_str()),
    ];

    host.present("{icon} {percentage}%", &values);
}

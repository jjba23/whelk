// bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT

//! The memory reading behind the system card: `/proc/meminfo`.

#![allow(dead_code)]

use std::fs;

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

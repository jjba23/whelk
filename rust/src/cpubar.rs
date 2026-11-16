// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! CPU usage, ported from `modules/cpu.c`.
//!
//! `/proc/stat` counts jiffies, so a percentage is the difference between two
//! samples.  A first paint with an initial sample is therefore a real interval,
//! while a failed initial sample makes the first good reading zero.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use gtk4 as gtk;

use crate::bar::ModuleSpec;
use crate::barfactory::ModuleContext;
use crate::barmodule::ModuleHost;

pub const DEFAULT_INTERVAL_SECONDS: u32 = 2;
const PROC_STAT: &str = "/proc/stat";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}

/// Parse the aggregate `cpu` line.  Fields beyond the first eight (guest and
/// guest_nice) are deliberately ignored, matching the C reader.
pub fn parse_sample(text: &str) -> Option<CpuSample> {
    let line = text.trim_start().lines().next()?;
    let rest = line.strip_prefix("cpu ")?;
    let mut fields = [0_u64; 8];
    let mut count = 0;

    for field in rest.split_whitespace().take(fields.len()) {
        fields[count] = field.parse().unwrap_or(0);
        count += 1;
    }

    Some(CpuSample {
        total: fields[..count].iter().sum(),
        idle: fields[3] + fields[4],
    })
}

/// The busy share between two samples, in percent.  Counter resets and equal
/// samples are zero rather than a negative or undefined reading.
pub fn usage(previous: &CpuSample, current: &CpuSample) -> f64 {
    if current.total < previous.total || current.idle < previous.idle {
        return 0.0;
    }

    let total_delta = current.total - previous.total;
    let idle_delta = current.idle - previous.idle;

    if total_delta == 0 {
        return 0.0;
    }

    let busy = total_delta.saturating_sub(idle_delta);

    ((busy as f64 / total_delta as f64) * 100.0).clamp(0.0, 100.0)
}

#[derive(Debug, Default)]
pub struct CpuSampler {
    previous: Option<CpuSample>,
}

impl CpuSampler {
    /// Take the startup sample the C module keeps, so its first timer reading
    /// has a real interval ahead of it.
    pub fn new() -> Self {
        Self {
            previous: read_sample(),
        }
    }

    pub fn usage(&mut self) -> Option<f64> {
        let current = read_sample()?;
        let usage = self
            .previous
            .map(|previous| usage(&previous, &current))
            .unwrap_or(0.0);

        self.previous = Some(current);

        Some(usage)
    }
}

fn read_sample() -> Option<CpuSample> {
    fs::read_to_string(PROC_STAT)
        .ok()
        .and_then(|text| parse_sample(&text))
}

pub fn module(module: &ModuleSpec, context: &ModuleContext) -> Option<gtk::Box> {
    let module = module.clone();
    let sampler = Rc::new(RefCell::new(CpuSampler::new()));
    let (root, host) = ModuleHost::new(
        &module,
        DEFAULT_INTERVAL_SECONDS,
        context.spacing,
        context.icon_font.as_deref(),
    );

    host.add_label();
    host.start(move |host| update(&sampler, host));

    Some(root)
}

fn update(sampler: &Rc<RefCell<CpuSampler>>, host: &ModuleHost) {
    let Some(usage) = sampler.borrow_mut().usage() else {
        host.set_text("—");

        return;
    };
    let usage = (usage + 0.5) as i32;
    let usage = usage.to_string();
    let values = [("usage", usage.as_str())];

    host.present("{icon} {usage}%", &values);
}

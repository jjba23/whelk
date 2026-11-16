// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The machine's account of itself, ported from `util/hostinfo.c`.
//!
//! What an About window says: the system's name, the processor, the memory and
//! disk that are there now, the kernel, the session and the display.  Every
//! reader answers `None` when its source cannot be read, so a window leaves a
//! row out rather than print a guess.
//!
//! The parsers take their file's text instead of opening the file, because the
//! awkward cases live in the text: `/proc/cpuinfo`'s per-core `processor : 0`
//! line is a core index rather than a model, and a memory total can arrive
//! without `MemAvailable` beside it.

use std::ffi::CStr;

use gtk::prelude::*;
use gtk4 as gtk;

use crate::session::Session;

/// `/etc/os-release`'s `PRETTY_NAME`, or `"GNU/Linux"`.
pub fn os_name() -> String {
    gtk::glib::os_info("PRETTY_NAME")
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "GNU/Linux".to_owned())
}

/// The model in `cpuinfo`, skipping the per-core `processor` index lines.
pub fn cpu_model_from(cpuinfo: &str) -> Option<String> {
    // The keys are matched case-insensitively because ARM says "Processor" and
    // x86 says "model name", which is also why the numeric per-core line has
    // to be skipped explicitly.
    const KEYS: [&str; 5] = ["model name", "Model", "Hardware", "Processor", "cpu model"];

    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        if KEYS.iter().any(|wanted| key.eq_ignore_ascii_case(wanted))
            && !value.is_empty()
            && !all_digits(value)
        {
            return Some(value.to_owned());
        }
    }

    None
}

/// The same, read from `/proc/cpuinfo`.
pub fn cpu_model() -> Option<String> {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| cpu_model_from(&contents))
}

/// The model and the thread count, e.g. `"AMD Ryzen 7 · 16 threads"`.
pub fn cpu_summary() -> Option<String> {
    cpu_summary_from(cpu_model().as_deref(), gtk::glib::num_processors())
}

/// The CPU summary from an already-read model and thread count.
pub fn cpu_summary_from(model: Option<&str>, threads: u32) -> Option<String> {
    if let Some(model) = model.filter(|model| !model.is_empty()) {
        return Some(format!("{model} · {}", thread_count(threads)));
    }

    if threads > 0 {
        return Some(thread_count(threads));
    }

    None
}

/// `meminfo` as `"30.7 GiB total · 21.0 GiB available"`.
pub fn memory_summary_from(meminfo: &str) -> Option<String> {
    let mut total = 0;
    let mut available = 0;

    for line in meminfo.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            if let Some(kibibytes) = parse_u64_prefix(value) {
                total = kibibytes;
            }
        } else if let Some(value) = line.strip_prefix("MemAvailable:")
            && let Some(kibibytes) = parse_u64_prefix(value)
        {
            available = kibibytes;
        }
    }

    if total == 0 {
        return None;
    }

    let gibibytes = |kibibytes: u64| kibibytes as f64 / (1024.0 * 1024.0);

    if available > 0 && available <= total {
        Some(format!(
            "{:.1} GiB total · {:.1} GiB available",
            gibibytes(total),
            gibibytes(available)
        ))
    } else {
        Some(format!("{:.1} GiB", gibibytes(total)))
    }
}

/// The same, read from `/proc/meminfo`.
pub fn memory_summary() -> Option<String> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| memory_summary_from(&contents))
}

/// The root filesystem: `"413 GiB free of 928 GiB (44% used)"`.
pub fn disk_summary() -> Option<String> {
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };

    if unsafe { libc::statvfs(c"/".as_ptr(), &mut stats) } != 0 {
        return None;
    }

    disk_summary_from(
        stats.f_blocks as u64 * stats.f_frsize as u64,
        stats.f_bavail as u64 * stats.f_frsize as u64,
    )
}

/// The disk summary from already-read capacity numbers.
pub fn disk_summary_from(total: u64, free: u64) -> Option<String> {
    if total == 0 {
        return None;
    }

    let used = total.saturating_sub(free);
    let gibibytes = |bytes: u64| bytes as f64 / 1_073_741_824.0;
    let percent = ((used as f64 * 100.0 / total as f64) + 0.5) as i32;

    Some(format!(
        "{:.0} GiB free of {:.0} GiB ({percent}% used)",
        gibibytes(free),
        gibibytes(total)
    ))
}

/// The kernel release and machine, e.g. `"7.3.3-1-default (x86_64)"`.
pub fn kernel_release() -> Option<String> {
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };

    if unsafe { libc::uname(&mut name) } != 0 {
        return None;
    }

    Some(format!(
        "{} ({})",
        c_array_string(&name.release),
        c_array_string(&name.machine)
    ))
}

/// `uptime` in seconds, as a sentence that leaves zero parts out.
pub fn uptime_from(uptime: &str) -> Option<String> {
    let seconds = parse_f64_prefix(uptime)?;
    let minutes = (seconds / 60.0) as u64;
    let days = minutes / (60 * 24);
    let hours = (minutes / 60) % 24;
    let rest = minutes % 60;

    if days > 0 && hours > 0 {
        return Some(format!(
            "{}, {}",
            quantity(days, "day", "days"),
            quantity(hours, "hour", "hours")
        ));
    }

    if days > 0 {
        return Some(quantity(days, "day", "days"));
    }

    if hours > 0 && rest > 0 {
        return Some(format!(
            "{}, {}",
            quantity(hours, "hour", "hours"),
            quantity(rest, "minute", "minutes")
        ));
    }

    if hours > 0 {
        return Some(quantity(hours, "hour", "hours"));
    }

    Some(quantity(minutes, "minute", "minutes"))
}

/// The same, read from `/proc/uptime`.
pub fn uptime_summary() -> Option<String> {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| uptime_from(&contents))
}

/// The session and its protocol, e.g. `"niri · Wayland"`.  A desktop name the
/// session already carries is not repeated.
pub fn session_summary() -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();

    session_summary_from(
        Session::detect().name(),
        desktop.as_deref(),
        session_type.as_deref(),
    )
}

/// The session summary from an already-known session name and environment.
pub fn session_summary_from(
    name: &str,
    desktop: Option<&str>,
    session_type: Option<&str>,
) -> String {
    let mut summary = name.to_owned();

    // A niri session exports XDG_CURRENT_DESKTOP=niri; printing "niri, niri"
    // tells the reader nothing twice.
    if let Some(desktop) = desktop.filter(|desktop| !desktop.is_empty()) {
        let named = desktop
            .split(':')
            .any(|part| part.eq_ignore_ascii_case(name));

        if !named {
            summary.push_str(" · ");
            summary.push_str(desktop);
        }
    }

    if let Some(session_type) = session_type.filter(|kind| !kind.is_empty()) {
        summary.push_str(" · ");
        summary.push_str(&uppercase_first(session_type));
    }

    summary
}

/// The first monitor, e.g. `"2560 × 1600 · 60 Hz · eDP-1"`, with the monitor
/// count appended when there is more than one.
pub fn display_summary() -> Option<String> {
    let display = gtk::gdk::Display::default()?;
    let monitors = display.monitors();
    let count = monitors.n_items();

    if count == 0 {
        return None;
    }

    let monitor = monitors.item(0).and_downcast::<gtk::gdk::Monitor>()?;
    let geometry = monitor.geometry();
    let connector = monitor.connector();
    let model = monitor.model();
    let label = connector
        .as_deref()
        .filter(|connector| !connector.is_empty())
        .or_else(|| model.as_deref().filter(|model| !model.is_empty()));

    display_summary_from(
        geometry.width(),
        geometry.height(),
        monitor.refresh_rate(),
        label,
        count,
    )
}

/// The display summary from already-read monitor properties.
pub fn display_summary_from(
    width: i32,
    height: i32,
    refresh_mhz: i32,
    label: Option<&str>,
    monitor_count: u32,
) -> Option<String> {
    if monitor_count == 0 {
        return None;
    }

    let mut summary = format!("{} × {}", width.max(0), height.max(0));

    if refresh_mhz > 0 {
        summary.push_str(&format!(" · {:.0} Hz", refresh_mhz as f64 / 1000.0));
    }

    if let Some(label) = label.filter(|label| !label.is_empty()) {
        summary.push_str(" · ");
        summary.push_str(label);
    }

    if monitor_count > 1 {
        summary.push_str(&format!(" · {monitor_count} monitors"));
    }

    Some(summary)
}

fn thread_count(threads: u32) -> String {
    format!("{threads} thread{}", if threads == 1 { "" } else { "s" })
}

fn quantity(value: u64, singular: &str, plural: &str) -> String {
    format!("{value} {}", if value == 1 { singular } else { plural })
}

fn uppercase_first(text: &str) -> String {
    let mut characters = text.chars();

    match characters.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
        None => String::new(),
    }
}

fn all_digits(text: &str) -> bool {
    let mut digit = false;

    for character in text.chars() {
        if character.is_ascii_digit() {
            digit = true;
        } else if !character.is_ascii_whitespace() {
            return false;
        }
    }

    digit
}

fn parse_u64_prefix(text: &str) -> Option<u64> {
    let text = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let text = text.strip_prefix('+').unwrap_or(text);
    let digits: String = text
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        return None;
    }

    digits.parse().ok()
}

fn parse_f64_prefix(text: &str) -> Option<f64> {
    let text = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let bytes = text.as_bytes();
    let mut index = 0;

    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }

    let integer_start = index;

    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }

    let mut digits = index > integer_start;

    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;

        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }

        digits |= index > fraction_start;
    }

    if !digits {
        return None;
    }

    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent_mark = index;

        index += 1;

        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }

        let exponent_start = index;

        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }

        if index == exponent_start {
            index = exponent_mark;
        }
    }

    text[..index].parse().ok()
}

fn c_array_string(value: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

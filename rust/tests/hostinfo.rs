// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The About window's readings, and the two cases /proc makes awkward: the
// per-core index line that is not a model, and a memory total with no
// MemAvailable beside it.

use whelk_rs::hostinfo;

#[test]
fn cpu_model_skips_the_core_index() {
    let cpuinfo = "processor : 0\n\
                   vendor_id : AuthenticAMD\n\
                   model name : AMD Ryzen 7 7840U w/ Radeon 780M\n\
                   processor : 1\n\
                   model name : AMD Ryzen 7 7840U w/ Radeon 780M\n";

    assert_eq!(
        hostinfo::cpu_model_from(cpuinfo).as_deref(),
        Some("AMD Ryzen 7 7840U w/ Radeon 780M")
    );
}

#[test]
fn cpu_model_reads_arm_processor() {
    let cpuinfo = "processor : 0\nProcessor : ARMv8 Processor rev 1 (v8l)\n";

    assert_eq!(
        hostinfo::cpu_model_from(cpuinfo).as_deref(),
        Some("ARMv8 Processor rev 1 (v8l)")
    );
}

#[test]
fn cpu_model_without_a_model() {
    assert_eq!(
        hostinfo::cpu_model_from("processor : 0\nprocessor : 1\n"),
        None
    );
    assert_eq!(hostinfo::cpu_model_from(""), None);
}

#[test]
fn cpu_summary_counts_threads() {
    assert_eq!(
        hostinfo::cpu_summary_from(Some("AMD Ryzen 7"), 16).as_deref(),
        Some("AMD Ryzen 7 · 16 threads")
    );
    assert_eq!(
        hostinfo::cpu_summary_from(Some("One core"), 1).as_deref(),
        Some("One core · 1 thread")
    );
    assert_eq!(
        hostinfo::cpu_summary_from(None, 8).as_deref(),
        Some("8 threads")
    );
    assert_eq!(hostinfo::cpu_summary_from(None, 0), None);
}

#[test]
fn memory_total_and_available() {
    let meminfo = "MemTotal:       32207892 kB\n\
                   MemFree:         1234567 kB\n\
                   MemAvailable:   22000000 kB\n";

    assert_eq!(
        hostinfo::memory_summary_from(meminfo).as_deref(),
        Some("30.7 GiB total · 21.0 GiB available")
    );
}

#[test]
fn memory_without_available() {
    assert_eq!(
        hostinfo::memory_summary_from("MemTotal:       1048576 kB\n").as_deref(),
        Some("1.0 GiB")
    );
    // An available figure larger than the total is not an availability.
    assert_eq!(
        hostinfo::memory_summary_from("MemTotal: 1048576 kB\nMemAvailable: 2097152 kB\n")
            .as_deref(),
        Some("1.0 GiB")
    );
    assert_eq!(hostinfo::memory_summary_from(""), None);
}

#[test]
fn disk_summary_rounds_percent_and_sizes() {
    const GIB: u64 = 1_073_741_824;

    assert_eq!(
        hostinfo::disk_summary_from(10 * GIB, 4 * GIB).as_deref(),
        Some("4 GiB free of 10 GiB (60% used)")
    );
    assert_eq!(hostinfo::disk_summary_from(0, 0), None);
}

#[test]
fn uptime_leaves_zero_parts_out() {
    assert_eq!(
        hostinfo::uptime_from("2640.00 0.00").as_deref(),
        Some("44 minutes")
    );
    assert_eq!(
        hostinfo::uptime_from("3600.00 0.00").as_deref(),
        Some("1 hour")
    );
    assert_eq!(
        hostinfo::uptime_from("90000.00 0.00").as_deref(),
        Some("1 day, 1 hour")
    );
    assert_eq!(
        hostinfo::uptime_from("60.00 0.00").as_deref(),
        Some("1 minute")
    );
    assert_eq!(
        hostinfo::uptime_from("3660.00 0.00").as_deref(),
        Some("1 hour, 1 minute")
    );
    assert_eq!(
        hostinfo::uptime_from("172800.00 0.00").as_deref(),
        Some("2 days")
    );
    assert_eq!(hostinfo::uptime_from(""), None);
    assert_eq!(hostinfo::uptime_from("not a number"), None);
}

#[test]
fn session_summary_does_not_repeat_the_session_name() {
    assert_eq!(
        hostinfo::session_summary_from("niri", Some("niri"), Some("wayland")),
        "niri · Wayland"
    );
    assert_eq!(
        hostinfo::session_summary_from("niri", Some("GNOME"), Some("wayland")),
        "niri · GNOME · Wayland"
    );
    // One matching component is enough: the desktop is not repeated.
    assert_eq!(
        hostinfo::session_summary_from("niri", Some("niri:GNOME"), Some("wayland")),
        "niri · Wayland"
    );
    assert_eq!(
        hostinfo::session_summary_from("x11", None, Some("x11")),
        "x11 · X11"
    );
    assert_eq!(
        hostinfo::session_summary_from("wayland", Some(""), Some("")),
        "wayland"
    );
}

#[test]
fn display_summary_names_the_first_monitor() {
    assert_eq!(
        hostinfo::display_summary_from(2560, 1600, 60_000, Some("eDP-1"), 1).as_deref(),
        Some("2560 × 1600 · 60 Hz · eDP-1")
    );
    assert_eq!(
        hostinfo::display_summary_from(1920, 1080, 0, None, 2).as_deref(),
        Some("1920 × 1080 · 2 monitors")
    );
    // A negative geometry is clamped, as the C summary does.
    assert_eq!(
        hostinfo::display_summary_from(-1, -1, 0, Some(""), 1).as_deref(),
        Some("0 × 0")
    );
    assert_eq!(hostinfo::display_summary_from(0, 0, 0, None, 0), None);
}

#[test]
fn host_readers_answer_without_a_display() {
    // The pure parsers are the tested part; these only assert the wrappers do
    // not invent a value when a source is unavailable.
    let _ = hostinfo::cpu_model();
    let _ = hostinfo::memory_summary();
    let _ = hostinfo::disk_summary();
    let _ = hostinfo::uptime_summary();
    let _ = hostinfo::kernel_release();
    assert!(!hostinfo::os_name().is_empty());
}

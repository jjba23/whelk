// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// These are the module readings that do not need a display: the clock's
// calendar, the window/custom truncation rules, the wpctl and pactl answers,
// sysfs CPU/memory/backlight/battery/Caps Lock fixtures, niri's layout listing,
// and the nmcli/bluetoothctl/power-profile line formats.

use std::fs;
use std::path::Path;

use gtk4::glib;
use tempfile::TempDir;
use whelk_rs::audiobar;
use whelk_rs::backlightbar;
use whelk_rs::batterybar;
use whelk_rs::bluetoothbar;
use whelk_rs::characterbar;
use whelk_rs::clockbar;
use whelk_rs::cpubar;
use whelk_rs::custombar;
use whelk_rs::keyboardbar;
use whelk_rs::languagebar;
use whelk_rs::memorybar;
use whelk_rs::powerprofilebar;
use whelk_rs::session::Session;
use whelk_rs::wifibar;
use whelk_rs::windowbar;

#[test]
fn clock_formats_its_time_and_lays_out_a_monday_first_month() {
    let now =
        glib::DateTime::new(&glib::TimeZone::local(), 2024, 9, 1, 12, 0, 0.0).expect("valid date");

    assert_eq!(
        clockbar::format_time(&now, "%Y-%m-%d %H:%M"),
        "2024-09-01 12:00"
    );

    let calendar = clockbar::calendar(&now);
    let lines = calendar.lines().collect::<Vec<_>>();

    assert_eq!(lines[0], now.format("%B %Y").unwrap().as_str());
    assert_eq!(lines[1], "Mo Tu We Th Fr Sa Su");
    assert_eq!(lines.len(), 8);
    assert_eq!(lines[2].trim_start(), "1");
    assert_eq!(lines[6].trim(), "23 24 25 26 27 28 29");
    assert_eq!(lines[7].trim(), "30");
}

#[test]
fn window_reading_truncates_the_title_and_uses_the_session_parser() {
    let answer = r#"{"title":"A very long window title","app_id":"org.example.App"}"#;
    let window = windowbar::focused_reading(Session::Niri, answer, 10).expect("focused window");

    assert_eq!(window.title, "A very lon…");
    assert_eq!(window.app_id, "org.example.App");
    assert!(windowbar::focused_reading(Session::Wayland, answer, 50).is_none());

    // River's push carries only a title; a blank push clears the row.
    let river =
        windowbar::focused_reading(Session::River, "Report.odt\n", 50).expect("river focused view");

    assert_eq!(river.title, "Report.odt");
    assert_eq!(river.app_id, "");
    assert!(windowbar::focused_reading(Session::River, "\n", 50).is_none());
}

#[test]
fn custom_reading_takes_the_first_line_and_honours_max_length() {
    assert_eq!(
        custombar::reading("now playing\nnext line", 0),
        "now playing"
    );
    assert_eq!(custombar::reading("long line", 4), "long…");
    assert_eq!(custombar::reading("short", 20), "short");
}

#[test]
fn audio_reads_wpctl_and_pactl_and_clamps_the_volume() {
    assert_eq!(
        audiobar::parse("Volume: 0.42"),
        audiobar::AudioReading {
            volume: 42,
            muted: false
        }
    );
    assert_eq!(
        audiobar::parse("Volume: 0.42 [MUTED]"),
        audiobar::AudioReading {
            volume: 42,
            muted: true
        }
    );
    assert_eq!(
        audiobar::parse("Volume: front-left: 26214 /  40% / -23.88 dB"),
        audiobar::AudioReading {
            volume: 40,
            muted: false
        }
    );
    assert_eq!(audiobar::parse("Volume: 1.80").volume, 150);
    assert_eq!(audiobar::parse("").volume, 0);
}

fn battery_pack(root: &Path, name: &str, kind: &str, capacity: &str, status: &str, present: &str) {
    let directory = root.join(name);

    fs::create_dir_all(&directory).expect("battery fixture directory");
    fs::write(directory.join("type"), format!("{kind}\n")).expect("type fixture");
    fs::write(directory.join("capacity"), format!("{capacity}\n")).expect("capacity fixture");
    fs::write(directory.join("status"), format!("{status}\n")).expect("status fixture");
    fs::write(directory.join("present"), format!("{present}\n")).expect("present fixture");
}

fn battery_root() -> TempDir {
    let root = tempfile::tempdir().expect("temporary battery root");
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).expect("power supply directory");
    battery_pack(&supply, "AC", "Mains", "100", "Not charging", "1");
    battery_pack(&supply, "BAT0", "Battery", "55", "Discharging", "1");

    root
}

#[test]
fn battery_reads_the_first_battery_or_the_configured_one() {
    let root = battery_root();
    let supply = root.path().join("power_supply");
    let reading = batterybar::read_at(&supply, None).expect("first battery");

    assert_eq!(reading.name, "BAT0");
    assert_eq!(reading.capacity, 55);
    assert_eq!(reading.status, "Discharging");
    assert!(reading.present);
    assert_eq!(batterybar::read_at(&supply, Some("BAT0")).unwrap(), reading);
    assert!(batterybar::read_at(&supply, Some("missing")).is_none());
}

#[test]
fn battery_absent_and_present_states_are_distinct() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    battery_pack(&supply, "BAT0", "Battery", "80", "Discharging", "0");

    let reading = batterybar::read_at(&supply, None).unwrap();

    assert!(!reading.present);
}

/// A pack with the energy pair sysfs reports for most laptops, in micro-units.
fn energy_pack(
    root: &Path,
    name: &str,
    capacity: &str,
    status: &str,
    now: &str,
    full: &str,
    power: &str,
) {
    let directory = root.join(name);

    fs::create_dir_all(&directory).expect("battery fixture directory");
    fs::write(directory.join("type"), "Battery\n").expect("type fixture");
    fs::write(directory.join("capacity"), format!("{capacity}\n")).expect("capacity fixture");
    fs::write(directory.join("status"), format!("{status}\n")).expect("status fixture");
    fs::write(directory.join("present"), "1\n").expect("present fixture");
    fs::write(directory.join("energy_now"), format!("{now}\n")).expect("energy_now fixture");
    fs::write(directory.join("energy_full"), format!("{full}\n")).expect("energy_full fixture");
    fs::write(directory.join("power_now"), format!("{power}\n")).expect("power_now fixture");
}

#[test]
fn battery_time_comes_from_energy_and_power() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    // 30 Wh left, draining 10 W: three hours.
    energy_pack(
        &supply,
        "BAT0",
        "75",
        "Discharging",
        "30000000",
        "40000000",
        "10000000",
    );

    let reading = batterybar::read_at(&supply, None).expect("a battery");

    assert_eq!(reading.seconds_remaining, Some(3 * 3600));
    assert_eq!(
        batterybar::format_seconds(reading.seconds_remaining),
        "3h 00m"
    );

    // Charging counts up to full instead: 10 Wh to go at 20 W is half an hour.
    energy_pack(
        &supply, "BAT0", "75", "Charging", "30000000", "40000000", "20000000",
    );

    let charging = batterybar::read_at(&supply, None).expect("a battery");

    assert_eq!(charging.seconds_remaining, Some(1800));
    assert_eq!(
        batterybar::format_seconds(charging.seconds_remaining),
        "30m"
    );
}

#[test]
fn battery_time_falls_back_to_charge_and_current() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");
    let directory = supply.join("BAT0");

    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("type"), "Battery\n").unwrap();
    fs::write(directory.join("capacity"), "50\n").unwrap();
    fs::write(directory.join("status"), "Discharging\n").unwrap();
    fs::write(directory.join("present"), "1\n").unwrap();
    // 2000 mAh at 12 V is 24 Wh; 2 A at 12 V is 24 W: one hour.
    fs::write(directory.join("charge_now"), "2000000\n").unwrap();
    fs::write(directory.join("charge_full"), "4000000\n").unwrap();
    fs::write(directory.join("current_now"), "2000000\n").unwrap();
    fs::write(directory.join("voltage_now"), "12000000\n").unwrap();

    let reading = batterybar::read_at(&supply, None).expect("a battery");

    assert_eq!(reading.seconds_remaining, Some(3600));
}

#[test]
fn a_full_or_idle_pack_has_no_duration() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    energy_pack(&supply, "BAT0", "100", "Full", "40000000", "40000000", "0");

    let reading = batterybar::read_at(&supply, None).expect("a battery");

    assert_eq!(reading.seconds_remaining, None);
    assert_eq!(batterybar::format_seconds(None), "-");
}

#[test]
fn device_all_sums_the_packs() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    // Two packs, 20 Wh and 40 Wh left of 40 Wh and 40 Wh: 60 of 80 is 75%.
    energy_pack(
        &supply,
        "BAT0",
        "50",
        "Discharging",
        "20000000",
        "40000000",
        "5000000",
    );
    energy_pack(
        &supply,
        "BAT1",
        "100",
        "Discharging",
        "40000000",
        "40000000",
        "5000000",
    );

    let both = batterybar::read_at(&supply, Some("all")).expect("two batteries");

    assert_eq!(both.name, "all");
    assert_eq!(both.capacity, 75);
    assert_eq!(both.status, "Discharging");
    // 60 Wh at 10 W is six hours; the second pack alone would say eight.
    assert_eq!(both.seconds_remaining, Some(6 * 3600));

    let one = batterybar::read_at(&supply, Some("BAT1")).expect("the first pack");

    assert_eq!(one.name, "BAT1");
    assert_eq!(one.capacity, 100);
}

#[test]
fn one_charging_pack_makes_the_machine_charge() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    energy_pack(&supply, "BAT0", "100", "Full", "40000000", "40000000", "0");
    energy_pack(
        &supply, "BAT1", "50", "Charging", "20000000", "40000000", "10000000",
    );

    let both = batterybar::read_at(&supply, Some("all")).expect("two batteries");

    assert_eq!(both.status, "Charging");
    // The full pack has nothing to fill, so the hour is the second pack's.
    assert_eq!(both.seconds_remaining, Some(2 * 3600));
}

#[test]
fn battery_classes_put_charging_before_thresholds() {
    assert_eq!(
        batterybar::battery_class(10, "Discharging", 30, 15),
        Some("critical")
    );
    assert_eq!(
        batterybar::battery_class(25, "Discharging", 30, 15),
        Some("warning")
    );
    assert_eq!(batterybar::battery_class(80, "Discharging", 30, 15), None);
    assert_eq!(
        batterybar::battery_class(5, "Charging", 30, 15),
        Some("charging")
    );
}

#[test]
fn wifi_reads_the_radio_and_the_active_escaped_ssid() {
    let reading = wifibar::parse("radio:enabled\nno:Other Network:40\nyes:Cafe\\:Guest:72\n");

    assert!(reading.enabled);
    assert_eq!(reading.ssid.as_deref(), Some("Cafe:Guest"));
    assert_eq!(reading.signal, 72);

    let disabled = wifibar::parse("radio:disabled\n");

    assert!(!disabled.enabled);
    assert_eq!(disabled.ssid, None);
    assert_eq!(disabled.signal, 0);
}

#[test]
fn bluetooth_reads_power_and_connected_count() {
    assert_eq!(
        bluetoothbar::parse("powered:yes\nconnected:2\n"),
        bluetoothbar::BluetoothReading {
            powered: true,
            connected: 2
        }
    );
    assert_eq!(
        bluetoothbar::parse("powered:no\nconnected:0\n"),
        bluetoothbar::BluetoothReading {
            powered: false,
            connected: 0
        }
    );
}

#[test]
fn cpu_samples_use_the_eight_aggregate_fields_and_report_deltas() {
    let previous =
        cpubar::parse_sample("cpu  100 0 50 850 0 0 0 0 10 10\n").expect("aggregate CPU line");
    let current =
        cpubar::parse_sample("cpu  200 0 100 900 0 0 0 0 20 20\n").expect("aggregate CPU line");

    assert_eq!(
        previous,
        cpubar::CpuSample {
            total: 1000,
            idle: 850
        }
    );
    assert_eq!(
        current,
        cpubar::CpuSample {
            total: 1200,
            idle: 900
        }
    );
    assert_eq!(cpubar::usage(&previous, &current), 75.0);
    assert_eq!(cpubar::usage(&current, &previous), 0.0);
    assert_eq!(cpubar::usage(&previous, &previous), 0.0);
    assert!(cpubar::parse_sample("intr 1 2 3\n").is_none());
}

#[test]
fn memory_uses_available_memory_and_rounds_the_percentage() {
    let info = memorybar::parse(
        "MemTotal: 8192000 kB\n\
         MemAvailable: 4096000 kB\n\
         SwapTotal: 2048000 kB\n\
         SwapFree: 1024000 kB\n",
    )
    .expect("meminfo");

    assert_eq!(info.total_kb, 8_192_000);
    assert_eq!(info.used_kb(), 4_096_000);
    assert_eq!(info.swap_used_kb(), 1_024_000);
    assert_eq!(info.percentage(), 50);
    assert_eq!(whelk_rs::format::str_gib(info.used_kb()), "3.9 GiB");
    assert!(memorybar::parse("MemFree: 4096 kB\n").is_none());
}

fn backlight_fixture(percent: &str, maximum: &str) -> TempDir {
    let root = tempfile::tempdir().expect("temporary backlight root");
    let panel = root.path().join("intel_backlight");

    fs::create_dir_all(&panel).expect("backlight fixture directory");
    fs::write(panel.join("brightness"), format!("{percent}\n")).expect("brightness fixture");
    fs::write(panel.join("max_brightness"), format!("{maximum}\n")).expect("max fixture");

    root
}

#[test]
fn backlight_rounds_and_clamps_or_hides_without_a_panel() {
    let root = backlight_fixture("50", "100");
    let panel = root.path().join("intel_backlight");

    assert_eq!(backlightbar::percent_at(root.path(), None), Some(50));
    assert_eq!(
        backlightbar::percent_at(root.path(), Some("intel_backlight")),
        Some(50)
    );

    fs::write(panel.join("brightness"), "120\n").expect("brightness fixture");
    assert_eq!(backlightbar::percent_at(root.path(), None), Some(100));

    fs::write(panel.join("max_brightness"), "0\n").expect("max fixture");
    assert_eq!(backlightbar::percent_at(root.path(), None), None);
}

#[test]
fn capslock_reads_the_first_keyboard_led() {
    let root = tempfile::tempdir().expect("temporary LED root");
    let led = root.path().join("input3::capslock");

    fs::create_dir_all(&led).expect("LED fixture directory");
    fs::write(led.join("brightness"), "1\n").expect("LED fixture");

    assert!(keyboardbar::capslock_active_at(root.path()));

    fs::write(led.join("brightness"), "0\n").expect("LED fixture");
    assert!(!keyboardbar::capslock_active_at(root.path()));
    assert!(!keyboardbar::capslock_active_at(
        &root.path().join("missing")
    ));
}

#[test]
fn caps_lock_tells_an_absent_led_from_an_unlit_one() {
    let root = tempfile::tempdir().expect("temporary LED root");

    assert!(!keyboardbar::has_capslock_led_at(root.path()));
    assert!(!keyboardbar::has_capslock_led_at(
        &root.path().join("missing")
    ));

    let led = root.path().join("input3::capslock");

    fs::create_dir_all(&led).expect("LED fixture directory");
    fs::write(led.join("brightness"), "0\n").expect("LED fixture");

    // The LED is there, so the module may show a reading; the reading itself
    // is still "unlocked".
    assert!(keyboardbar::has_capslock_led_at(root.path()));
    assert!(!keyboardbar::capslock_active_at(root.path()));

    let other = root.path().join("input3::numlock");

    fs::create_dir_all(&other).expect("LED fixture directory");
    assert!(keyboardbar::has_capslock_led_at(root.path()));
}

#[test]
fn language_uses_niris_active_marker_and_the_configured_codes() {
    let sample = "Keyboard layouts: \"English (US)\", \"Dutch\"\n\
                  * 0 English (US)\n\
                    1 Dutch\n";

    assert_eq!(
        languagebar::parse(sample),
        Some(languagebar::Layout {
            index: 0,
            name: "English (US)".to_owned()
        })
    );
    assert_eq!(
        languagebar::reading(sample, Some("us,nl")).unwrap(),
        languagebar::LanguageReading {
            text: "US".to_owned(),
            name: "English (US)".to_owned()
        }
    );
    assert_eq!(
        languagebar::reading(sample, None).unwrap().text,
        "English (US)"
    );
    assert!(languagebar::parse("English (US)\nDutch").is_none());
}

#[test]
fn language_lists_every_layout_and_names_the_active_one() {
    // niri indents the rows under its header line; the active one keeps its
    // asterisk.
    let sample = concat!(
        "Keyboard layouts: \"English (US)\", \"Dutch\", \"German\"\n",
        "  * 0 English (US)\n",
        "    1 Dutch\n",
        "    2 German\n",
    );

    assert_eq!(
        languagebar::layouts(sample),
        vec![
            languagebar::Layout {
                index: 0,
                name: "English (US)".to_owned()
            },
            languagebar::Layout {
                index: 1,
                name: "Dutch".to_owned()
            },
            languagebar::Layout {
                index: 2,
                name: "German".to_owned()
            },
        ]
    );
    assert_eq!(languagebar::active_index(sample), Some(0));

    let second = sample.replace("* 0 English (US)", "  0 English (US)");
    let second = second.replace("1 Dutch", "* 1 Dutch");

    assert_eq!(languagebar::active_index(&second), Some(1));
    assert!(languagebar::layouts("Keyboard layouts: \"English (US)\"\n").is_empty());
    assert_eq!(
        languagebar::active_index("Keyboard layouts: \"English (US)\"\n"),
        None
    );
}

#[test]
fn a_layout_click_asks_niri_for_the_next_or_a_named_layout() {
    assert_eq!(
        languagebar::next_command(),
        "niri msg action switch-layout next"
    );
    assert_eq!(
        languagebar::switch_command(2),
        "niri msg action switch-layout 2"
    );
}

#[test]
fn power_profile_uses_the_first_cli_line_and_hides_on_empty_output() {
    assert_eq!(
        powerprofilebar::reading("performance\nbalanced\n"),
        Some("performance".to_owned())
    );
    assert_eq!(powerprofilebar::reading("\n"), None);
    assert_eq!(
        powerprofilebar::reading("  balanced  \n"),
        Some("balanced".to_owned())
    );
}

#[test]
fn the_character_card_keeps_its_four_sections_in_one_order() {
    let ids = characterbar::sections()
        .iter()
        .map(|section| section.id)
        .collect::<Vec<_>>();

    assert_eq!(ids, ["accents", "emoji", "smiles", "kawaii"]);

    for section in characterbar::sections() {
        assert!(!section.title.is_empty(), "{} has no title", section.id);
        assert!(!section.characters.is_empty(), "{} is empty", section.id);

        let mut seen = std::collections::HashSet::new();

        for character in section.characters {
            assert_eq!(
                character.trim(),
                *character,
                "{} holds a padded entry",
                section.id
            );
            assert!(
                !character.contains('\n'),
                "{} holds a line break",
                section.id
            );
            assert!(
                seen.insert(*character),
                "{} repeats {character}",
                section.id
            );
        }
    }
}

#[test]
fn the_accent_section_reaches_past_latin_one() {
    let accents = characterbar::section("accents").expect("an accents section");

    for character in ["é", "ñ", "ø", "ß", "ç", "ł", "ş", "ő", "ů", "þ", "æ", "œ"] {
        assert!(
            accents.characters.contains(&character),
            "the accents lack {character}"
        );
    }

    assert!(
        characterbar::section("emoji")
            .expect("an emoji section")
            .characters
            .contains(&"😀")
    );
    assert!(
        characterbar::section("smiles")
            .expect("a smiles section")
            .characters
            .contains(&":-)")
    );
    assert!(
        characterbar::section("kawaii")
            .expect("a kawaii section")
            .characters
            .contains(&"uwu")
    );
}

#[test]
fn the_sections_option_picks_which_cards_show() {
    let ids = |list: Option<&str>| {
        characterbar::chosen_sections(list)
            .iter()
            .map(|section| section.id)
            .collect::<Vec<_>>()
    };
    let everything = vec!["accents", "emoji", "smiles", "kawaii"];

    assert_eq!(ids(None), everything);
    assert_eq!(ids(Some("")), everything);
    assert_eq!(ids(Some("emoji, kawaii")), ["emoji", "kawaii"]);
    assert_eq!(ids(Some(" SMILEs ")), ["smiles"]);
    // A list naming nothing known shows everything rather than an empty card.
    assert_eq!(ids(Some("nope,,")), everything);
}

#[test]
fn an_empty_character_copies_nothing() {
    assert!(!whelk_rs::clipboard::copy_text(""));
}

#[test]
fn the_power_profile_reading_lists_the_daemons_profiles() {
    // The daemon indents a profile by two spaces, its details by four, and
    // marks the one in use with an asterisk.
    let sample = "  performance:\n\
                  \tCpuDriver:\tamd_pstate\n\
                  \tPlatformDriver:\tplatform_profile\n\
                  \tDegraded:   no\n\
                  \n\
                  \x20 balanced:\n\
                  \tCpuDriver:\tamd_pstate\n\
                  \n\
                  * power-saver:\n\
                  \tCpuDriver:\tamd_pstate\n";

    assert_eq!(
        powerprofilebar::profiles(sample),
        ["performance", "balanced", "power-saver"]
    );
    assert_eq!(
        powerprofilebar::active(sample),
        Some("power-saver".to_owned())
    );

    // A daemon that answers `get` only has no list and no marker.
    assert!(powerprofilebar::profiles("power-saver\n").is_empty());
    assert_eq!(powerprofilebar::active("power-saver\n"), None);
    assert_eq!(
        powerprofilebar::reading("power-saver\n").as_deref(),
        Some("power-saver")
    );
}

#[test]
fn the_power_profile_menu_comes_from_the_listing_or_the_configuration() {
    let listing = "  performance:\n  balanced:\n* power-saver:\n";

    assert_eq!(
        powerprofilebar::offered(Some(listing), None),
        ["performance", "balanced", "power-saver"]
    );
    assert_eq!(
        powerprofilebar::offered(Some(listing), Some("balanced, performance")),
        ["performance", "balanced"]
    );
    assert_eq!(
        powerprofilebar::offered(None, Some("power-saver, Balanced")),
        ["power-saver", "balanced"]
    );
    assert_eq!(
        powerprofilebar::offered(None, None),
        ["performance", "balanced", "power-saver"]
    );
    // A filter that names nothing the daemon has never blanks the menu.
    assert_eq!(
        powerprofilebar::offered(Some(listing), Some("turbo")),
        ["performance", "balanced", "power-saver"]
    );
}

#[test]
fn a_power_profile_click_cycles_or_names_one_profile() {
    assert_eq!(powerprofilebar::cycle_command(), "powerprofilesctl cycle");
    assert_eq!(
        powerprofilebar::set_command("power-saver"),
        "powerprofilesctl set power-saver"
    );
    assert_eq!(
        powerprofilebar::profile_class("power-saver"),
        "profile-power-saver"
    );
    assert_eq!(
        powerprofilebar::profile_class("Balanced"),
        "profile-balanced"
    );
    assert_eq!(
        powerprofilebar::profile_class("on battery?"),
        "profile-on-battery-"
    );
}

#[test]
fn battery_reports_the_draw_and_how_worn_the_pack_is() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");
    let pack = supply.join("BAT0");

    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("type"), "Battery\n").unwrap();
    fs::write(pack.join("capacity"), "80\n").unwrap();
    fs::write(pack.join("status"), "Discharging\n").unwrap();
    fs::write(pack.join("present"), "1\n").unwrap();
    // 32 Wh left of a 40 Wh full pack made from a 50 Wh design, drawing 12 W.
    fs::write(pack.join("energy_now"), "32000000\n").unwrap();
    fs::write(pack.join("energy_full"), "40000000\n").unwrap();
    fs::write(pack.join("energy_full_design"), "50000000\n").unwrap();
    fs::write(pack.join("power_now"), "12000000\n").unwrap();

    let reading = batterybar::read_at(&supply, None).expect("a battery");

    assert_eq!(reading.power_microwatts, Some(12_000_000));
    assert_eq!(reading.health_percent, Some(80));
    assert_eq!(batterybar::format_watts(Some(12_300_000)), "12.3 W");
    assert_eq!(batterybar::format_watts(Some(5_000_000)), "5.0 W");
    assert_eq!(batterybar::format_watts(Some(0)), "-");
    assert_eq!(batterybar::format_watts(None), "-");

    // A pack that never reports a design capacity has no health to give.
    fs::remove_file(pack.join("energy_full_design")).unwrap();
    assert_eq!(
        batterybar::read_at(&supply, None).unwrap().health_percent,
        None
    );
}

#[test]
fn the_battery_detail_joins_only_what_is_known() {
    assert_eq!(
        batterybar::detail(Some(3 * 3600), Some(12_300_000), Some(80)),
        "3h 00m · 12.3 W · 80% health"
    );
    assert_eq!(batterybar::detail(None, Some(5_000_000), None), "5.0 W");
    assert_eq!(batterybar::detail(Some(0), Some(0), None), "");
    assert_eq!(batterybar::detail(None, None, None), "");
}

#[test]
fn a_full_pack_has_a_state_of_its_own() {
    assert_eq!(batterybar::battery_class(100, "Full", 30, 15), Some("full"));
    assert_eq!(
        batterybar::battery_class(100, "Charging", 30, 15),
        Some("charging")
    );
    assert_eq!(
        batterybar::battery_class(10, "Discharging", 30, 15),
        Some("critical")
    );
    assert_eq!(
        batterybar::battery_class(25, "Discharging", 30, 15),
        Some("warning")
    );
    assert_eq!(batterybar::battery_class(80, "Discharging", 30, 15), None);
}

#[test]
fn the_power_card_names_the_packs_the_draw_and_the_adapter() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    battery_pack(&supply, "AC", "Mains", "100", "Not charging", "1");
    fs::write(supply.join("AC").join("online"), "1\n").unwrap();
    battery_pack(&supply, "BAT0", "Battery", "70", "Discharging", "1");
    battery_pack(&supply, "BAT1", "Battery", "50", "Discharging", "1");

    let card = batterybar::card_at(&supply, Some("all")).expect("a card");

    assert_eq!(card.title, "Batteries");
    assert_eq!(card.capacity, 60);
    assert_eq!(card.state, "Discharging");
    assert_eq!(
        card.facts
            .iter()
            .find(|(label, _)| label == "Adapter")
            .map(|(_, value)| value.as_str()),
        Some("plugged in")
    );
    assert_eq!(card.packs.len(), 2);
    assert_eq!(card.packs[0].0, "BAT0");

    // One pack by name, unplugged, with no design capacity to report.
    assert_eq!(
        card.packs
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["BAT0", "BAT1"]
    );

    fs::write(supply.join("AC").join("online"), "0\n").unwrap();

    let card = batterybar::card_at(&supply, None).expect("a card");

    assert_eq!(card.title, "Battery");
    assert_eq!(card.capacity, 70);
    assert_eq!(
        card.facts
            .iter()
            .find(|(label, _)| label == "Adapter")
            .map(|(_, value)| value.as_str()),
        Some("on battery")
    );
    assert_eq!(card.packs.len(), 1);
}

#[test]
fn a_desktop_with_no_pack_has_no_power_card() {
    let root = tempfile::tempdir().unwrap();
    let supply = root.path().join("power_supply");

    fs::create_dir_all(&supply).unwrap();
    battery_pack(&supply, "AC", "Mains", "100", "Not charging", "1");

    assert!(batterybar::card_at(&supply, None).is_none());
}

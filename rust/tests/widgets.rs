// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

use std::collections::BTreeMap;

use whelk_rs::bar::ModuleSpec;
use whelk_rs::settings::SchemeValue;
use whelk_rs::widgets::{self, Area, Reminders, WidgetConfig, WidgetKind};

fn spec(kind: &str, options: &[(&str, SchemeValue)]) -> ModuleSpec {
    ModuleSpec {
        kind: kind.to_owned(),
        options: options
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn text(value: &str) -> SchemeValue {
    SchemeValue::Text(value.to_owned())
}

fn config(kind: &str, options: &[(&str, SchemeValue)]) -> WidgetConfig {
    WidgetConfig::new(spec(kind, options), 48, 64, None, None, "default")
}

#[test]
fn the_kind_catalogue_is_the_c_factory_table() {
    for (name, kind) in [
        ("clock", WidgetKind::Clock),
        ("calendar", WidgetKind::Calendar),
        ("weather", WidgetKind::Weather),
        ("battery", WidgetKind::Battery),
        ("notes", WidgetKind::Notes),
        ("reminders", WidgetKind::Reminders),
        ("world-clock", WidgetKind::WorldClock),
        ("stocks", WidgetKind::Stocks),
        ("custom", WidgetKind::Custom),
    ] {
        assert_eq!(WidgetKind::from_name(name), Some(kind));
        assert_eq!(kind.name(), name);
        assert!(!kind.default_title().is_empty());
        assert!(!kind.glyph().is_empty());
    }

    assert_eq!(WidgetKind::from_name("pomodoro"), None);
}

#[test]
fn a_bare_card_gets_its_kinds_standard_size() {
    assert_eq!(WidgetKind::Clock.standard_size(), (220, 240));
    assert_eq!(WidgetKind::Calendar.standard_size(), (340, 330));
    assert_eq!(WidgetKind::WorldClock.standard_size(), (260, 260));

    let clock = config("clock", &[]);

    assert_eq!((clock.width, clock.height), (220, 240));
    assert_eq!((clock.x, clock.y), (48, 64));
    assert_eq!(clock.monitor, "default");
    assert_eq!(clock.title(), "Clock");
    assert_eq!(clock.id(0), "widget-clock-0");
    assert_eq!(clock.object_class(), "whelk-widget-clock");
}

#[test]
fn geometry_is_clamped_the_way_the_scheme_record_clamps_it() {
    let widget = WidgetConfig::new(spec("weather", &[]), -10, -20, Some(20), Some(5), "");

    assert_eq!((widget.x, widget.y), (0, 0));
    assert_eq!((widget.width, widget.height), (100, 100));
    assert_eq!(widget.monitor, "default");

    let named = WidgetConfig::new(spec("weather", &[]), 8, 8, Some(400), Some(300), "DP-1");

    assert_eq!((named.width, named.height), (400, 300));
    assert_eq!(named.monitor, "DP-1");
}

#[test]
fn a_widget_alist_becomes_the_same_config() {
    let alist = vec![
        ("kind".to_owned(), SchemeValue::Symbol("notes".to_owned())),
        ("x".to_owned(), SchemeValue::Integer(400)),
        ("y".to_owned(), SchemeValue::Integer(64)),
        ("width".to_owned(), SchemeValue::Integer(300)),
        ("height".to_owned(), SchemeValue::Integer(260)),
        ("monitor".to_owned(), text("HDMI-1")),
        (
            "options".to_owned(),
            SchemeValue::Alist(vec![("text".to_owned(), text("Ship the release."))]),
        ),
    ];
    let widget = WidgetConfig::from_alist(&alist).expect("a widget config");

    assert_eq!(widget.kind(), Some(WidgetKind::Notes));
    assert_eq!((widget.x, widget.y), (400, 64));
    assert_eq!((widget.width, widget.height), (300, 260));
    assert_eq!(widget.monitor, "HDMI-1");
    assert_eq!(
        widget.spec.text("text").as_deref(),
        Some("Ship the release.")
    );
}

#[test]
fn a_widget_without_a_kind_is_not_a_widget() {
    assert_eq!(
        WidgetConfig::from_alist(&[("x".to_owned(), SchemeValue::Integer(1))]),
        None
    );
}

#[test]
fn a_card_wears_the_bar_modules_mouse_vocabulary() {
    let widget = config(
        "notes",
        &[
            ("on-click", text("foot -e $HOME/notes")),
            ("on-click-right", text("")),
            ("on-scroll-down", text("true")),
            ("tooltip", text("Today")),
            ("css", text("mine")),
        ],
    );

    assert_eq!(
        widget.click_commands(),
        vec![
            ("on-click", "foot -e $HOME/notes".to_owned()),
            ("on-scroll-down", "true".to_owned()),
        ]
    );
    assert_eq!(widget.tooltip().as_deref(), Some("Today"));
    assert_eq!(widget.css().as_deref(), Some("mine"));
}

#[test]
fn the_refresh_cadence_follows_the_kind_and_its_options() {
    assert_eq!(config("clock", &[]).interval(), 1);
    assert_eq!(config("calendar", &[]).interval(), 60);
    assert_eq!(config("reminders", &[]).interval(), 0);
    assert_eq!(config("notes", &[]).interval(), 0);
    assert_eq!(
        config("notes", &[("file", text("/tmp/note.md"))]).interval(),
        30
    );
    assert_eq!(
        config("custom", &[("interval", SchemeValue::Integer(1))]).interval(),
        5
    );
    assert_eq!(config("custom", &[]).interval(), 300);
}

#[test]
fn reminders_parse_state_settings_and_the_built_in_list() {
    let saved = Reminders::restore(Some("[x] Water the plants\nRead the manual"), Some("one"));
    let rows = saved.rows();

    assert_eq!(rows.len(), 2);
    assert!(rows[0].done);
    assert_eq!(rows[0].text, "Water the plants");
    assert!(!rows[1].done);
    assert_eq!(rows[1].text, "Read the manual");

    let configured = Reminders::restore(Some(""), Some("one, two\nthree"));

    assert_eq!(configured.rows().len(), 3);
    assert_eq!(
        Reminders::restore(None, None).len(),
        Reminders::parse(widgets::DEFAULT_REMINDERS).len()
    );
}

#[test]
fn an_empty_reminder_is_not_a_row() {
    let reminders = Reminders::parse(" , one ,, \n , two \n");

    assert_eq!(reminders.len(), 2);
    assert_eq!(reminders.rows()[0].text, "one");
    assert_eq!(reminders.rows()[1].text, "two");
}

#[test]
fn a_reminder_row_toggles_and_serialises_back_to_the_state_file() {
    let mut reminders = Reminders::parse("Water the plants,Read the manual");

    assert!(reminders.toggle(1));
    assert_eq!(
        reminders.serialised(),
        "Water the plants\n[x] Read the manual"
    );
    assert!(!reminders.toggle(9));
    assert!(reminders.toggle(1));
    assert_eq!(reminders.serialised(), "Water the plants\nRead the manual");
}

#[test]
fn every_done_marker_is_read_and_stripped() {
    for item in [
        "[x] Water the plants",
        "[X] Water the plants",
        "x Water the plants",
        "X Water the plants",
    ] {
        assert!(widgets::reminder_done(item), "{item}");
        assert_eq!(widgets::reminder_text(item), "Water the plants", "{item}");
    }

    assert!(!widgets::reminder_done("Water the plants"));
    assert_eq!(
        widgets::reminder_text("Water the plants"),
        "Water the plants"
    );
}

#[test]
fn a_card_draws_at_most_eight_reminders() {
    let items = (0..12)
        .map(|index| format!("item {index}"))
        .collect::<Vec<_>>()
        .join(",");

    assert_eq!(Reminders::parse(&items).rows().len(), 8);
}

#[test]
fn a_world_clock_reads_name_and_zone_entries() {
    let cities = widgets::cities("Cupertino=America/Los_Angeles, Europe/London ,Tokyo=Asia/Tokyo");

    assert_eq!(cities.len(), 3);
    assert_eq!(cities[0].label, "Cupertino");
    assert_eq!(cities[0].zone, "America/Los_Angeles");
    assert_eq!(
        cities[1].label, "London",
        "a bare zone is named after its city"
    );
    assert_eq!(cities[1].zone, "Europe/London");

    assert_eq!(widgets::cities("").len(), 3, "the built-in list");
    assert_eq!(widgets::cities(" , ").len(), 0);
}

#[test]
fn a_city_reads_a_time_and_an_unknown_zone_falls_back_to_utc() {
    let (time, day) = widgets::city_time("UTC").expect("UTC is known");

    assert_eq!(time.len(), 5);
    assert_eq!(day.len(), 3);

    // `g_time_zone_new_identifier` is not bound by glib-rs, so the binding's
    // UTC fallback is what an unknown zone gets; the C card logs a warning
    // first and then draws the same time.
    assert_eq!(
        widgets::city_time("Mars/Olympus_Mons"),
        widgets::city_time("UTC")
    );
}

#[test]
fn the_calendar_lays_out_the_month_monday_first() {
    // 1 November 2026 is a Sunday, so six empty cells precede the month.
    let grid = widgets::month_grid(2026, 11, (2026, 11, 17));

    assert_eq!(grid.heading, "November 2026");
    assert_eq!(grid.days, 30);
    assert_eq!(grid.leading, 6);
    assert_eq!(grid.today, Some(17));

    // A card showing another month does not mark today.
    assert_eq!(widgets::month_grid(2026, 10, (2026, 11, 17)).today, None);
    // February in a leap year, starting on a Monday.
    let february = widgets::month_grid(2024, 2, (2024, 2, 1));

    assert_eq!(february.days, 29);
    assert_eq!(february.leading, 3);
    assert_eq!(february.today, Some(1));
    assert_eq!(widgets::days_in_month(2026, 2), 28);
    assert_eq!(widgets::days_in_month(2000, 2), 29);
    assert_eq!(widgets::days_in_month(1900, 2), 28);
}

#[test]
fn the_weather_reading_is_the_c_cards_text() {
    assert_eq!(widgets::weather_condition(0), "Clear");
    assert_eq!(widgets::weather_condition(3), "Partly cloudy");
    assert_eq!(widgets::weather_condition(63), "Rain");
    assert_eq!(widgets::weather_condition(75), "Snow");
    assert_eq!(widgets::weather_condition(96), "Thunderstorm");
    assert_eq!(widgets::weather_condition(1234), "Thunderstorm");
    assert_eq!(widgets::weather_condition(30), "Weather");
    assert_eq!(widgets::weather_glyph(0), "☀");
    assert_eq!(widgets::weather_glyph(2), "⛅");
    assert_eq!(widgets::weather_glyph(61), "☔");

    let answer = r#"{
        "current": {"temperature_2m": 12.4, "apparent_temperature": 11.1,
                    "relative_humidity_2m": 81, "weather_code": 61},
        "daily": {"temperature_2m_max": [15.2], "temperature_2m_min": [7.8]}
    }"#;
    let reading = widgets::parse_weather(answer, true).expect("a reading");

    assert_eq!(reading.temperature, "12°C");
    assert_eq!(reading.condition, "Rain");
    assert_eq!(reading.range, "H 15°  L 8°");
    assert_eq!(reading.details, "Feels like 11° · 81% humidity");
    assert_eq!(
        widgets::parse_weather(answer, false)
            .expect("imperial")
            .temperature,
        "12°F"
    );
}

#[test]
fn weather_without_a_daily_range_is_not_a_reading() {
    assert_eq!(
        widgets::parse_weather(r#"{"current": {"temperature_2m": 12}}"#, true),
        None
    );
    assert_eq!(widgets::parse_weather("not json", true), None);
    assert_eq!(widgets::parse_weather("", true), None);
}

#[test]
fn the_weather_request_names_the_units_and_the_place() {
    let metric = widgets::weather_url("52.3676", "4.9041", false);

    assert!(metric.contains("latitude=52.3676&longitude=4.9041"));
    assert!(metric.contains("forecast_days=1"));
    assert!(!metric.contains("fahrenheit"));

    assert!(
        widgets::weather_url("37.7749", "-122.4194", true).contains("&temperature_unit=fahrenheit")
    );
}

#[test]
fn stock_symbols_are_trimmed_upper_cased_and_limited() {
    assert_eq!(
        widgets::stock_symbols("aapl, msft ,, nvda, goog, amd, intc"),
        vec!["AAPL", "MSFT", "NVDA", "GOOG", "AMD", "INTC"]
    );
    assert_eq!(widgets::stock_symbols(""), vec!["AAPL", "MSFT", "NVDA"]);
}

#[test]
fn a_quote_is_read_from_the_spark_answer() {
    let answer = r#"{
        "AAPL": {"fulldayPrice": 191.24, "fulldayChangePercent": 1.234},
        "MSFT": {"fulldayPrice": 402.5, "chartPreviousClose": 400.0},
        "NVDA": {"fulldayPrice": 0}
    }"#;
    let symbols = vec!["AAPL".to_owned(), "MSFT".to_owned(), "NVDA".to_owned()];
    let quotes = widgets::parse_stocks(answer, &symbols);

    assert_eq!(quotes.len(), 2, "a symbol without a price is left out");
    assert_eq!(quotes[0].symbol, "AAPL");
    assert_eq!(quotes[0].price_text(), "191.24");
    assert_eq!(quotes[0].change_text(), "+1.23%");
    assert!(quotes[0].is_up());
    assert_eq!(quotes[1].price_text(), "402.50");
    assert_eq!(quotes[1].change_text(), "+0.62%");
    assert_eq!(
        widgets::parse_stocks("{}", &["AAPL".to_owned()]),
        Vec::new()
    );
}

#[test]
fn the_quote_request_encodes_its_symbols() {
    assert_eq!(
        widgets::stocks_url(&["AAPL".to_owned(), "BRK.B".to_owned()]),
        "https://query1.finance.yahoo.com/v8/finance/spark?symbols=AAPL,BRK.B&range=1d&interval=1d"
    );
    assert!(widgets::stocks_url(&["A B".to_owned()]).contains("symbols=A%20B"));
}

#[test]
fn a_note_reads_its_file_then_its_text_then_says_how_to_configure_it() {
    let directory = tempfile::tempdir().expect("tempdir");
    let note = directory.path().join("note.md");

    std::fs::write(&note, "  Ship the release.\n").expect("write note");

    assert_eq!(
        widgets::note_text(note.to_str(), Some("ignored")),
        "Ship the release."
    );
    assert_eq!(
        widgets::note_text(Some("/nonexistent/note.md"), Some("Set by the file")),
        "Set by the file"
    );
    assert_eq!(widgets::note_text(None, None), widgets::DEFAULT_NOTE);
}

#[test]
fn a_custom_reading_falls_back_to_its_empty_answer() {
    assert_eq!(widgets::custom_text(Some("42"), Some("nothing")), "42");
    assert_eq!(widgets::custom_text(Some(""), Some("nothing")), "nothing");
    assert_eq!(widgets::custom_text(None, None), "No output");
    assert_eq!(widgets::custom_interval(Some(1)), 5);
    assert_eq!(widgets::custom_interval(None), 300);
}

#[test]
fn a_drag_snaps_and_stops_at_the_monitors_edge() {
    let area = Area {
        width: 1920,
        height: 1080,
    };

    assert_eq!(
        widgets::drag_position((40, 64), (11.0, -10.0), (220, 240), area),
        (48, 56)
    );
    assert_eq!(
        widgets::drag_position((40, 64), (-100.0, -100.0), (220, 240), area),
        (0, 0)
    );
    assert_eq!(
        widgets::drag_position((1700, 900), (400.0, 400.0), (220, 240), area),
        (1700, 840),
        "a card at the edge stays at the edge"
    );
}

#[test]
fn a_saved_position_wins_over_the_settings_file_and_is_clamped_again() {
    let widget = WidgetConfig::new(spec("clock", &[]), 48, 64, None, None, "default");
    let area = Area {
        width: 1920,
        height: 1080,
    };

    assert_eq!(widget.restore_position(None, area), (48, 64));
    assert_eq!(widget.restore_position(Some((800, 600)), area), (800, 600));
    assert_eq!(
        widget.restore_position(Some((1900, 1070)), area),
        (1700, 840)
    );
    assert_eq!(widgets::FALLBACK_AREA.width, 1920);
}

#[test]
fn a_placement_belongs_to_the_monitor_it_names_or_the_one_it_is_on() {
    assert_eq!(widgets::monitor_key("DP-1", Some("HDMI-1")), Some("DP-1"));
    assert_eq!(
        widgets::monitor_key("default", Some("HDMI-1")),
        Some("HDMI-1")
    );
    assert_eq!(widgets::monitor_key("", Some("HDMI-1")), Some("HDMI-1"));
    assert_eq!(widgets::monitor_key("default", None), None);
    assert_eq!(widgets::monitor_key("default", Some("")), None);
}

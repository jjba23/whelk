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
        "https://query1.finance.yahoo.com/v8/finance/spark?symbols=AAPL,BRK.B&range=1d&interval=5m"
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

// ---------------------------------------------------------------------------
// Families, anchors and a card's own material
// ---------------------------------------------------------------------------

#[test]
fn a_family_scales_a_kind_and_snaps_it_to_the_grid() {
    use widgets::{SizeFamily, family_size};

    // The same numbers `widget-family-size` in `(whelk state)` produces: two
    // halves of the shell asking about one card must not disagree.
    assert_eq!(family_size((220, 240), SizeFamily::Small), (168, 184));
    assert_eq!(family_size((220, 240), SizeFamily::Medium), (220, 240));
    assert_eq!(family_size((340, 220), SizeFamily::Large), (512, 328));

    // Medium is the kind's own size and is left alone; the other two are new
    // sizes and land on the drag grid.
    assert_eq!(family_size((340, 330), SizeFamily::Medium), (340, 330));

    for family in [SizeFamily::Small, SizeFamily::Large] {
        let (width, height) = family_size((340, 330), family);

        assert_eq!(width % widgets::GRID, 0, "{family:?} width off the grid");
        assert_eq!(height % widgets::GRID, 0, "{family:?} height off the grid");
        assert!(width >= widgets::MIN_SIZE && height >= widgets::MIN_SIZE);
    }
}

#[test]
fn a_family_only_decides_the_size_nobody_wrote_down() {
    let bare = config("weather", &[]);
    let large = bare.clone().in_family(widgets::SizeFamily::Large);

    assert_eq!((bare.width, bare.height), (340, 220));
    assert_eq!((large.width, large.height), (512, 328));

    // A configuration that wrote pixels keeps them, whatever family it names.
    let written = WidgetConfig::new(spec("weather", &[]), 10, 20, Some(400), Some(300), "DP-1");
    let card = WidgetConfig::from_alist(&[
        ("kind".to_owned(), text("weather")),
        ("width".to_owned(), SchemeValue::Integer(400)),
        ("height".to_owned(), SchemeValue::Integer(300)),
        ("size".to_owned(), SchemeValue::Symbol("large".to_owned())),
    ])
    .expect("a card reads");

    assert_eq!((card.width, card.height), (400, 300));
    assert_eq!((written.width, written.height), (400, 300));
    assert_eq!(card.size, widgets::SizeFamily::Large);
}

#[test]
fn an_anchor_is_the_offset_from_its_own_edges() {
    use widgets::{Anchor, FALLBACK_AREA, anchored_offset, anchored_origin};

    let area = Area {
        width: 1000,
        height: 700,
    };
    let size = (200, 100);
    let offset = (10, 20);

    assert_eq!(
        anchored_origin(Anchor::TopLeft, area, offset, size),
        (10, 20)
    );
    assert_eq!(
        anchored_origin(Anchor::TopRight, area, offset, size),
        (790, 20)
    );
    assert_eq!(
        anchored_origin(Anchor::BottomLeft, area, offset, size),
        (10, 580)
    );
    assert_eq!(
        anchored_origin(Anchor::BottomRight, area, offset, size),
        (790, 580)
    );
    // A centred card is centred, and its offset nudges it from the middle.
    assert_eq!(
        anchored_origin(Anchor::Center, area, (0, 0), size),
        (400, 300)
    );
    assert_eq!(anchored_origin(Anchor::Top, area, (0, 0), size), (400, 0));
    assert_eq!(
        anchored_origin(Anchor::Right, FALLBACK_AREA, (24, 0), size),
        (1920 - 200 - 24, (1080 - 100) / 2)
    );

    // The inverse is what a drag saves: every anchor round-trips.
    for anchor in [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ] {
        let origin = anchored_origin(anchor, area, offset, size);

        assert_eq!(
            anchored_offset(anchor, area, origin, size),
            offset,
            "{anchor:?} does not round-trip"
        );
    }
}

#[test]
fn a_corner_card_stays_in_its_corner_on_a_smaller_screen() {
    use widgets::{Anchor, anchored_origin};

    let wide = Area {
        width: 1920,
        height: 1080,
    };
    let small = Area {
        width: 1280,
        height: 800,
    };
    let size = (340, 220);
    let offset = (24, 24);

    let on_wide = anchored_origin(Anchor::BottomRight, wide, offset, size);
    let on_small = anchored_origin(Anchor::BottomRight, small, offset, size);

    assert_eq!(on_wide, (1920 - 340 - 24, 1080 - 220 - 24));
    assert_eq!(on_small, (1280 - 340 - 24, 800 - 220 - 24));
    // The same distance from the corner, which is the point of an anchor.
    assert_eq!(wide.width - on_wide.0, small.width - on_small.0);
    assert_eq!(wide.height - on_wide.1, small.height - on_small.1);
}

#[test]
fn a_restored_position_is_an_offset_from_the_anchor() {
    use widgets::Anchor;

    let card = config("stocks", &[]).anchored(Anchor::BottomRight);
    let area = Area {
        width: 1000,
        height: 700,
    };

    let first = card.restore_position(None, area);
    let restored = card.restore_position(Some((24, 24)), area);

    assert_eq!(first, (1000 - card.width - 48, 700 - card.height - 64));
    assert_eq!(restored, (1000 - card.width - 24, 700 - card.height - 24));
}

#[test]
fn a_card_wears_only_the_material_it_names() {
    use whelk_rs::bar::BarSettings;

    let plain = config("weather", &[]);

    assert_eq!(plain.skin(), None);

    let card = config(
        "weather",
        &[
            ("opacity", SchemeValue::Number(0.6)),
            ("accent", text("#80ff80")),
            ("radius", SchemeValue::Integer(18)),
        ],
    );
    let material = card.skin().expect("the card names a material");
    let mut bar = BarSettings {
        opacity: 0.9,
        background: Some("#101010".to_owned()),
        blur: 12,
        ..BarSettings::default()
    };

    material.apply(&mut bar);

    assert_eq!(bar.opacity, 0.6);
    assert_eq!(bar.radius, 18);
    assert_eq!(bar.accent.as_deref(), Some("#80ff80"));
    // What the card did not name is still the bar's.
    assert_eq!(bar.background.as_deref(), Some("#101010"));
    assert_eq!(bar.blur, 12);
}

#[test]
fn a_card_says_how_often_it_reads() {
    assert_eq!(config("weather", &[]).interval(), 1800);
    assert_eq!(
        config("weather", &[("interval", SchemeValue::Integer(5))]).interval(),
        5
    );
    assert_eq!(
        config("weather", &[("interval", SchemeValue::Integer(0))]).interval(),
        0
    );
    // A command a second is a process a second: a custom card keeps its floor.
    assert_eq!(
        config("custom", &[("interval", SchemeValue::Integer(1))]).interval(),
        widgets::custom_interval(Some(1))
    );
}

#[test]
fn a_note_loses_its_ragged_whitespace_and_not_its_paragraphs() {
    assert_eq!(
        widgets::note_display("  Groceries  \n\n\n\n- milk\n\t- bread   \n\n"),
        "  Groceries\n\n- milk\n    - bread",
        "one blank line between paragraphs, no tabs, no trailing spaces"
    );
    assert_eq!(
        widgets::note_display("\n\n\nsingle line\n\n"),
        "single line"
    );
    assert_eq!(widgets::note_display(""), "");
}

#[test]
fn the_family_and_the_anchor_survive_the_bridge() {
    let card = WidgetConfig::from_alist(&[
        ("kind".to_owned(), text("calendar")),
        ("x".to_owned(), SchemeValue::Integer(12)),
        ("y".to_owned(), SchemeValue::Integer(16)),
        ("size".to_owned(), SchemeValue::Symbol("small".to_owned())),
        (
            "anchor".to_owned(),
            SchemeValue::Symbol("bottom-right".to_owned()),
        ),
    ])
    .expect("a card reads");

    assert_eq!(card.size, widgets::SizeFamily::Small);
    assert_eq!(card.anchor, widgets::Anchor::BottomRight);
    assert_eq!((card.width, card.height), (256, 248));
}

#[test]
fn the_added_kinds_are_in_the_catalogue_and_keep_their_cadence() {
    for (name, kind, interval) in [
        ("now-playing", WidgetKind::NowPlaying, 5),
        ("visualizer", WidgetKind::Visualizer, 0),
        ("media", WidgetKind::Media, 1),
        ("photos", WidgetKind::Photos, 30),
        ("files", WidgetKind::Files, 60),
        ("news", WidgetKind::News, 1800),
        ("system", WidgetKind::System, 5),
    ] {
        assert_eq!(WidgetKind::from_name(name), Some(kind));
        assert_eq!(kind.name(), name);
        assert_eq!(kind.interval(), interval);
        assert!(!kind.default_title().is_empty());
        assert!(!kind.glyph().is_empty());
    }

    // A media player is also spelled the way the bar module is.
    assert_eq!(WidgetKind::from_name("music"), Some(WidgetKind::NowPlaying));
    assert_eq!(
        WidgetKind::from_name("spectrum"),
        Some(WidgetKind::Visualizer)
    );
    assert_eq!(
        WidgetKind::from_name("media-controls"),
        Some(WidgetKind::Media)
    );
}

#[test]
fn a_visualizer_frame_parses_cava_and_holds_its_peaks() {
    assert_eq!(
        widgets::parse_visualizer_frame("0;50;100;nonsense;150;", 4),
        vec![0.0, 0.5, 1.0, 1.0]
    );
    assert!(widgets::parse_visualizer_frame("", 8).is_empty());

    let mut frame = widgets::VisualizerFrame::new(2);

    frame.push(&[1.0, 0.5]);
    assert_eq!(frame.levels, vec![1.0, 0.5]);
    assert_eq!(frame.peaks, vec![1.0, 0.5]);

    frame.push(&[0.0, 0.25]);
    assert_eq!(frame.levels, vec![0.0, 0.25]);
    assert!(frame.peaks[0] < 1.0 && frame.peaks[0] > 0.9);
    assert!(frame.peaks[1] < 0.5 && frame.peaks[1] > 0.4);

    let config = widgets::visualizer_config(8, 20);

    assert!(config.contains("bars = 8"));
    assert!(config.contains("framerate = 20"));
    assert!(config.contains("data_format = ascii"));
    assert!(config.contains("bar_delimiter = 59"));
}

#[test]
fn a_player_reading_becomes_the_cards_words() {
    use whelk_rs::mpris::{Playback, Reading, Track};

    let reading = Reading {
        player: Some("spotify".to_owned()),
        track: Track {
            title: "Golden Gate".to_owned(),
            artist: "The Shells".to_owned(),
            album: "Harbour".to_owned(),
            length_us: 200_000_000,
            ..Track::default()
        },
        playback: Playback::Playing,
        position_us: 50_000_000,
        players: 1,
    };
    let text = widgets::now_playing_text(&reading);

    assert!(text.available);
    assert_eq!(text.title, "Golden Gate");
    assert_eq!(text.detail(), "The Shells — Harbour");
    assert_eq!(text.percent, 25);
    assert_eq!(text.timeline(), "0:50 / 3:20");
    assert!(text.playing);
    assert_eq!(text.player, "spotify");
    assert_eq!(
        text.glyph, "▶",
        "a card draws a transport mark any font has"
    );

    let idle = widgets::now_playing_text(&Reading::default());

    assert!(!idle.available);
    assert_eq!(idle.title, "Nothing playing");
    assert_eq!(idle.detail(), "");
    assert_eq!(idle.timeline(), "0:00");
    assert_eq!(idle.glyph, "⏹");
}

#[test]
fn a_photo_card_walks_its_roots_and_wraps_at_both_ends() {
    let directory = tempfile::tempdir().expect("tempdir");
    let nested = directory.path().join("nested");

    std::fs::create_dir(&nested).expect("nested dir");
    std::fs::write(directory.path().join("a.png"), b"").expect("a.png");
    std::fs::write(directory.path().join("b.jpg"), b"").expect("b.jpg");
    std::fs::write(directory.path().join("not-a-picture.txt"), b"").expect("txt");
    std::fs::write(directory.path().join(".hidden.png"), b"").expect("hidden");
    std::fs::write(nested.join("c.webp"), b"").expect("c.webp");

    let files = widgets::photo_files(&[directory.path().to_path_buf()]);
    let names: Vec<String> = files
        .iter()
        .map(|path| {
            path.strip_prefix(directory.path())
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(names, vec!["a.png", "b.jpg", "nested/c.webp"]);
    assert_eq!(widgets::photo_step(0, 3, -1), 2);
    assert_eq!(widgets::photo_step(2, 3, 1), 0);
    assert_eq!(widgets::photo_step(4, 0, 1), 0);
    assert_eq!(
        widgets::photo_caption(std::path::Path::new("/tmp/My-Trip_2024.jpg")),
        "My Trip 2024"
    );
    assert_eq!(
        widgets::photo_paths(" ~/Pictures , /srv/photos ,, "),
        vec![
            std::path::PathBuf::from("~/Pictures"),
            std::path::PathBuf::from("/srv/photos")
        ]
    );
}

#[test]
fn a_files_card_reads_the_desktops_recent_documents() {
    let xbel = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file:///home/joe/Notes/Plan%20A.md" modified="2026-11-24T10:00:00Z">
    <info><metadata owner="whelk"><title>Plan A</title></metadata></info>
  </bookmark>
  <bookmark href="https://example.com/post"><title>Post &amp; reply</title></bookmark>
  <bookmark href="file:///tmp/My%20Doc.txt"></bookmark>
  <bookmark href=""><title>No address</title></bookmark>
</xbel>"#;
    let files = widgets::parse_recent(xbel, 10);

    assert_eq!(files.len(), 3);
    assert_eq!(files[0].title, "Plan A");
    assert_eq!(files[0].kind, "Local");
    assert_eq!(files[1].title, "Post & reply");
    assert_eq!(files[1].kind, "Web");
    assert_eq!(
        files[2].title, "My Doc.txt",
        "a record without a title is named by its URI"
    );
    assert_eq!(widgets::parse_recent(xbel, 1).len(), 1);
    assert_eq!(
        widgets::uri_display_name("https://example.com/notes/hello%20world?x=1"),
        "hello world"
    );
    assert_eq!(
        widgets::uri_display_name("file:///tmp/100%25.txt"),
        "100%.txt"
    );
    assert_eq!(
        widgets::uri_display_name("file:///tmp/50%€.txt"),
        "50%€.txt",
        "a percent followed by a multi-byte character is not a three-byte escape"
    );
    assert_eq!(widgets::uri_display_name("file:///tmp/done%"), "done%");
    assert_eq!(
        widgets::recent_xbel_path(Some(std::path::Path::new("/data")), None),
        Some(std::path::PathBuf::from("/data/recently-used.xbel"))
    );
}

#[test]
fn a_news_card_reads_rss_and_atom() {
    let rss = r#"<rss><channel>
      <item><title>First &amp; best</title><link>https://example.com/1</link>
        <pubDate>Mon, 24 Nov 2026 10:00:00 GMT</pubDate></item>
      <item><title><![CDATA[Second]]></title><link>https://example.com/2</link></item>
    </channel></rss>"#;
    let atom = r#"<feed><entry><title>Atom one</title>
      <link href="https://example.org/a"/><updated>2026-11-24T10:00:00Z</updated>
    </entry></feed>"#;
    let items = widgets::parse_feed(rss, "example.com", 6);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "First & best");
    assert_eq!(items[0].link, "https://example.com/1");
    assert_eq!(items[0].source, "example.com");
    assert_eq!(items[0].published, "Mon, 24 Nov 2026 10:00:00 GMT");
    assert_eq!(items[1].title, "Second");

    let atom = widgets::parse_feed(atom, "example.org", 6);

    assert_eq!(atom.len(), 1);
    assert_eq!(atom[0].title, "Atom one");
    assert_eq!(atom[0].link, "https://example.org/a");
    assert_eq!(atom[0].published, "2026-11-24T10:00:00Z");
    assert_eq!(widgets::parse_feed("not a feed", "x", 6), Vec::new());

    assert_eq!(
        widgets::feed_urls(
            "https://a.example/feed, nope, https://a.example/feed, http://b.example/rss"
        ),
        vec!["https://a.example/feed", "http://b.example/rss"]
    );
}

#[test]
fn a_system_card_reads_the_kernels_own_numbers() {
    let load = widgets::parse_loadavg("0.52 0.48 0.44 1/1234 5678").expect("a load");

    assert!((load.one - 0.52).abs() < 1e-9);
    assert!((load.five - 0.48).abs() < 1e-9);
    assert!((load.fifteen - 0.44).abs() < 1e-9);
    assert_eq!(widgets::load_text(load), "0.52  0.48  0.44");
    assert_eq!(widgets::parse_loadavg(""), None);

    assert_eq!(widgets::parse_uptime("1234.56 2345.67"), Some(1235));
    assert_eq!(widgets::parse_uptime("not a number"), None);
    assert_eq!(widgets::uptime_text(59), "0 min");
    assert_eq!(widgets::uptime_text(3600), "1 h");
    assert_eq!(widgets::uptime_text(3660), "1 h 1 min");
    assert_eq!(widgets::uptime_text(86_400), "1 day");
    assert_eq!(widgets::uptime_text(2 * 86_400), "2 days");
    assert_eq!(widgets::uptime_text(3 * 86_400 + 4 * 3600), "3 days, 4 h");
    assert_eq!(
        widgets::memory_text(2 * 1024 * 1024, 8 * 1024 * 1024),
        "2.0 GiB of 8.0 GiB"
    );
}

#[test]
fn a_forecast_is_read_beside_the_current_conditions() {
    let answer = r#"{
        "current": {"temperature_2m": 12, "apparent_temperature": 11,
                    "relative_humidity_2m": 80, "weather_code": 61},
        "daily": {
            "time": ["2026-11-24", "2026-11-25", "2026-11-26"],
            "temperature_2m_max": [15, 18, 12],
            "temperature_2m_min": [7, 9, 5],
            "weather_code": [61, 0, 3]
        }
    }"#;
    let reading = widgets::parse_weather(answer, true).expect("a reading");

    assert_eq!(reading.forecast.len(), 3);
    assert_eq!(reading.forecast[0].label, "Tue");
    assert_eq!(reading.forecast[0].glyph, "☔");
    assert_eq!(reading.forecast[0].range, "15°  7°");
    assert_eq!(reading.forecast[1].label, "Wed");
    assert_eq!(reading.forecast[2].label, "Thu");
    assert!(
        widgets::weather_url_days("1", "2", false, 3).contains("forecast_days=3"),
        "a card that draws three days asks for three"
    );
    assert_eq!(
        widgets::parse_weather(
            r#"{"current": {"temperature_2m": 1}, "daily": {"temperature_2m_max": [2], "temperature_2m_min": [0]}}"#,
            true
        )
        .expect("no forecast is still a reading")
        .forecast
        .len(),
        0
    );
}

#[test]
fn a_quote_carries_the_days_shape() {
    let answer = r#"{
        "AAPL": {"fulldayPrice": 191.24, "fulldayChangePercent": 1.234,
                 "close": [188.0, 189.5, 187.2, 191.24]}
    }"#;
    let quotes = widgets::parse_stocks(answer, &["AAPL".to_owned()]);

    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].spark, vec![188.0, 189.5, 187.2, 191.24]);

    let rising = widgets::sparkline_points(&[1.0, 2.0, 3.0], 100.0, 20.0);

    assert_eq!(rising, vec![(0.0, 20.0), (50.0, 10.0), (100.0, 0.0)]);
    assert_eq!(
        widgets::sparkline_points(&[5.0, 5.0], 10.0, 10.0),
        vec![(0.0, 5.0), (10.0, 5.0)],
        "a flat line is drawn down the middle"
    );
    assert_eq!(
        widgets::sparkline_points(&[42.0], 10.0, 10.0),
        vec![(5.0, 5.0)]
    );
    assert_eq!(widgets::sparkline_points(&[], 10.0, 10.0), Vec::new());
}

#[test]
fn a_calendar_can_number_its_weeks_the_way_iso_does() {
    // 1 January 2026 is a Thursday, so it belongs to week 1.
    assert_eq!(widgets::iso_week(2026, 1, 1), 1);
    assert_eq!(widgets::iso_week(2026, 11, 24), 48);
    assert_eq!(widgets::iso_week(2026, 12, 31), 53);

    let grid = widgets::month_grid(2026, 11, (2026, 11, 24));

    assert_eq!(grid.weeks.len(), 6);
    assert_eq!(grid.weeks[0], 44, "1 November is in the week of 26 October");
    assert_eq!(grid.weeks[4], 48);
    assert_eq!(grid.weeks[5], 49, "the last row starts on 30 November");
    assert_eq!(widgets::month_grid(2026, 2, (2026, 2, 1)).weeks.len(), 5);
}

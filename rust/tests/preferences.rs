// The settings window's model, tested without a display: how a typed value is
// written back to Scheme, how a module's override key is named, which options a
// text field may edit, and the two converted fields the window draws.

use std::collections::BTreeMap;

use whelk_rs::bar::{BarSettings, ModuleSpec};
use whelk_rs::preferences::{self, OptionKind, PreferenceValue};
use whelk_rs::settings::SchemeValue;

fn spec(kind: &str, options: &[(&str, SchemeValue)]) -> ModuleSpec {
    ModuleSpec {
        kind: kind.to_owned(),
        options: options
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn a_typed_value_is_written_as_the_scheme_it_came_from() {
    assert_eq!(PreferenceValue::Bool(true).to_scheme(), "#t");
    assert_eq!(PreferenceValue::Bool(false).to_scheme(), "#f");
    assert_eq!(PreferenceValue::Integer(120).to_scheme(), "120");
    assert_eq!(PreferenceValue::Decimal(0.75).to_scheme(), "0.75");
    assert_eq!(PreferenceValue::Decimal(1.0).to_scheme(), "1");
    assert_eq!(
        PreferenceValue::Text("Inter".to_owned()).to_scheme(),
        "\"Inter\""
    );
    // A module option that arrived as a symbol must go back as one, or
    // `#:accelerator-style 'cua` becomes the string "cua".
    assert_eq!(
        PreferenceValue::Symbol("cua".to_owned()).to_scheme(),
        "'cua"
    );
    assert_eq!(
        PreferenceValue::Text("a \"b\" c".to_owned()).to_scheme(),
        "\"a \\\"b\\\" c\""
    );
}

#[test]
fn an_override_key_names_the_kind_and_its_occurrence() {
    assert_eq!(
        preferences::module_key("clock", 1, "format"),
        "module-clock-1-format"
    );
    assert_eq!(
        preferences::module_visible_key("wifi", 0),
        "module-wifi-0-visible?"
    );
}

#[test]
fn modules_are_numbered_per_kind_across_the_whole_bar() {
    let bar = BarSettings {
        left: vec![spec("clock", &[]), spec("wifi", &[])],
        center: vec![],
        right: vec![spec("clock", &[]), spec("tray", &[])],
        overflow: vec![spec("wifi", &[])],
        ..BarSettings::default()
    };
    let cards = preferences::module_cards(&bar);

    let named: Vec<(String, usize, &str)> = cards
        .iter()
        .map(|card| (card.kind.clone(), card.occurrence, card.group))
        .collect();

    assert_eq!(
        named,
        vec![
            ("clock".to_owned(), 0, "Left"),
            ("wifi".to_owned(), 0, "Left"),
            ("clock".to_owned(), 1, "Right"),
            ("tray".to_owned(), 0, "Right"),
            ("wifi".to_owned(), 1, "Overflow"),
        ]
    );
}

#[test]
fn only_options_a_text_field_can_write_back_are_offered() {
    let bar = BarSettings {
        right: vec![spec(
            "custom",
            &[
                ("exec", SchemeValue::Text("printf hi".to_owned())),
                ("ratio", SchemeValue::Number(0.75)),
                ("interval", SchemeValue::Integer(30)),
                ("hidden?", SchemeValue::Bool(false)),
                ("tooltip", SchemeValue::Text("two\nlines".to_owned())),
                (
                    "choices",
                    SchemeValue::List(vec![SchemeValue::Text("one".to_owned())]),
                ),
                ("fraction", SchemeValue::Other),
                ("style", SchemeValue::Symbol("digital".to_owned())),
            ],
        )],
        ..BarSettings::default()
    };
    let cards = preferences::module_cards(&bar);
    let card = &cards[0];

    let offered: Vec<&str> = card
        .options
        .iter()
        .map(|option| option.name.as_str())
        .collect();

    assert_eq!(offered, vec!["exec", "interval", "ratio", "style"]);
    assert!(card.skipped_line_breaks);
    assert!(card.skipped_lists);

    let kinds: Vec<&OptionKind> = card.options.iter().map(|option| &option.kind).collect();

    assert!(matches!(kinds[0], OptionKind::Text(text) if text == "printf hi"));
    assert!(matches!(kinds[1], OptionKind::Integer(30)));
    assert!(matches!(kinds[2], OptionKind::Number(value) if (*value - 0.75).abs() < 1e-9));
    assert!(matches!(kinds[3], OptionKind::Symbol(text) if text == "digital"));
}

#[test]
fn a_scheme_modules_update_token_is_not_offered_for_typing() {
    let bar = BarSettings {
        right: vec![spec(
            "scheme",
            &[
                ("update", SchemeValue::Integer(1)),
                ("interval", SchemeValue::Integer(2)),
            ],
        )],
        ..BarSettings::default()
    };
    let cards = preferences::module_cards(&bar);

    assert_eq!(cards[0].options.len(), 1);
    assert_eq!(cards[0].options[0].name, "interval");
}

#[test]
fn a_clock_is_minutes_to_the_runtime_and_hh_mm_to_a_person() {
    assert_eq!(preferences::clock_text(20 * 60 + 30), "20:30");
    assert_eq!(preferences::clock_text(6 * 60 + 5), "06:05");
    assert_eq!(preferences::parse_clock_text("06:15"), Some(6 * 60 + 15));
    assert_eq!(preferences::parse_clock_text("23:59"), Some(23 * 60 + 59));
    // A field still being typed is not a value yet.
    assert_eq!(preferences::parse_clock_text("2"), None);
    assert_eq!(preferences::parse_clock_text("20:"), None);
    assert_eq!(preferences::parse_clock_text("24:00"), None);
}

#[test]
fn launcher_modes_read_as_the_words_the_card_uses() {
    assert_eq!(preferences::launcher_mode_label("app"), "Applications");
    assert_eq!(preferences::launcher_mode_label("calc"), "Calculator");
    assert_eq!(preferences::launcher_mode_label("menuitem"), "Menu items");
    assert_eq!(preferences::launcher_mode_label("custom"), "custom");
}

#[test]
fn a_colour_is_written_as_the_text_gdk_reads_back() {
    assert_eq!(preferences::color_text(Some("#ff8800")), "rgb(255,136,0)");
    assert_eq!(
        preferences::color_text(Some("rgba(10, 20, 30, 0.5)")),
        "rgba(10,20,30,0.5)"
    );
    // An unset or unparsable colour is black, as the C control falls back.
    assert_eq!(preferences::color_text(None), "rgb(0,0,0)");
    assert_eq!(preferences::color_text(Some("not a colour")), "rgb(0,0,0)");
}

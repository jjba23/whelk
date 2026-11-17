// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The module host's decisions are testable without a display: shared spec
// defaults, one request at a time, a late answer never matching the current
// token, and the click/scroll vocabulary read from the same option names as
// the C module factory.

use std::collections::BTreeMap;
use std::time::Duration;

use whelk_rs::bar::{BarSettings, ModuleSpec};
use whelk_rs::barfactory;
use whelk_rs::barmodule::{ModuleSettings, ModuleState};
use whelk_rs::session::Session;
use whelk_rs::settings::SchemeValue;

fn module(kind: &str, options: Vec<(&str, SchemeValue)>) -> ModuleSpec {
    ModuleSpec {
        kind: kind.to_owned(),
        options: options
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn defaults_match_the_c_module_factory() {
    let settings = ModuleSettings::from_module(&module("workspaces", Vec::new()), 1);

    assert_eq!(settings.interval_seconds, 1);
    assert_eq!(settings.refresh_interval(), Some(1));
    assert_eq!(settings.command_timeout, Duration::from_secs(5));
    assert_eq!(settings.css_class, None);
    assert!(!settings.actions.has_clicks());
    assert!(!settings.actions.has_scroll());
}

#[test]
fn interval_zero_paints_once_and_negative_means_zero() {
    let zero = ModuleSettings::from_module(
        &module("clock", vec![("interval", SchemeValue::Integer(0))]),
        1,
    );

    assert_eq!(zero.interval_seconds, 0);
    assert_eq!(zero.refresh_interval(), None);

    let negative = ModuleSettings::from_module(
        &module("clock", vec![("interval", SchemeValue::Integer(-4))]),
        1,
    );

    assert_eq!(negative.refresh_interval(), None);
}

#[test]
fn command_timeout_is_bounded_and_zero_disables_it() {
    let custom = ModuleSettings::from_module(
        &module("clock", vec![("command-timeout", SchemeValue::Integer(12))]),
        1,
    );

    assert_eq!(custom.command_timeout, Duration::from_secs(12));

    let disabled = ModuleSettings::from_module(
        &module("clock", vec![("command-timeout", SchemeValue::Integer(0))]),
        1,
    );

    assert_eq!(disabled.command_timeout, Duration::ZERO);

    let capped = ModuleSettings::from_module(
        &module(
            "clock",
            vec![("command-timeout", SchemeValue::Integer(99_999))],
        ),
        1,
    );

    assert_eq!(capped.command_timeout, Duration::from_secs(60 * 60));
}

#[test]
fn click_and_scroll_options_are_shared() {
    let settings = ModuleSettings::from_module(
        &module(
            "clock",
            vec![
                ("on-click", SchemeValue::Text("primary".into())),
                ("on-click-middle", SchemeValue::Text("middle".into())),
                ("on-click-right", SchemeValue::Text("right".into())),
                ("on-scroll-up", SchemeValue::Text("up".into())),
                ("on-scroll-down", SchemeValue::Text("down".into())),
                ("css-class", SchemeValue::Text("mine".into())),
            ],
        ),
        1,
    );

    assert!(settings.actions.has_clicks());
    assert!(settings.actions.has_scroll());
    assert_eq!(settings.actions.primary.as_deref(), Some("primary"));
    assert_eq!(settings.actions.middle.as_deref(), Some("middle"));
    assert_eq!(settings.actions.secondary.as_deref(), Some("right"));
    assert_eq!(settings.actions.scroll_up.as_deref(), Some("up"));
    assert_eq!(settings.actions.scroll_down.as_deref(), Some("down"));
    assert_eq!(settings.css_class.as_deref(), Some("mine"));
}

#[test]
fn empty_action_values_do_not_install_controllers() {
    let settings = ModuleSettings::from_module(
        &module(
            "clock",
            vec![
                ("on-click", SchemeValue::Text(String::new())),
                ("on-scroll-up", SchemeValue::Text(String::new())),
            ],
        ),
        1,
    );

    assert!(!settings.actions.has_clicks());
    assert!(!settings.actions.has_scroll());
}

#[test]
fn one_request_at_a_time_and_stale_tokens_are_rejected() {
    let mut state = ModuleState::default();
    let first = state.begin().expect("first request");

    assert!(state.is_in_flight());
    assert_eq!(state.begin(), None);

    assert!(state.finish(first));
    assert!(!state.is_in_flight());

    let second = state.begin().expect("second request");

    state.cancel();

    // The cancelled answer is much later than the token it names.
    assert!(!state.finish(second));
    assert!(!state.is_in_flight());

    let third = state.begin().expect("third request");

    assert_ne!(third, first);
    assert!(!state.finish(second));
    assert!(state.finish(third));
}

#[test]
fn factory_has_the_daily_use_modules_and_rejects_unknown_kinds() {
    for kind in [
        "audio",
        "backlight",
        "battery",
        "bluetooth",
        "clock",
        "clipboard",
        "cpu",
        "custom",
        "dnd",
        "inhibit",
        "keyboard",
        "language",
        "memory",
        "menu",
        "notifications",
        "power-profile",
        "start",
        "tasks",
        "tray",
        "wallpaper",
        "wifi",
        "window",
        "workspaces",
    ] {
        assert!(barfactory::supports(kind), "{kind} is not registered");
        assert!(barfactory::factory(kind).is_some());
    }

    assert!(!barfactory::supports("unknown"));
    assert!(barfactory::factory("unknown").is_none());
}

#[test]
fn factory_context_carries_the_bar_facts() {
    let bar = BarSettings {
        spacing: 10,
        icon_font: Some("Icons".to_owned()),
        ..BarSettings::default()
    };

    let context = barfactory::ModuleContext::from_bar(&bar, Session::Niri);

    assert_eq!(context.session, Session::Niri);
    assert_eq!(context.spacing, 10);
    assert_eq!(context.icon_font.as_deref(), Some("Icons"));
    assert_eq!(context.bar.spacing, 10);
    assert!(context.shell_menu.is_empty());
}

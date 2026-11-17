// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

// The night light's arithmetic, without a screen: the colour of a temperature,
// the shape of a scheduled window, and the gamma table a display server gets.

use whelk_rs::nightlight::{
    Backend, Choice, Config, Engine, GammaRamp, MAX_TEMPERATURE, MIN_TEMPERATURE, Mode,
    NEUTRAL_TEMPERATURE, Rgb, choice, color, mix, next_tick_seconds, parse_clock, strength,
    wake_seconds,
};

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}

fn scheduled(start: i32, end: i32, transition: i32) -> Config {
    Config {
        enabled: true,
        mode: Mode::Scheduled,
        start_minute: start,
        end_minute: end,
        transition_minutes: transition,
        ..Config::default()
    }
}

#[test]
fn neutral_temperature_is_exactly_white() {
    let rgb = color(NEUTRAL_TEMPERATURE);

    for channel in rgb.channels() {
        assert_close(channel, 1.0);
    }

    assert!(rgb.is_neutral());
}

#[test]
fn warm_temperatures_reduce_blue_without_lifting_red() {
    let warm = color(3_400);
    let warmest = color(1_000);

    assert!(warm.red() >= warm.green());
    assert!(warm.green() > warm.blue());
    assert!(warm.blue() < 0.75);
    assert!(warm.red() <= 1.0);
    assert!(warmest.blue() < 0.01);
    assert!(warmest.green() > 0.0);
}

#[test]
fn temperatures_are_clamped_to_the_supported_range() {
    let too_cold = color(-4_000);
    let coldest = color(MIN_TEMPERATURE);
    let too_hot = color(12_000);
    let neutral = color(NEUTRAL_TEMPERATURE);

    for channel in 0..3 {
        assert_close(too_cold.channels()[channel], coldest.channels()[channel]);
        assert_close(too_hot.channels()[channel], neutral.channels()[channel]);
    }
}

#[test]
fn an_overnight_window_crosses_midnight() {
    let config = scheduled(19 * 60, 7 * 60, 30);

    assert_eq!(strength(&config, 12.0 * 60.0), 0.0);
    assert_eq!(strength(&config, 18.0 * 60.0 + 59.0), 0.0);
    assert_eq!(strength(&config, 19.0 * 60.0), 0.0);
    assert_close(strength(&config, 19.0 * 60.0 + 15.0), 0.5);
    assert_eq!(strength(&config, 23.0 * 60.0), 1.0);
    assert_eq!(strength(&config, 0.0), 1.0);
    assert_close(strength(&config, 6.0 * 60.0 + 45.0), 0.5);
    assert_eq!(strength(&config, 7.0 * 60.0), 0.0);
}

#[test]
fn a_daytime_window_has_no_fade_when_its_transition_is_zero() {
    let config = scheduled(8 * 60, 17 * 60, 0);

    assert_eq!(strength(&config, 7.0 * 60.0), 0.0);
    assert_eq!(strength(&config, 8.0 * 60.0), 1.0);
    assert_eq!(strength(&config, 12.0 * 60.0), 1.0);
    assert_eq!(strength(&config, 17.0 * 60.0), 0.0);
}

#[test]
fn disabled_always_and_all_day_modes_are_distinct() {
    let disabled = Config {
        enabled: false,
        ..scheduled(19 * 60, 7 * 60, 30)
    };
    let always = Config {
        mode: Mode::Always,
        ..scheduled(19 * 60, 7 * 60, 30)
    };
    let all_day = scheduled(12 * 60, 12 * 60, 0);

    assert_eq!(strength(&disabled, 23.0 * 60.0), 0.0);
    assert_eq!(strength(&always, 12.0 * 60.0), 1.0);
    // Equal ends have no dark half to describe; the useful reading is all day.
    assert_eq!(strength(&all_day, 3.0 * 60.0), 1.0);
}

#[test]
fn a_transition_longer_than_the_window_meets_in_the_middle() {
    let config = scheduled(12 * 60, 13 * 60, 120);

    assert_eq!(strength(&config, 12.0 * 60.0), 0.0);
    assert_close(strength(&config, 12.0 * 60.0 + 15.0), 0.5);
    assert_eq!(strength(&config, 12.0 * 60.0 + 30.0), 1.0);
    assert_close(strength(&config, 12.0 * 60.0 + 45.0), 0.5);
    assert_eq!(strength(&config, 13.0 * 60.0), 0.0);
}

#[test]
fn mix_moves_from_neutral_to_the_target_and_clamps_strength() {
    let target = color(3_400);

    for channel in 0..3 {
        assert_close(mix(3_400, 0.0).channels()[channel], 1.0);
        assert_close(
            mix(3_400, 1.0).channels()[channel],
            target.channels()[channel],
        );
        assert_close(
            mix(3_400, 0.5).channels()[channel],
            (1.0 + target.channels()[channel]) / 2.0,
        );
    }

    assert_close(mix(3_400, -2.0).blue(), 1.0);
    assert_close(mix(3_400, 4.0).blue(), target.blue());
}

#[test]
fn next_tick_follows_fades_and_avoids_long_sleeps() {
    let config = scheduled(19 * 60, 7 * 60, 30);
    let always = Config {
        mode: Mode::Always,
        ..scheduled(19 * 60, 7 * 60, 30)
    };

    // The opening edge is still at zero strength, but the fade is about to
    // move, so waking after a minute would make every schedule start late.
    assert_eq!(next_tick_seconds(&config, 19.0 * 60.0), 5);
    assert_eq!(next_tick_seconds(&config, 19.0 * 60.0 + 15.0), 5);
    assert_eq!(next_tick_seconds(&config, 12.0 * 60.0), 60);
    assert!(next_tick_seconds(&config, 18.0 * 60.0 + 59.0) <= 60);
    assert_eq!(next_tick_seconds(&always, 12.0 * 60.0), 300);
}

#[test]
fn clocks_are_strict_hh_mm() {
    assert_eq!(parse_clock("07:05"), Some(7 * 60 + 5));
    assert_eq!(parse_clock("23:59"), Some(23 * 60 + 59));

    for invalid in ["24:00", "7", "seven", "07:5", "07:005", "-1:00"] {
        assert_eq!(parse_clock(invalid), None);
    }
}

#[test]
fn the_ramp_is_an_identity_curve_scaled_per_channel() {
    let ramp = GammaRamp::new(4, Rgb::new([1.0, 0.5, 0.25]));

    assert_eq!(ramp.size(), 4);
    assert_eq!(ramp.red(), [0, 21_845, 43_690, 65_535]);
    assert_eq!(ramp.green(), [0, 10_923, 21_845, 32_768]);
    assert_eq!(ramp.blue(), [0, 5_461, 10_923, 16_384]);
}

#[test]
fn a_one_entry_ramp_is_full_scale() {
    let two = GammaRamp::new(2, Rgb::NEUTRAL);
    let one = GammaRamp::new(1, Rgb::new([1.0, 0.5, 0.0]));

    assert_eq!(two.red(), [0, 65_535]);
    assert_eq!(two.green(), [0, 65_535]);
    assert_eq!(two.blue(), [0, 65_535]);
    assert_eq!(one.red(), [65_535]);
    assert_eq!(one.green(), [32_768]);
    assert_eq!(one.blue(), [0]);
}

#[test]
fn defaults_and_normalization_match_the_c_configuration() {
    let mut config = Config::default();

    assert!(!config.configured);
    assert!(!config.enabled);
    assert_eq!(config.mode, Mode::Scheduled);
    assert_eq!(config.temperature, 3_400);
    assert_eq!(config.start_minute, 19 * 60);
    assert_eq!(config.end_minute, 7 * 60);
    assert_eq!(config.transition_minutes, 30);

    config.temperature = 12_000;
    config.start_minute = -10;
    config.end_minute = 2_000;
    config.transition_minutes = 2_000;
    config.normalize();

    assert_eq!(config.temperature, MAX_TEMPERATURE);
    assert_eq!(config.start_minute, 0);
    assert_eq!(config.end_minute, 1_439);
    assert_eq!(config.transition_minutes, 1_440);
}

#[derive(Default)]
struct FakeBackend {
    writes: Vec<Rgb>,
    restores: usize,
    fail: bool,
}

impl Backend for FakeBackend {
    fn set_color(&mut self, rgb: Rgb) -> bool {
        self.writes.push(rgb);

        !self.fail
    }

    fn restore(&mut self) {
        self.restores += 1;
    }
}

fn always_on() -> Config {
    Config {
        enabled: true,
        mode: Mode::Always,
        ..Config::default()
    }
}

#[test]
fn a_scheduled_filter_outside_its_window_leaves_the_screen_alone() {
    let config = scheduled(19 * 60, 7 * 60, 30);
    let wake = wake_seconds(&config, 12.0 * 60.0).expect("the window opens later");
    let mut engine = Engine::new(config, FakeBackend::default());

    // Midday is neutral and the engine has never written, so it must not take
    // the gamma tables just to write an identity ramp.
    assert!(!engine.apply(12.0 * 60.0));
    assert!(engine.backend().writes.is_empty());
    assert!(wake > 0);
}

#[test]
fn an_always_on_filter_writes_once_and_then_only_on_a_change() {
    let mut engine = Engine::new(always_on(), FakeBackend::default());

    assert!(engine.apply(12.0 * 60.0));
    assert_eq!(engine.backend().writes.len(), 1);

    assert!(!engine.apply(12.0 * 60.0));
    assert_eq!(
        engine.backend().writes.len(),
        1,
        "the same colour is not another gamma table"
    );
    // Always-on cannot change with the clock, so nothing is scheduled.
    assert_eq!(engine.tick(12.0 * 60.0), None);
}

#[test]
fn a_fade_writes_each_step_but_not_the_same_minute_twice() {
    let mut engine = Engine::new(scheduled(19 * 60, 7 * 60, 30), FakeBackend::default());

    engine.apply(19.0 * 60.0 + 15.0);
    assert_eq!(engine.backend().writes.len(), 1);

    engine.apply(19.0 * 60.0 + 15.0);
    assert_eq!(engine.backend().writes.len(), 1);

    assert_eq!(engine.tick(19.0 * 60.0 + 20.0), Some(5));
    assert_eq!(engine.backend().writes.len(), 2);
}

#[test]
fn turning_the_filter_off_puts_the_neutral_colour_back_once() {
    let mut engine = Engine::new(always_on(), FakeBackend::default());

    engine.apply(12.0 * 60.0);
    assert_eq!(engine.backend().writes.len(), 1);

    let mut off = always_on();
    off.enabled = false;

    engine.set_config(off);

    assert!(engine.tick(12.0 * 60.0).is_none());
    assert_eq!(engine.backend().writes.len(), 2);
    assert!(engine.backend().writes[1].is_neutral());

    engine.apply(12.0 * 60.0);
    assert_eq!(
        engine.backend().writes.len(),
        2,
        "neutral is written once, not on every tick"
    );
}

#[test]
fn a_failed_write_is_not_recorded_and_is_retried() {
    let mut engine = Engine::new(always_on(), FakeBackend::default());

    engine.backend_mut().fail = true;
    assert!(!engine.apply(12.0 * 60.0));
    assert_eq!(engine.backend().writes.len(), 1);

    engine.backend_mut().fail = false;
    assert!(engine.apply(12.0 * 60.0));
    assert_eq!(engine.backend().writes.len(), 2);
}

#[test]
fn restore_releases_the_outputs_and_forgets_the_colour() {
    let mut engine = Engine::new(always_on(), FakeBackend::default());

    engine.apply(12.0 * 60.0);
    engine.restore();

    assert_eq!(engine.backend().restores, 1);

    // After a restore the engine is back to "has never written", so a neutral
    // tick (the filter turned off) does not take the outputs again.
    let mut off = always_on();
    off.enabled = false;
    engine.set_config(off);

    engine.apply(12.0 * 60.0);
    assert_eq!(engine.backend().writes.len(), 1);
}

#[test]
fn the_display_environment_picks_the_backend() {
    // A Wayland session means the compositor's gamma protocol; whelk does not
    // fall through to RandR in the same session when that fails.
    assert_eq!(choice(Some("wayland-1"), Some(":0")), Choice::Wayland);
    assert_eq!(choice(Some(""), Some(":0")), Choice::X11);
    assert_eq!(choice(None, Some(":0")), Choice::X11);
    // Empty variables are what a stripped environment has; both mean nothing
    // to warm.
    assert_eq!(choice(Some(""), Some("")), Choice::Unavailable);
    assert_eq!(choice(None, None), Choice::Unavailable);
}

#[test]
fn an_empty_schedule_never_needs_a_wake_up() {
    let mut config = scheduled(19 * 60, 7 * 60, 30);

    assert!(wake_seconds(&config, 12.0 * 60.0).is_some());

    config.enabled = false;
    assert_eq!(wake_seconds(&config, 12.0 * 60.0), None);

    config.enabled = true;
    config.mode = Mode::Always;
    assert_eq!(wake_seconds(&config, 12.0 * 60.0), None);

    // Start and end at the same minute describe an all-day window: the colour
    // cannot move with the clock.
    let all_day = scheduled(7 * 60, 7 * 60, 30);
    assert_eq!(wake_seconds(&all_day, 12.0 * 60.0), None);
}

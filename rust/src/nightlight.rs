// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! Night-light arithmetic, ported from `src/nightlight.c`.
//!
//! The policy knows nothing about Wayland or X11: it turns a temperature into
//! an RGB multiplier, a schedule into a strength, and a strength into a 16-bit
//! gamma ramp.  [`Engine`] adds C's controller on top -- apply only when the
//! colour changed, and only from neutral after whelk has touched the screen --
//! and the display backends carry the table to the server.  [`runtime`] owns
//! the clock, [`backend`] picks the Wayland or RandR implementation, and the
//! parts that can be tested without a monitor stay free of both.

mod backend;
mod runtime;
mod wayland;
mod x11;

pub use backend::{Choice, Platform, choice};
pub use runtime::{start, stop, update};

pub const MIN_TEMPERATURE: i32 = 1_000;
pub const MAX_TEMPERATURE: i32 = 6_500;
pub const NEUTRAL_TEMPERATURE: i32 = 6_500;
pub const MINUTES_PER_DAY: f64 = 1_440.0;
const LAST_MINUTE: i32 = 1_439;
const MAX_TRANSITION_MINUTES: i32 = 1_440;

/// How the filter decides when it is warm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Warm between the configured start and end minutes.
    #[default]
    Scheduled,
    /// Warm whenever the filter is enabled.
    Always,
}

/// The colour multipliers applied to the three display channels.
///
/// The values are always in `0.0..=1.0`; a ramp that raises a channel above
/// full scale would clip instead of merely warming the picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb([f64; 3]);

impl Rgb {
    pub const NEUTRAL: Self = Self([1.0; 3]);

    pub fn new(channels: [f64; 3]) -> Self {
        Self(channels.map(|channel| channel.clamp(0.0, 1.0)))
    }

    pub fn channels(self) -> [f64; 3] {
        self.0
    }

    pub fn red(self) -> f64 {
        self.0[0]
    }

    pub fn green(self) -> f64 {
        self.0[1]
    }

    pub fn blue(self) -> f64 {
        self.0[2]
    }

    /// Blend from neutral towards `other` by `amount`, clamped to `0..=1`.
    pub fn mix(self, other: Self, amount: f64) -> Self {
        self.with_other(other, amount.clamp(0.0, 1.0))
    }

    fn with_other(self, other: Self, amount: f64) -> Self {
        Self(std::array::from_fn(|index| {
            self.0[index] + amount * (other.0[index] - self.0[index])
        }))
    }

    /// Whether this colour differs from `other` by more than a gamma step
    /// anyone can see.  Used to decide a rewrite is not worth the table.
    pub fn is_close(self, other: Self, epsilon: f64) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .all(|(left, right)| (left - right).abs() < epsilon)
    }

    pub fn is_neutral(self) -> bool {
        self.0
            .into_iter()
            .all(|channel| (channel - 1.0).abs() < 0.000_001)
    }
}

/// One configuration, already normalized to the values the arithmetic accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub configured: bool,
    pub enabled: bool,
    pub mode: Mode,
    pub temperature: i32,
    pub start_minute: i32,
    pub end_minute: i32,
    pub transition_minutes: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            configured: false,
            enabled: false,
            mode: Mode::Scheduled,
            temperature: 3_400,
            start_minute: 19 * 60,
            end_minute: 7 * 60,
            transition_minutes: 30,
        }
    }
}

impl Config {
    /// Clamp every field to the same ranges as the C configuration.
    pub fn normalize(&mut self) {
        self.temperature = self.temperature.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);
        self.start_minute = self.start_minute.clamp(0, LAST_MINUTE);
        self.end_minute = self.end_minute.clamp(0, LAST_MINUTE);
        self.transition_minutes = self.transition_minutes.clamp(0, MAX_TRANSITION_MINUTES);
    }

    pub fn strength(&self, minutes_of_day: f64) -> f64 {
        strength(self, minutes_of_day)
    }

    pub fn next_tick_seconds(&self, minutes_of_day: f64) -> u32 {
        next_tick_seconds(self, minutes_of_day)
    }
}

/// A three-channel 16-bit gamma table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GammaRamp {
    size: usize,
    entries: Vec<u16>,
}

impl GammaRamp {
    /// Build the identity curve scaled by `rgb`.  A zero size is treated as a
    /// single full-scale entry, matching the C helper's division guard.
    pub fn new(size: usize, rgb: Rgb) -> Self {
        let size = size.max(1);
        let divisor = if size > 1 { (size - 1) as f64 } else { 1.0 };
        let channels = rgb.channels();
        let mut entries = vec![0; size * 3];

        for index in 0..size {
            let level = if size > 1 {
                index as f64 / divisor
            } else {
                1.0
            };

            for channel in 0..3 {
                entries[channel * size + index] =
                    (level * channels[channel] * f64::from(u16::MAX)).round() as u16;
            }
        }

        Self { size, entries }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn red(&self) -> &[u16] {
        &self.entries[..self.size]
    }

    pub fn green(&self) -> &[u16] {
        &self.entries[self.size..self.size * 2]
    }

    pub fn blue(&self) -> &[u16] {
        &self.entries[self.size * 2..]
    }

    pub fn channels(&self) -> [&[u16]; 3] {
        [self.red(), self.green(), self.blue()]
    }
}

/// The blackbody colour of `temperature`, normalized so 6500 K is white.
pub fn color(temperature: i32) -> Rgb {
    let reference = blackbody_raw(f64::from(NEUTRAL_TEMPERATURE));
    let colour = blackbody_raw(f64::from(
        temperature.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE),
    ));

    Rgb::new(std::array::from_fn(|channel| {
        let base = reference.0[channel];

        if base > 0.0 {
            colour.0[channel] / base
        } else {
            1.0
        }
    }))
}

/// Blend the neutral white point towards `temperature`'s colour.
pub fn mix(temperature: i32, strength: f64) -> Rgb {
    Rgb::NEUTRAL.mix(color(temperature), strength)
}

/// How warm the filter should be at a local time, in minutes from midnight.
pub fn strength(config: &Config, minutes_of_day: f64) -> f64 {
    if !config.enabled {
        return 0.0;
    }

    if config.mode == Mode::Always {
        return 1.0;
    }

    let start = normalize_minutes(f64::from(config.start_minute));
    let end = normalize_minutes(f64::from(config.end_minute));

    // Equal ends describe a window with no dark half.  Reading it as "all day"
    // is more useful than a schedule that can never be on.
    if start == end {
        return 1.0;
    }

    let window = normalize_minutes(end - start + MINUTES_PER_DAY);
    let elapsed = normalize_minutes(minutes_of_day - start);

    if elapsed >= window {
        return 0.0;
    }

    let transition = f64::from(config.transition_minutes.max(0)).min(window / 2.0);

    if transition <= 0.0 {
        return 1.0;
    }

    // Both ends fade: the start is dark and climbs, the end is bright and
    // falls.  When the transition fills the window the two meet in the middle
    // at exactly half strength.
    ((elapsed / transition).min((window - elapsed) / transition)).clamp(0.0, 1.0)
}

/// The colour difference below which another gamma table is wasted.  C's
/// `COLOR_EPSILON`; a fade step is larger than this, a rounding wobble is not.
pub const COLOR_EPSILON: f64 = 0.001;

/// Where the engine writes: the Wayland and RandR backends, or a test's fake.
pub trait Backend {
    /// Write one colour to every output.  False when none could be written, so
    /// the engine keeps the previous colour and tries again next tick.
    fn set_color(&mut self, rgb: Rgb) -> bool;

    /// Release the outputs, restoring the tables they had before whelk.
    fn restore(&mut self);
}

/// The clock and the colour, as C's `nightlight.c` has them: apply the
/// schedule, and write only when the resulting colour differs from the one on
/// screen.
pub struct Engine<B: Backend> {
    config: Config,
    backend: B,
    applied: Option<Rgb>,
}

impl<B: Backend> Engine<B> {
    pub fn new(config: Config, backend: B) -> Self {
        Self {
            config,
            backend,
            applied: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Apply what the schedule says at MINUTES and answer when the colour can
    /// next move, or `None` when the clock cannot move it.
    pub fn tick(&mut self, minutes: f64) -> Option<u32> {
        self.apply(minutes);
        wake_seconds(&self.config, minutes)
    }

    /// Apply the scheduled colour.  True when the backend was asked to write.
    pub fn apply(&mut self, minutes: f64) -> bool {
        let strength = if self.config.enabled {
            strength(&self.config, minutes)
        } else {
            0.0
        };
        let neutral = strength <= 0.0;
        let rgb = if neutral {
            Rgb::NEUTRAL
        } else {
            mix(self.config.temperature, strength)
        };

        // Nothing has been written yet: a neutral colour (midday, or a filter
        // that is off) must not take exclusive control of the outputs just to
        // write an identity table.  The gamma may be a calibrator's.
        if neutral && self.applied.is_none() {
            return false;
        }

        if self
            .applied
            .is_some_and(|previous| previous.is_close(rgb, COLOR_EPSILON))
        {
            return false;
        }

        if !self.backend.set_color(rgb) {
            return false;
        }

        self.applied = Some(rgb);

        true
    }

    /// Hand the outputs back.  The C backend releases its controls on exit so
    /// another night light or a calibrator can take them.
    pub fn restore(&mut self) {
        self.backend.restore();
        self.applied = None;
    }
}

/// Seconds until the next wake, or `None` when the policy cannot change with
/// the clock: off, always-on, or a window whose ends are the same minute.
pub fn wake_seconds(config: &Config, minutes_of_day: f64) -> Option<u32> {
    if !config.enabled || config.mode == Mode::Always || config.start_minute == config.end_minute {
        return None;
    }

    Some(next_tick_seconds(config, minutes_of_day))
}

/// How long until the strength can next change by a visible amount.
pub fn next_tick_seconds(config: &Config, minutes_of_day: f64) -> u32 {
    if !config.enabled || config.mode == Mode::Always {
        return 300;
    }

    let current_strength = strength(config, minutes_of_day);

    // A fade is the only time a small step is visible; five seconds is finer
    // than the eye can follow on a thirty-minute transition.
    if current_strength > 0.0 && current_strength < 1.0 {
        return 5;
    }

    let start = normalize_minutes(f64::from(config.start_minute));
    let end = normalize_minutes(f64::from(config.end_minute));

    if start == end {
        return 300;
    }

    let window = normalize_minutes(end - start + MINUTES_PER_DAY);
    let elapsed = normalize_minutes(minutes_of_day - start);
    let until_next = if elapsed < window {
        let transition = f64::from(config.transition_minutes.max(0)).min(window / 2.0);

        // Exactly at the opening edge the strength is still zero, but the fade
        // is about to move: a one-minute sleep there would make every schedule
        // start a minute late.
        if transition > 0.0 && (elapsed < transition || window - elapsed < transition) {
            return 5;
        }

        let fade_out = normalize_minutes(start + window - transition);

        normalize_minutes(fade_out - minutes_of_day)
    } else {
        normalize_minutes(start - minutes_of_day)
    };

    // A minute outside the window is a cheap wake-up and keeps the schedule
    // honest across a suspend or a timezone change; a boundary that is already
    // behind us waits for the next tick rather than spinning.
    (until_next * 60.0).ceil().clamp(5.0, 60.0) as u32
}

/// Parse `HH:MM` into minutes from midnight.
pub fn parse_clock(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();

    if bytes.len() != 5 || bytes[2] != b':' {
        return None;
    }

    if bytes[..2].iter().any(|byte| !byte.is_ascii_digit())
        || bytes[3..].iter().any(|byte| !byte.is_ascii_digit())
    {
        return None;
    }

    let hour = i32::from(bytes[0] - b'0') * 10 + i32::from(bytes[1] - b'0');
    let minute = i32::from(bytes[3] - b'0') * 10 + i32::from(bytes[4] - b'0');

    (hour <= 23 && minute <= 59).then_some(hour * 60 + minute)
}

fn normalize_minutes(minutes: f64) -> f64 {
    minutes.rem_euclid(MINUTES_PER_DAY)
}

fn blackbody_raw(kelvin: f64) -> Rgb {
    let temperature = kelvin / 100.0;

    let (red, green, blue) = if temperature <= 66.0 {
        let red = 255.0;
        let green = 99.470_802_586_1 * temperature.ln() - 161.119_568_166_1;
        let blue = if temperature <= 19.0 {
            0.0
        } else {
            138.517_731_223_1 * (temperature - 10.0).ln() - 305.044_792_730_7
        };

        (red, green, blue)
    } else {
        (
            329.698_727_446 * (temperature - 60.0).powf(-0.133_204_759_2),
            288.122_169_528_3 * (temperature - 60.0).powf(-0.075_514_849_2),
            255.0,
        )
    };

    Rgb::new([red / 255.0, green / 255.0, blue / 255.0])
}

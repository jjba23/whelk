// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The wallpaper policy, as data.
//!
//! This is the first module moved out of `src/util/wallpaper.c`.  It has no
//! GTK, display server, or Guile dependency: a path, a transition and a
//! rectangle are enough.  Keeping it here means the migration can prove parity
//! before a single pixel is drawn by Rust.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Unknown,
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff", "svg",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "webm", "mkv", "mov", "m4v", "ogv", "avi", "mpg", "mpeg",
];

pub fn kind_for_path(path: impl AsRef<Path>) -> Kind {
    let Some(extension) = path.as_ref().extension().and_then(|value| value.to_str()) else {
        return Kind::Unknown;
    };

    if IMAGE_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        Kind::Image
    } else if VIDEO_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        Kind::Video
    } else {
        Kind::Unknown
    }
}

pub fn is_supported(path: impl AsRef<Path>) -> bool {
    kind_for_path(path) != Kind::Unknown
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Animation {
    None,
    #[default]
    Crossfade,
    Fade,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Zoom,
}

impl Animation {
    pub const ALL: [Animation; 8] = [
        Animation::None,
        Animation::Crossfade,
        Animation::Fade,
        Animation::SlideLeft,
        Animation::SlideRight,
        Animation::SlideUp,
        Animation::SlideDown,
        Animation::Zoom,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|animation| animation.name().eq_ignore_ascii_case(text))
    }

    pub fn name(self) -> &'static str {
        match self {
            Animation::None => "none",
            Animation::Crossfade => "crossfade",
            Animation::Fade => "fade",
            Animation::SlideLeft => "slide-left",
            Animation::SlideRight => "slide-right",
            Animation::SlideUp => "slide-up",
            Animation::SlideDown => "slide-down",
            Animation::Zoom => "zoom",
        }
    }

    pub fn alpha_in(self, progress: f64) -> f64 {
        let t = progress.clamp(0.0, 1.0);

        match self {
            Animation::None
            | Animation::SlideLeft
            | Animation::SlideRight
            | Animation::SlideUp
            | Animation::SlideDown => 1.0,
            Animation::Fade => {
                if t < 0.5 {
                    0.0
                } else {
                    (t - 0.5) * 2.0
                }
            }
            Animation::Crossfade | Animation::Zoom => t,
        }
    }

    pub fn alpha_out(self, progress: f64) -> f64 {
        let t = progress.clamp(0.0, 1.0);

        match self {
            Animation::Fade => {
                if t < 0.5 {
                    1.0 - t * 2.0
                } else {
                    0.0
                }
            }
            Animation::Crossfade
            | Animation::Zoom
            | Animation::SlideLeft
            | Animation::SlideRight
            | Animation::SlideUp
            | Animation::SlideDown => 1.0,
            Animation::None => 0.0,
        }
    }

    pub fn zoom_in(self, progress: f64) -> f64 {
        if self != Animation::Zoom {
            return 1.0;
        }

        1.0 + 0.12 * (1.0 - progress.clamp(0.0, 1.0))
    }

    pub fn offsets(self, progress: f64, width: f64, height: f64) -> Offsets {
        let t = progress.clamp(0.0, 1.0);
        let mut offsets = Offsets {
            out_x: 0.0,
            out_y: 0.0,
            in_x: 0.0,
            in_y: 0.0,
        };

        match self {
            Animation::SlideLeft => {
                offsets.out_x = -width * t;
                offsets.in_x = width * (1.0 - t);
            }
            Animation::SlideRight => {
                offsets.out_x = width * t;
                offsets.in_x = -width * (1.0 - t);
            }
            Animation::SlideUp => {
                offsets.out_y = -height * t;
                offsets.in_y = height * (1.0 - t);
            }
            Animation::SlideDown => {
                offsets.out_y = height * t;
                offsets.in_y = -height * (1.0 - t);
            }
            _ => {}
        }

        offsets
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Offsets {
    pub out_x: f64,
    pub out_y: f64,
    pub in_x: f64,
    pub in_y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Fill,
}

impl Fit {
    pub fn parse(text: &str) -> Option<Self> {
        [Fit::Cover, Fit::Contain, Fit::Fill]
            .into_iter()
            .find(|fit| fit.name().eq_ignore_ascii_case(text))
    }

    pub fn name(self) -> &'static str {
        match self {
            Fit::Cover => "cover",
            Fit::Contain => "contain",
            Fit::Fill => "fill",
        }
    }

    pub fn geometry(
        self,
        view_width: f64,
        view_height: f64,
        image_width: f64,
        image_height: f64,
        zoom: f64,
    ) -> Geometry {
        if view_width <= 0.0 || view_height <= 0.0 || image_width <= 0.0 || image_height <= 0.0 {
            return Geometry {
                scale_x: 1.0,
                scale_y: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
            };
        }

        let zoom = if zoom > 0.0 { zoom } else { 1.0 };

        let (scale_x, scale_y) = match self {
            Fit::Contain => {
                let scale = (view_width / image_width).min(view_height / image_height) * zoom;
                (scale, scale)
            }
            Fit::Fill => (
                view_width / image_width * zoom,
                view_height / image_height * zoom,
            ),
            Fit::Cover => {
                let scale = (view_width / image_width).max(view_height / image_height) * zoom;
                (scale, scale)
            }
        };

        Geometry {
            scale_x,
            scale_y,
            offset_x: (view_width - image_width * scale_x) / 2.0,
            offset_y: (view_height - image_height * scale_y) / 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub scale_x: f64,
    pub scale_y: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// Smoothstep: 0 at 0, 1 at 1, and no visible kick at either end.
pub fn ease(progress: f64) -> f64 {
    let t = progress.clamp(0.0, 1.0);

    t * t * (3.0 - 2.0 * t)
}

/// The next item in a collection.
///
/// Sequential order wraps around.  Shuffled order advances a caller-owned
/// xorshift state and never repeats the current index, matching the C engine's
/// reproducible rotation.
pub fn next_index(count: usize, current: usize, shuffle: bool, state: &mut u64) -> usize {
    if count <= 1 {
        return 0;
    }

    if !shuffle {
        return (current + 1) % count;
    }

    let mut x = if *state == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        *state
    };

    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;

    let candidate = ((x.wrapping_mul(0x2545_f491_4f6c_dd1d)) >> 33) as usize % count;

    if candidate == current {
        (candidate + 1) % count
    } else {
        candidate
    }
}

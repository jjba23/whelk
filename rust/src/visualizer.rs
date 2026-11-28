// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The live half of the visualizer card: one `cava` process per card.
//!
//! cava does the audio decoding, the FFT and the smoothing, so whelk does not
//! have to link an audio stack or reinvent a spectrum analyser.  The card asks
//! for raw ASCII on stdout and reads one line per frame; the frame parser and
//! the private cava configuration are small and live here, beside the process
//! they exist for.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

static NEXT_CONFIG: AtomicU64 = AtomicU64::new(1);

type FrameCallback = dyn Fn(Vec<f64>) + 'static;
type EndCallback = dyn Fn() + 'static;

/// A running cava and the private configuration that started it.
///
/// The card keeps this alive and calls [`stop`](Self::stop) from its destroy
/// handler.  Dropping it also stops cava and removes the configuration, so a
/// card that is closed cannot leave a process reading the speakers forever.
pub struct Cava {
    process: gio::Subprocess,
    config: PathBuf,
    bars: usize,
}

impl Cava {
    /// Start `cava` with `bars` columns at `framerate` frames per second.
    ///
    /// `on_frame` is handed each parsed frame, and `on_end` once when cava
    /// exits.  A missing cava is a card that says so rather than a shell that
    /// fails to start, so the error is returned to the caller to paint.
    pub fn start<F, E>(
        bars: usize,
        framerate: u32,
        on_frame: F,
        on_end: E,
    ) -> Result<Rc<Self>, String>
    where
        F: Fn(Vec<f64>) + 'static,
        E: Fn() + 'static,
    {
        let config = config_path();

        if let Some(parent) = config.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("Could not create cava's configuration directory: {error}")
            })?;
        }

        std::fs::write(&config, visualizer_config(bars, framerate))
            .map_err(|error| format!("Could not write cava's configuration: {error}"))?;

        let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDOUT_PIPE);
        let process =
            match launcher.spawn(&[OsStr::new("cava"), OsStr::new("-p"), config.as_os_str()]) {
                Ok(process) => process,
                Err(error) => {
                    let _ = std::fs::remove_file(&config);

                    return Err(format!("Could not start cava: {error}"));
                }
            };

        let Some(stdout) = process.stdout_pipe() else {
            process.force_exit();
            let _ = std::fs::remove_file(&config);

            return Err("cava did not open its output pipe".to_owned());
        };

        let bars = bars.clamp(1, VISUALIZER_MAX_BARS);
        let cava = Rc::new(Self {
            process,
            config,
            bars,
        });
        let stream = gio::DataInputStream::new(&stdout);

        stream.set_newline_type(gio::DataStreamNewlineType::Lf);
        read_next(stream, Rc::clone(&cava), Rc::new(on_frame));

        let finished = Rc::clone(&cava);
        let on_end: Rc<EndCallback> = Rc::new(on_end);

        finished
            .process
            .wait_check_async(gio::Cancellable::NONE, move |_result| on_end());

        Ok(cava)
    }

    /// Stop cava now.  Safe to call more than once; the destroy handler and the
    /// final drop both reach it.
    pub fn stop(&self) {
        self.process.force_exit();
        let _ = std::fs::remove_file(&self.config);
    }
}

impl Drop for Cava {
    fn drop(&mut self) {
        self.process.force_exit();
        let _ = std::fs::remove_file(&self.config);
    }
}

fn read_next(stream: gio::DataInputStream, cava: Rc<Cava>, on_frame: Rc<FrameCallback>) {
    let next = stream.clone();

    stream.read_line_async(
        glib::Priority::DEFAULT_IDLE,
        gio::Cancellable::NONE,
        move |result| {
            let Ok(Some(line)) = result else {
                return;
            };
            let frame = parse_visualizer_frame(&String::from_utf8_lossy(line.as_ref()), cava.bars);

            on_frame(frame);
            read_next(next, cava, on_frame);
        },
    );
}

fn config_path() -> PathBuf {
    let serial = NEXT_CONFIG.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("whelk");

    directory.join(format!("visualizer-{}-{serial}.conf", std::process::id()))
}

/// The widest visualizer a card may ask for.
pub const VISUALIZER_MAX_BARS: usize = 64;

/// One cava line as levels in `0.0..=1.0`.  An unreadable field is silence and
/// a short frame is simply shorter; the card pads it.
fn parse_visualizer_frame(line: &str, bars: usize) -> Vec<f64> {
    line.split([';', ',', ' ', '\t'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .map(|value| (value / 100.0).clamp(0.0, 1.0))
        .take(bars.min(VISUALIZER_MAX_BARS))
        .collect()
}

/// The cava configuration the visualizer owns while its card is alive.
///
/// It writes raw ASCII to stdout, which is the one mode a subprocess can read
/// without an audio server on the shell's side or a second decoder.  The card
/// never lets cava open its own terminal, and the `[input]` section is cava's
/// own auto-detection, so PulseAudio, PipeWire and ALSA all work as they do for
/// every other cava user.
fn visualizer_config(bars: usize, framerate: u32) -> String {
    let bars = bars.clamp(1, VISUALIZER_MAX_BARS);
    let framerate = framerate.clamp(1, 120);

    format!(
        "[general]\nbars = {bars}\nframerate = {framerate}\n\n\
         [input]\nmethod = pulse\nsource = auto\n\n\
         [output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = ascii\n\
         ascii_max_range = 100\nbar_delimiter = 59\nframe_delimiter = 10\n"
    )
}

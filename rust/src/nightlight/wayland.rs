// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The `wlr-gamma-control-unstable-v1` night light backend.
//!
//! The protocol's control object is exclusive: taking one just to write an
//! identity table would steal an output from a calibrator or another night
//! light.  So a control is only created once the controller asks for a warm
//! colour, and a neutral colour releases every control, which lets the
//! compositor restore each output's own table.
//!
//! The connection is separate from GTK's: gamma control is a privileged,
//! global protocol and needs its own registry.  Its socket is watched with a
//! GLib source, so gamma sizes and hotplugged outputs arrive without a
//! roundtrip on every fade step.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Seek, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk4 as gtk;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::zwlr_gamma_control_manager_v1::{
    self, ZwlrGammaControlManagerV1,
};
use wayland_protocols_wlr::gamma_control::v1::client::zwlr_gamma_control_v1::{
    self, ZwlrGammaControlV1,
};

use crate::nightlight::{Backend, GammaRamp, Rgb};
use crate::sources;

/// A refused output is usually another client holding it.  Retrying on every
/// fade step would be a request storm; thirty seconds is enough for the other
/// client to go away.
const RETRY: Duration = Duration::from_secs(30);

struct Output {
    name: u32,
    _output: wl_output::WlOutput,
    control: Option<ZwlrGammaControlV1>,
    size: u32,
    retry_at: Option<Instant>,
    warned: bool,
}

struct State {
    /// Kept alive for the connection's lifetime; dropping it would destroy
    /// the registry proxy and stop output announcements.
    _registry: wl_registry::WlRegistry,
    manager: Option<ZwlrGammaControlManagerV1>,
    manager_name: u32,
    outputs: Vec<Output>,
    /// The warm colour the controller asked for; `None` means neutral and the
    /// controls are released.
    wants: Option<Rgb>,
    qh: QueueHandle<State>,
}

impl State {
    fn output_index(&self, name: u32) -> Option<usize> {
        self.outputs.iter().position(|output| output.name == name)
    }

    /// Create the control for one output if there is a manager and the failed
    /// backoff has passed.  True when a control was created, so the caller
    /// knows a roundtrip is needed to learn its gamma size.
    fn ensure_control(&mut self, name: u32) -> bool {
        let Some(manager) = self.manager.clone() else {
            return false;
        };
        let Some(index) = self.output_index(name) else {
            return false;
        };
        let output = &mut self.outputs[index];

        if output.control.is_some() {
            return false;
        }

        if output
            .retry_at
            .is_some_and(|retry_at| Instant::now() < retry_at)
        {
            return false;
        }

        output.retry_at = None;
        output.size = 0;
        output.control = Some(manager.get_gamma_control(&output._output, &self.qh, name));

        true
    }

    fn write_output(&mut self, name: u32) -> bool {
        let Some(rgb) = self.wants else {
            return false;
        };
        let Some(index) = self.output_index(name) else {
            return false;
        };
        let output = &self.outputs[index];

        if output.size == 0 {
            return false;
        }
        let Some(control) = &output.control else {
            return false;
        };
        let Some(fd) = gamma_table(GammaRamp::new(output.size as usize, rgb)) else {
            return false;
        };

        control.set_gamma(fd.as_fd());

        true
    }

    fn write_all(&mut self) -> bool {
        let names: Vec<u32> = self.outputs.iter().map(|output| output.name).collect();
        let mut wrote = false;

        for name in names {
            wrote |= self.write_output(name);
        }

        wrote
    }

    /// Give every output back to the compositor.
    fn release(&mut self) {
        for output in &mut self.outputs {
            if let Some(control) = output.control.take() {
                control.destroy();
            }

            output.size = 0;
            output.retry_at = None;
            output.warned = false;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == "zwlr_gamma_control_manager_v1" && state.manager.is_none() {
                    let manager: ZwlrGammaControlManagerV1 =
                        registry.bind(name, version.min(1), qh, ());

                    state.manager = Some(manager);
                    state.manager_name = name;
                } else if interface == "wl_output" {
                    let output: wl_output::WlOutput = registry.bind(name, version.min(2), qh, name);

                    state.outputs.push(Output {
                        name,
                        _output: output,
                        control: None,
                        size: 0,
                        retry_at: None,
                        warned: false,
                    });
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = state.output_index(name) {
                    let output = state.outputs.remove(index);

                    if let Some(control) = output.control {
                        control.destroy();
                    }
                }

                if state.manager.is_some() && state.manager_name == name {
                    state.manager = None;
                    state.manager_name = 0;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // A mode change can reset the CRTC's gamma table and the compositor
        // may announce nothing else.
        if matches!(event, wl_output::Event::Mode { .. }) {
            state.write_output(*data);
        }
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrGammaControlManagerV1,
        _event: zwlr_gamma_control_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, u32> for State {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        data: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                if let Some(index) = state.output_index(*data) {
                    let output = &mut state.outputs[index];

                    output.size = size;
                    output.warned = false;
                }

                state.write_output(*data);
            }
            zwlr_gamma_control_v1::Event::Failed => {
                let Some(index) = state.output_index(*data) else {
                    return;
                };
                let output = &mut state.outputs[index];

                if !output.warned {
                    eprintln!(
                        "whelk: night light: the compositor refused gamma control for one \
                         output; another client may hold it, or the output has no gamma table"
                    );
                    output.warned = true;
                }

                output.size = 0;
                output.retry_at = Some(Instant::now() + RETRY);

                if let Some(control) = output.control.take() {
                    control.destroy();
                }
            }
            _ => {}
        }
    }
}

/// The backend's own Wayland connection, its queue and the state both share.
pub struct WaylandBackend {
    connection: Connection,
    queue: Rc<RefCell<EventQueue<State>>>,
    state: Rc<RefCell<State>>,
    source: Option<glib::SourceId>,
}

impl WaylandBackend {
    pub fn new() -> Option<Self> {
        let connection = Connection::connect_to_env().ok()?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let registry = connection.display().get_registry(&qh, ());
        let mut state = State {
            _registry: registry,
            manager: None,
            manager_name: 0,
            outputs: Vec::new(),
            wants: None,
            qh,
        };

        queue.roundtrip(&mut state).ok()?;

        state.manager.as_ref()?;

        let queue = Rc::new(RefCell::new(queue));
        let state = Rc::new(RefCell::new(state));
        let fd = connection.backend().poll_fd().as_raw_fd();
        let watched_connection = connection.clone();
        let watched_queue = Rc::clone(&queue);
        let watched_state = Rc::clone(&state);
        // glib-rs does not bind g_unix_fd_source_new, so the socket is polled
        // with a zero timeout once a second.  A gamma size or a hotplugged
        // output does not need sub-second latency.
        let source = glib::timeout_add_seconds_local(1, move || {
            let mut poll_fd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut poll_fd, 1, 0) } > 0;

            if ready && let Some(guard) = watched_connection.prepare_read() {
                let _ = guard.read();
            }

            let _ = watched_queue
                .borrow_mut()
                .dispatch_pending(&mut watched_state.borrow_mut());

            glib::ControlFlow::Continue
        });

        Some(Self {
            connection,
            queue,
            state,
            source: Some(source),
        })
    }
}

impl Backend for WaylandBackend {
    fn set_color(&mut self, rgb: Rgb) -> bool {
        let mut state = self.state.borrow_mut();

        if rgb.is_neutral() {
            state.wants = None;
            state.release();
        } else {
            state.wants = Some(rgb);

            let names: Vec<u32> = state.outputs.iter().map(|output| output.name).collect();
            let mut created = false;

            for name in names {
                created |= state.ensure_control(name);
            }

            if created {
                // Gamma sizes arrive as events; one roundtrip here is cheaper
                // than a fade step that cannot be written.
                let mut queue = self.queue.borrow_mut();

                let _ = queue.roundtrip(&mut state);
            }

            if !state.write_all() {
                // Nothing was written: no output has a usable control yet, or
                // they are all in their retry window.  Answering false is what
                // keeps the engine trying instead of believing the screen is
                // warm.
                drop(state);
                let _ = self.connection.flush();

                return false;
            }
        }

        drop(state);

        if self.connection.flush().is_err() {
            eprintln!("whelk: night light: could not flush a gamma update");
        }

        true
    }

    fn restore(&mut self) {
        self.state.borrow_mut().release();
        let _ = self.connection.flush();
    }
}

impl Drop for WaylandBackend {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            sources::remove(source);
        }

        self.state.borrow_mut().release();
        let _ = self.connection.flush();
    }
}

/// Write RAMP into a descriptor and leave it at offset zero.
///
/// The compositor reads the table from the descriptor it is handed, and not
/// every compositor maps it from the start: niri reads from the current file
/// offset and answers `failed` when that offset is the end of a table that was
/// just written, while wlroots maps the descriptor from zero and never noticed.
/// Rewinding is what makes the same table work on both.
fn gamma_table(ramp: GammaRamp) -> Option<OwnedFd> {
    let fd = gamma_fd(ramp.size() * 6)?;
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::with_capacity(ramp.size() * 6);

    for channel in [ramp.red(), ramp.green(), ramp.blue()] {
        for value in channel {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }

    file.write_all(&bytes).ok()?;
    file.rewind().ok()?;

    Some(file.into())
}

/// A memfd with `bytes` of space, or an unlinked temporary file where a
/// hardened runtime refuses memfd_create.
fn gamma_fd(bytes: usize) -> Option<OwnedFd> {
    let fd = unsafe { libc::memfd_create(c"whelk-gamma".as_ptr(), libc::MFD_CLOEXEC) };

    if fd >= 0 {
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let sized = unsafe { libc::ftruncate(fd.as_raw_fd(), bytes as libc::off_t) };

        if sized == 0 {
            return Some(fd);
        }

        return None;
    }

    let mut hasher = DefaultHasher::new();

    std::process::id().hash(&mut hasher);
    Instant::now().hash(&mut hasher);

    let path = std::env::temp_dir().join(format!("whelk-gamma-{:016x}", hasher.finish()));
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .ok()?;

    let _ = std::fs::remove_file(&path);
    file.set_len(bytes as u64).ok()?;

    Some(OwnedFd::from(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn the_gamma_table_reads_back_from_its_start() {
        // A compositor that reads the descriptor instead of mapping it sees
        // whatever the offset points at: the table must start at zero, or niri
        // answers `failed` and the screen never warms.
        let ramp = GammaRamp::new(4, Rgb::new([1.0, 0.5, 0.25]));
        let mut file = std::fs::File::from(gamma_table(ramp.clone()).expect("a table"));
        let mut read = Vec::new();

        file.read_to_end(&mut read).expect("read back");

        let expected: Vec<u8> = ramp
            .channels()
            .iter()
            .flat_map(|channel| channel.iter().flat_map(|value| value.to_ne_bytes()))
            .collect();

        assert_eq!(read.len(), ramp.size() * 6);
        assert_eq!(read, expected);
    }
}

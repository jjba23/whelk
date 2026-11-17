// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! River's status pushes, ported from `util/riverstatus.c`.
//!
//! `riverctl` has no query for the focused view.  River publishes it on the
//! `river-status` protocol instead: one `zriver_seat_status_v1` per seat, whose
//! `focused_view` event carries the title and fires again whenever the view or
//! one of its properties changes.  A title is what the window bar needs; an
//! empty title means no view is focused, the same "nothing to show" as a
//! session with no IPC.
//!
//! The connection is the client's own, like the C module's: binding status
//! globals for the shell must not depend on GTK's display connection or on
//! another module's registry.  `glib-rs` has no `g_unix_fd_source_new`, so the
//! socket is polled with a zero timeout and dispatched on GTK's main loop.  A
//! focused-view title has no sub-second deadline, and the poll only runs on a
//! River session.

use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk4 as gtk;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};

mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("../protocol/river-status-unstable-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("../protocol/river-status-unstable-v1.xml");
}

use crate::sources;
use protocol::zriver_seat_status_v1::{self, ZriverSeatStatusV1};
use protocol::zriver_status_manager_v1::{self, ZriverStatusManagerV1};

/// How often the protocol socket is polled while a River session is focused.
/// The alternative is one read per focus change; this is one cheap syscall.
const POLL: Duration = Duration::from_millis(100);

type ViewCallback = Box<dyn FnMut(&str)>;

struct Seat {
    name: u32,
    seat: wl_seat::WlSeat,
    status: Option<ZriverSeatStatusV1>,
}

struct State {
    /// Kept alive for the connection's lifetime; dropping it would stop the
    /// seat and manager announcements.
    _registry: wl_registry::WlRegistry,
    manager: Option<ZriverStatusManagerV1>,
    manager_name: u32,
    seats: Vec<Seat>,
    callback: ViewCallback,
}

impl State {
    fn seat_index(&self, name: u32) -> Option<usize> {
        self.seats.iter().position(|seat| seat.name == name)
    }

    fn bind_status(&mut self, index: usize, qh: &QueueHandle<Self>) {
        if self.seats[index].status.is_some() {
            return;
        }

        let Some(manager) = self.manager.clone() else {
            return;
        };
        let status = manager.get_river_seat_status(&self.seats[index].seat, qh, ());

        self.seats[index].status = Some(status);
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
                if interface == "zriver_status_manager_v1" && state.manager.is_none() {
                    let manager: ZriverStatusManagerV1 =
                        registry.bind(name, version.min(3), qh, ());

                    state.manager = Some(manager);
                    state.manager_name = name;

                    // Seats may have arrived before the manager; give each one
                    // the status object it could not have yet.
                    for index in 0..state.seats.len() {
                        state.bind_status(index, qh);
                    }
                } else if interface == "wl_seat" {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(1), qh, ());

                    state.seats.push(Seat {
                        name,
                        seat,
                        status: None,
                    });

                    let index = state.seats.len() - 1;

                    state.bind_status(index, qh);
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = state.seat_index(name) {
                    let seat = state.seats.remove(index);

                    if let Some(status) = seat.status {
                        status.destroy();
                    }
                }

                if state.manager_name == name {
                    state.manager = None;
                    state.manager_name = 0;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _seat: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZriverStatusManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _manager: &ZriverStatusManagerV1,
        _event: zriver_status_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZriverSeatStatusV1, ()> for State {
    fn event(
        state: &mut Self,
        _status: &ZriverSeatStatusV1,
        event: zriver_seat_status_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zriver_seat_status_v1::Event::FocusedView { title } = event {
            (state.callback)(&title);
        }
    }
}

/// A live connection to River's status manager.  Dropping it destroys the
/// per-seat status objects and stops the socket watch, which is what the C
/// module's destroy callback does.
pub struct Watch {
    connection: Connection,
    queue: Rc<RefCell<EventQueue<State>>>,
    state: Rc<RefCell<State>>,
    source: Option<glib::SourceId>,
}

impl Watch {
    /// Subscribe to the focused view.  `None` when this is not a Wayland
    /// session, the compositor has no `river-status` manager, or the socket
    /// cannot be watched -- the bar then stays empty rather than guessing.
    pub fn start<F>(callback: F) -> Option<Self>
    where
        F: FnMut(&str) + 'static,
    {
        let connection = Connection::connect_to_env().ok()?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let registry = connection.display().get_registry(&qh, ());
        let mut state = State {
            _registry: registry,
            manager: None,
            manager_name: 0,
            seats: Vec::new(),
            callback: Box::new(callback),
        };

        // The manager and the seats are announced during this roundtrip; a
        // compositor without the manager is not River.
        queue.roundtrip(&mut state).ok()?;
        state.manager.as_ref()?;

        let queue = Rc::new(RefCell::new(queue));
        let state = Rc::new(RefCell::new(state));
        let fd = connection.backend().poll_fd().as_raw_fd();
        let watched_connection = connection.clone();
        let watched_queue = Rc::clone(&queue);
        let watched_state = Rc::clone(&state);
        let source = glib::timeout_add_local(POLL, move || {
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
            let _ = watched_connection.flush();

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

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            sources::remove(source);
        }

        {
            let mut state = self.state.borrow_mut();

            for seat in &mut state.seats {
                if let Some(status) = seat.status.take() {
                    status.destroy();
                }
            }

            if let Some(manager) = state.manager.take() {
                manager.destroy();
            }
        }

        let _ = self.connection.flush();
        // The queue stays alive until here so the destroy requests above are
        // the last thing on the connection.
        let _ = &self.queue;
    }
}

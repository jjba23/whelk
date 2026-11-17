// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The RandR night light backend.
//!
//! On an X server the gamma table belongs to a CRTC.  Unlike the Wayland
//! protocol, RandR has no "release" request that restores what was there
//! before, so the table each CRTC held when whelk first wrote it is captured
//! and put back when the filter turns neutral or the shell exits.

use std::collections::HashMap;

use x11rb::connection::Connection as _;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;

use crate::nightlight::{Backend, GammaRamp, Rgb};

struct Original {
    size: u16,
    red: Vec<u16>,
    green: Vec<u16>,
    blue: Vec<u16>,
}

pub struct X11Backend {
    connection: RustConnection,
    root: Window,
    originals: HashMap<randr::Crtc, Original>,
}

impl X11Backend {
    pub fn new() -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;

        // A server below RandR 1.2 has no per-CRTC gamma at all.
        connection.randr_query_version(1, 2).ok()?.reply().ok()?;

        Some(Self {
            connection,
            root,
            originals: HashMap::new(),
        })
    }

    fn resources(&self) -> Option<randr::GetScreenResourcesCurrentReply> {
        self.connection
            .randr_get_screen_resources_current(self.root)
            .ok()?
            .reply()
            .ok()
    }

    fn capture(&mut self, crtc: randr::Crtc, size: u16) -> bool {
        let Some(reply) = self
            .connection
            .randr_get_crtc_gamma(crtc)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return false;
        };

        if reply.size() != size {
            return false;
        }

        self.originals.insert(
            crtc,
            Original {
                size,
                red: reply.red,
                green: reply.green,
                blue: reply.blue,
            },
        );

        true
    }
}

impl Backend for X11Backend {
    fn set_color(&mut self, rgb: Rgb) -> bool {
        let Some(resources) = self.resources() else {
            return false;
        };

        for crtc in resources.crtcs {
            let size = match self
                .connection
                .randr_get_crtc_gamma_size(crtc)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            {
                Some(reply) if reply.size > 0 => reply.size,
                // A CRTC without a gamma table is a real answer (some virtual
                // and remote outputs have none), not an error.
                _ => continue,
            };

            if self
                .originals
                .get(&crtc)
                .is_some_and(|original| original.size != size)
            {
                self.originals.remove(&crtc);
            }

            if !self.originals.contains_key(&crtc) {
                self.capture(crtc, size);
            }

            if let Some(original) = self.originals.get(&crtc)
                && rgb.is_neutral()
            {
                let _ = self.connection.randr_set_crtc_gamma(
                    crtc,
                    &original.red,
                    &original.green,
                    &original.blue,
                );

                continue;
            }

            let ramp = GammaRamp::new(size as usize, rgb);

            let _ =
                self.connection
                    .randr_set_crtc_gamma(crtc, ramp.red(), ramp.green(), ramp.blue());
        }

        let _ = self.connection.flush();

        true
    }

    fn restore(&mut self) {
        let Some(resources) = self.resources() else {
            return;
        };

        for crtc in resources.crtcs {
            let Some(original) = self.originals.get(&crtc) else {
                continue;
            };

            let _ = self.connection.randr_set_crtc_gamma(
                crtc,
                &original.red,
                &original.green,
                &original.blue,
            );
        }

        let _ = self.connection.flush();
    }
}

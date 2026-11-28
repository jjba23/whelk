;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(use-modules (guix packages)
             (gnu)
             (gnu packages)
             (gnu packages guile)
             (gnu packages admin)
             (gnu packages build-tools)
             (gnu packages ninja)
             (gnu packages compression)
             (gnu packages pkg-config)
             (gnu packages rust)
             (gnu packages gdb)
             (gnu packages linux)
             (gnu packages gtk)
             (gnu packages glib)
             (gnu packages gnome)
             (gnu packages freedesktop)
             (gnu packages xorg)
             (gnu packages base)
             (gnu packages commencement))

(packages->manifest (list guile-3.0
                          ;; The separate outputs each carry one piece: `out'
                          ;; has rustc, `cargo' the build tool, and `tools'
                          ;; rustfmt and clippy.  Selecting one is not enough.
                          (list rust "out")
                          (list rust "cargo")
                          (list rust "tools")
                          ;; rust-analyzer reads the standard library's
                          ;; sources from the sysroot; without this it shows
                          ;; inferred signatures and no documentation.
                          (list rust "rust-src")
                          gcc-toolchain
                          pkg-config
                          wayland ; wayland-scanner, for the river-status protocol
                          gtk
                          gtk4-layer-shell
                          ;; The notification and AT-SPI tests spawn a
                          ;; private dbus-daemon.  Without it those tests skip.
                          dbus
                          ;; whelk types menu shortcuts with wtype on Wayland;
                          ;; the dev shell should have it just like the package.
                          wtype
                          ;; The Rust shell still resolves GTK's data files
                          ;; through glib's "bin" output at development time.
                          (list glib "bin")
                          ;; `libtiff-4.pc' lists liblzma in its
                          ;; Requires.private, and pkg-config resolves those even
                          ;; for `--cflags'.
                          xz
                          ;; An icon theme makes the tray's fallback icon and
                          ;; GTK's own symbols resolvable on a bare machine.
                          adwaita-icon-theme
                          ;; The shell's own scripts use dirname/mktemp; do
                          ;; not depend on the host PATH.
                          coreutils
                          ;; `maak debug-run' launches whelk under gdb, so the
                          ;; debugger comes from the pinned shell too.
                          gdb
                          ;; Reading these is how the shell is debugged without
                          ;; a compositor (see docs/installation.org).
                          procps
                          util-linux
                          ;; The Scheme layer's API reference under `doc/api'
                          ;; is extracted by dokgen; maak's `docs' task renders
                          ;; it.  The Rust crates are documented by rustdoc.
                          (specification->package "guile-dokgen")
                          (specification->package "texinfo")))

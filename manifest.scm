;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(use-modules (guix packages)
             (gnu)
             (gnu packages guile)
             (gnu packages admin)
             (gnu packages build-tools)
             (gnu packages ninja)
             (gnu packages compression)
             (gnu packages pkg-config)
             (gnu packages linux)
             (gnu packages gtk)
             (gnu packages glib)
             (gnu packages gnome)
             (gnu packages freedesktop)
             (gnu packages xorg)
             (gnu packages base)
             (gnu packages commencement))

(packages->manifest (list guile-3.0
                          (list rust "cargo")
                          gcc-toolchain
                          pkg-config
                          wayland ; wayland-scanner, for the river-status protocol
                          gtk
                          gtk4-layer-shell
                          ;; The notification and AT-SPI tests spawn a
                          ;; private dbus-daemon.  Without it those tests skip.
                          dbus
                          ;; The locker's headers are a build requirement: a
                          ;; build without them could only ship a lock screen
                          ;; that never authenticates.  The library itself is
                          ;; opened with dlopen at run time, so linux-pam is a
                          ;; build-time input here, not a linked one.
                          linux-pam
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
                          ;; Reading these is how the shell is debugged without
                          ;; a compositor (see docs/installation.org).
                          procps
                          util-linux))

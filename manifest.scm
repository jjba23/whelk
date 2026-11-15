;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
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
                          gcc-toolchain
                          pkg-config
                          meson
                          ninja
                          wayland ; wayland-scanner, for the river-status protocol
                          gtk
                          gtk4-layer-shell
                          ;; X11: the EWMH dock fallback and the RandR gamma
                          ;; table the night light uses on an X server.
                          libx11
                          libxrandr
                          ;; `g_test_dbus_up' spawns a dbus-daemon for every
                          ;; test that talks to a private bus: the menus, the
                          ;; tray and the idle manager.  Without it those tests
                          ;; still pass, by skipping.
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
                          ;; `glib-compile-resources' lives in glib's "bin"
                          ;; output, which gtk does not propagate; without it
                          ;; meson aborts at gnome.compile_resources.
                          (list glib "bin")
                          ;; `libtiff-4.pc' lists liblzma in its
                          ;; Requires.private, and pkg-config resolves those even
                          ;; for `--cflags'.
                          xz
                          ;; An icon theme makes the tray's fallback icon and
                          ;; GTK's own symbols resolvable on a bare machine.
                          adwaita-icon-theme
                          ;; `meson test' and the shell's own scripts use
                          ;; dirname/mktemp; do not depend on the host PATH.
                          coreutils
                          ;; Reading these is how the shell is debugged without
                          ;; a compositor (see docs/installation.org).
                          procps
                          util-linux))

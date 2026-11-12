;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(use-modules (guix packages)
             (gnu)
             (gnu packages guile)
             (gnu packages admin)
             (gnu packages ninja)
             (gnu packages pkg-config)
             (gnu packages linux)
             (gnu packages gtk)
             (gnu packages glib)
             (gnu packages gnome)
             (gnu packages freedesktop)
             (gnu packages base)
             (gnu packages commencement))

(packages->manifest (list guile-3.0
                          gcc-toolchain
                          pkg-config
                          meson
                          ninja
                          gtk
                          gtk4-layer-shell
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
                          ;; a compositor (see README).
                          procps
                          util-linux))

;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(use-modules (guix packages)
             (guix gexp)
             (guix git-download)
             (guix build-system meson)
             ((guix licenses)
              #:prefix license:)
             (gnu packages gtk)
             (gnu packages glib)
             (gnu packages guile)
             (gnu packages freedesktop)
             (gnu packages pkg-config))

(define %source-dir
  (dirname (current-filename)))

(define-public whelk
  (package
    (name "whelk")
    (version "dev")
    (source
     (local-file %source-dir
                 #:recursive? #t
                 #:select? (git-predicate %source-dir)))
    (build-system meson-build-system)
    (native-inputs (list (list glib "bin")
                         pkg-config))
    (inputs (list gtk
                  gtk4-layer-shell
                  linux-pam
                  guile-3.0))
    ;; whelk invokes wtype by name to type a menu shortcut.  It has to be in
    ;; the package's profile, not only the build environment, or a shell and a
    ;; desktop launch would not put it on PATH.
    (propagated-inputs (list wtype))
    (home-page "https://github.com/jjba23/whelk")
    (synopsis "Wayland shell configured with GNU Guile Scheme")
    (description
     "Whelk is a Wayland shell built with GTK4 and @code{gtk4-layer-shell}.  Its
status bar, floating widgets and, in time, its launcher and notifications are
described in GNU Guile Scheme: a bar is a list of modules, and every module is a
reading plus the mouse actions on it.")
    (license license:expat)))

whelk

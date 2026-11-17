;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(use-modules (guix packages)
             (guix gexp)
             (guix git-download)
             (guix import crate)
             (guix build-system cargo)
             ((guix licenses)
              #:prefix license:)
             (gnu packages admin)
             (gnu packages freedesktop)
             (gnu packages glib)
             (gnu packages gtk)
             (gnu packages guile)
             (gnu packages linux)
             (gnu packages pkg-config)
             (gnu packages xorg))

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
    ;; Cargo refuses to build when the channel's rust is older than the
    ;; workspace's rust-version (1.92); use a channel that provides it.
    (build-system cargo-build-system)
    (arguments
     '(#:cargo-package-crates '("whelk-rs")
       ;; The ordinary test phase runs both the Rust tests and the retained
       ;; Guile suite.  The lock screen's PAM conversation is a deployment
       ;; check, not a sandbox one.
       #:tests? #t
       #:phases
       (modify-phases %standard-phases
         ;; GTK tests need a display even when they never map a window.  Set
         ;; HOME and the runtime directory inside the build tree, as the Nix
         ;; check does, so dconf and GLib do not write to /homeless-shelter.
         (replace 'check
           (lambda* (#:key tests? #:allow-other-keys)
             (when tests?
               (setenv "HOME" (getcwd))
               (setenv "XDG_RUNTIME_DIR" (getcwd))
               (setenv "GTK_A11Y" "test")
               (setenv "GDK_BACKEND" "x11")
               (setenv "GSK_RENDERER" "cairo")
               (invoke "xvfb-run" "-a" "cargo" "test"
                       "--workspace" "--locked"))))
         ;; Cargo installs the binary; the Scheme modules, desktop entry and
         ;; icons have no Cargo target.  The staged layout is also what makes
         ;; the binary find its modules beside its own prefix.
         (add-after 'install 'install-runtime-data
           (lambda* (#:key outputs #:allow-other-keys)
             (invoke "cargo" "run" "--offline" "--locked"
                     "-p" "xtask" "--" "install"
                     "--prefix" (assoc-ref outputs "out")
                     "--no-build"))))))
    (native-inputs
     (list (list glib "bin")
           dbus ; the notification tests use a private session bus
           guile-3.0
           pkg-config
           wayland
           xvfb-run))
    (inputs
     (append
      (cargo-inputs-from-lockfile "Cargo.lock")
      (list gtk
            gtk4-layer-shell
            guile-3.0
            linux-pam
            wayland)))
    ;; whelk invokes wtype by name to type a menu shortcut.  It has to be in
    ;; the package's profile, not only the build environment, or a shell and a
    ;; desktop launch would not put it on PATH.
    (propagated-inputs (list wtype))
    (home-page "https://github.com/jjba23/whelk")
    (synopsis "Wayland shell configured with GNU Guile Scheme")
    (description
     "Whelk is a Wayland shell built with GTK4 and @code{gtk4-layer-shell}.  Its
status bar, floating widgets, launcher and notification service are described in
GNU Guile Scheme: a bar is a list of modules, and every module is a reading plus
the mouse actions on it.")
    (license license:expat)))

whelk

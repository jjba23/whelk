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
             (gnu packages audio)
             (gnu packages curl)
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
         ;; The workspace root is a virtual manifest, so cargo-build-system's
         ;; default `cargo install --path .' cannot work here.  xtask installs
         ;; the already-built binary together with the Scheme modules, desktop
         ;; entry and icons that have no Cargo target.
         (replace 'install
           (lambda* (#:key outputs #:allow-other-keys)
             (invoke "cargo" "run" "--offline" "--locked"
                     "-p" "xtask" "--" "install"
                     "--prefix" (assoc-ref outputs "out")
                     "--no-build")))
         ;; `wtype`, `curl` and `cava` are called by name at run time, and a compositor
         ;; starts the shell with the PATH it inherited -- not a login shell's,
         ;; and not necessarily the profile that carries propagated inputs.  A
         ;; propagated input only reaches the profile of whoever installs the
         ;; package, so the binary carries both store paths itself; the cards
         ;; and the menu shortcuts then work however the session was started.
         (add-after 'install 'wrap-runtime-tools
           (lambda* (#:key inputs outputs #:allow-other-keys)
             (let* ((binary (string-append (assoc-ref outputs "out")
                                           "/bin/whelk"))
                    (tools (list (dirname (search-input-file inputs "/bin/wtype"))
                                 (dirname (search-input-file inputs "/bin/curl"))
                                 (dirname (search-input-file inputs "/bin/cava")))))
               (wrap-program binary
                 `("PATH" ":" prefix (,@tools)))))))))
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
            wayland
            ;; Called by name at run time; found by the wrap phase below, and
            ;; propagated as well so a profile carries them for scripts.
            cava
            curl
            wtype)))
    ;; whelk invokes wtype by name to type a menu shortcut, and curl to read a
    ;; weather or stock card.  Both have to be in the package's profile, not
    ;; only in the build environment, or a shell and a desktop launch would not
    ;; put them on PATH.
    (propagated-inputs (list cava wtype curl))
    (home-page "https://github.com/jjba23/whelk")
    (synopsis "Wayland shell configured with GNU Guile Scheme")
    (description
     "Whelk is a Wayland shell built with GTK4 and @code{gtk4-layer-shell}.  Its
status bar, floating widgets, launcher and notification service are described in
GNU Guile Scheme: a bar is a list of modules, and every module is a reading plus
the mouse actions on it.")
    (license license:expat)))

whelk

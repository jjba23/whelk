;; bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT

(use-modules (guix packages)
             (guix gexp)
             (guix git-download)
             (guix import crate)
             (guix build-system cargo)
             ((guix licenses)
              #:prefix license:)
             (gnu packages freedesktop)
             (gnu packages glib)
             (gnu packages gtk)
             (gnu packages guile)
             (gnu packages pkg-config))

(define %source-dir
  (dirname (current-filename)))

(define-public bibelot
  (package
    (name "bibelot")
    (version "0.1.0")
    (source
     (local-file %source-dir
                 #:recursive? #t
                 #:select? (git-predicate %source-dir)))
    (build-system cargo-build-system)
    (arguments
     '(#:cargo-package-crates '()
       #:install-source? #f
       #:tests? #t
       #:phases
       (modify-phases %standard-phases
         (replace 'check
           (lambda* (#:key tests? #:allow-other-keys)
             (when tests?
               (setenv "HOME" (getcwd))
               (setenv "XDG_RUNTIME_DIR" (getcwd))
               (invoke "cargo" "test" "--locked"))))
         (replace 'install
           (lambda* (#:key outputs #:allow-other-keys)
             (let ((out (assoc-ref outputs "out")))
               (install-file "scheme/bibelot/theme.scm"
                             (string-append out
                                            "/share/guile/site/3.0/bibelot"))
               (install-file "README.org"
                             (string-append out "/share/doc/bibelot"))
               (install-file "docs/widgets.org"
                             (string-append out "/share/doc/bibelot"))
               (install-file "LICENSE"
                             (string-append out "/share/licenses/bibelot"))))))))
    (native-inputs (list guile-3.0 pkg-config))
    (inputs
     (append
      (cargo-inputs-from-lockfile "Cargo.lock")
      (list gtk gtk4-layer-shell guile-3.0 wayland)))
    (home-page "https://github.com/jjba23/bibelot")
    (synopsis "Floating desktop widget cards")
    (description
     "Bibelot owns the desktop widget catalogue, its readings, placement and
state file, the Gtk4 card surfaces, and the (bibelot theme) Scheme module.  It
is a library: a host shell supplies the application, skin and settings path.")
    (license license:expat)))

bibelot

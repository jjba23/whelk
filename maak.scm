;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(define-module (maak)
  #:declarative? #t
  #:use-module (maak maak)
  ;; The task named `run' shadows the shell helper of that name for the rest of
  ;; this file, so every command here is started through `dsl:run' -- the
  ;; helper under the name of the module it comes from -- instead.
  #:use-module ((maak dsl) #:prefix dsl:)
  ;; `scandir' is what the `diagrams' task walks resources/diagrams with.
  #:use-module (ice-9 ftw))

;; ===== Maak tasks =====

(define project-name
  'whelk)

;; The URL for the CSS stylesheet used to style API documentation.
(define api-docs-css-url
  "https://jointhefreeworld.org/static/css/jointhefreeworld.css")

(define (run)
  "Run whelk inside the Guix environment."
  ;; Nothing of the right name running is not a failure, so the command is
  ;; allowed to say so instead of the task dying on `pkill''s non-zero exit.
  (dsl:run (list "pkill" "whelk") #:allow-failure? #t)
  ;; The shell no longer searches the current directory for Scheme modules:
  ;; a packaged build must not load whatever happens to sit beside the
  ;; caller.  The development tree names its modules explicitly.
  (time-machine-program-shell
   (list "env" "WHELK_SCHEME_DIR=./scheme"
         "cargo" "run" "-p" "whelk-rs" "--release")
   #:verbose? #t))

(define (test)
  "Format, lint and run the whole Cargo test suite inside the Guix environment."
  ;; The script runs inside the shell: it also gives the loader the Guix
  ;; library path that Cargo's dynamically linked build scripts need.
  (time-machine-manifest-shell
   (list "sh" "scripts/guix-test.sh")
   #:verbose? #t))

(define (clippy)
  "Run the same lints as `test' without the format and test phases."
  (time-machine-manifest-shell
   (list "sh" "scripts/guix-test.sh" "clippy")
   #:verbose? #t))

(define (docs)
  "Generate the Scheme layer's API reference and render it as HTML.

whelk is a Rust shell with a Guile configuration engine, and the two are
documented separately: `cargo doc' is the Rust API and is not run here, while
this task reads `scheme/whelk' and writes what dokgen extracts from it.  Dokgen
loads the modules rather than parsing their text, so every exported procedure
that has a docstring is in the reference.

`docs/' holds the hand-written pages; the generated chapter goes to `doc/api'
and its rendering to `doc/api-dist', so neither replaces a page.  Beside the
Texinfo dokgen writes the stylesheet and the script the page is read with, and
a directory of diagrams for the docstrings that carry one; Texinfo copies
neither, so both are put beside the page that points at them."
  (delete-file-recursively "doc/api")
  (time-machine-manifest-shell
   (list "dokgen" "api"
         "--destination" "doc/api"
         "./scheme/whelk")
   #:verbose? #t)
  (mkdir-p "doc/api-dist")
  (time-machine-manifest-shell
   (list "texi2any" "-v" "--html" "--no-split"
         (~ "--css-ref=~a" api-docs-css-url)
         "-I" "doc/api"
         "-c" (shell-quote
               (string-append
                "EXTRA_HEAD="
                "<link rel=\"stylesheet\" href=\"dokgen.css\">"
                "<script src=\"dokgen.js\" defer></script>"))
         "-o" "doc/api-dist/index.html"
         "./doc/api/index.texi")
   #:verbose? #t)
  (for-each (lambda (asset)
              (when (file-exists? (~ "doc/api/~a" asset))
                (cp (~ "doc/api/~a" asset) "doc/api-dist"
                    #:force? #t #:verbose? #t)))
            '("dokgen.css" "dokgen.js"))
  (when (file-exists? "doc/api/diagrams")
    (cp "doc/api/diagrams" "doc/api-dist" #:force? #t #:verbose? #t)))

(define (diagrams)
  "Render every mermaid source under resources/diagrams to the SVG beside it.

A diagram is committed twice -- the source and the rendering -- so reading the
README needs no renderer.  `mmdc' is a JavaScript tool the pinned shell does not
carry, so it is looked up on $PATH, where `npm install --global
@mermaid-js/mermaid-cli' puts it."
  (for-each
   (lambda (name)
     (let* ((source (in-vicinity "resources/diagrams" name))
            (image (string-append (string-drop-right source 4) ".svg")))
       (dsl:run (list "mmdc" "-i" source "-o" image
                      "--backgroundColor" "transparent")
                #:verbose? #t)))
   (scandir "resources/diagrams"
            (lambda (name)
              (and (not (member name '("." "..")))
                   (string-suffix? ".mmd" name))))))

(define deb-packages
  (make-parameter (map symbol->string
                       '(build-essential debhelper
                                         devscripts
                                         cme
                                         lintian
                                         cargo
                                         rustc
                                         pkg-config
                                         libgtk-4-dev
                                         libgtk4-layer-shell-dev
                                         guile-3.0-dev
                                         libwayland-dev
                                         libglib2.0-dev-bin))))

(define (deb-build)
  "Build .deb package (unsigned)"
  (dsl:run (list "apt" "update")
           #:sudo? #t
           #:verbose? #t)
  (dsl:run (append (list "apt" "install" "-y")
                   (deb-packages))
           #:sudo? #t
           #:verbose? #t)

  (dynamic-wind
   ;; Setup: create temporary symlink at project root
   (lambda ()
     (ln-s "packaging/debian" "debian"
           #:force? #f
           #:verbose? #t))

   ;; Execution: run debian build commands against ./debian
   (lambda ()
     (dsl:run (list "mk-build-deps" "-i" "-r" "debian/control")
              #:sudo? #t
              #:verbose? #t)
     (dsl:run (list "debuild" "-b" "-uc" "-us")
              #:verbose? #t))

   ;; Cleanup: remove symlink safely (leaves packaging/debian intact)
   (lambda ()
     (rm-rf "debian"
            #:verbose? #t))))

(define (arch-build)
  "Build Arch Linux package (.pkg.tar.zst)"
  (dsl:run (list "makepkg" "-si")
           #:verbose? #t)
  (dsl:run (list "namcap" "packaging/arch/PKGBUILD")
           #:verbose? #t))

(define rpm-packages
  (make-parameter (map symbol->string
                       '(rpm-build rpmdevtools
                                   gcc
                                   cargo
                                   rust
                                   pkgconfig
                                   gtk4-devel
                                   gtk4-layer-shell-devel
                                   guile30-devel
                                   libwayland-client-devel
                                   pam-devel))))

(define (rpm-build)
  "Build RPM package for openSUSE and Fedora/RHEL"
  (let* ((latest-tag (git-describe-tag))
         ;; `git describe' prints nothing in a dry run, and a version is not
         ;; derived from nothing: the tag is kept as it came when it carries no
         ;; leading `v'.
         (latest-tag-num (cond ((string=? latest-tag "") "")
                               ((string-prefix? "v" latest-tag)
                                (string-drop latest-tag 1))
                               (else latest-tag)))
         (sources (~ "~a/rpmbuild/SOURCES" (home-dir)))
         (specs (~ "~a/rpmbuild/SPECS" (home-dir)))
         (rpms (~ "~a/rpmbuild/RPMS/x86_64" (home-dir)))
         (srpms (~ "~a/rpmbuild/SRPMS" (home-dir))))
    (dsl:run (list "rpmdev-setuptree")
             #:verbose? #t)
    (git-archive-tar (~ "~a/whelk-~a.tar.gz" sources latest-tag-num)
                     #:prefix (~ "whelk-~a/" latest-tag-num)
                     #:revision latest-tag)
    (cp "packaging/opensuse/whelk.spec" specs
        #:verbose? #t)
    (dsl:run (list "rpmbuild" "-ba" (~ "~a/whelk.spec" specs))
             #:verbose? #t)
    (dsl:run (list "ls" (~ "~a/whelk-[0-9]*.rpm" rpms))
             #:expand? #t)
    (let* ((latest-rpm
            (string-trim-both
             (capture (~ "ls -t ~a/whelk-[0-9]*.rpm | head -n 1" rpms))))
           (latest-src-rpm
            (string-trim-both
             (capture (~ "ls -t ~a/whelk-[0-9]*.rpm | head -n 1" srpms)))))
      (cp "packaging/opensuse/whelk-rpmlintrc" rpms
          #:verbose? #t)
      (dsl:run (list "rpmlint" latest-rpm)
               #:verbose? #t)
      (cp "packaging/opensuse/whelk-rpmlintrc" srpms
          #:verbose? #t)
      (dsl:run (list "rpmlint" latest-src-rpm)
               #:verbose? #t))))

(define (cargo-build)
  "Cargo release build."
  (time-machine-program-shell (list "cargo" "build" "--release" "--bin" "whelk")))

(define (debug-run)
  "Debug build, then gdb."
  (time-machine-program-shell (list "cargo" "build" "--bin" "whelk"))
  (dsl:run (list "env" "WHELK_SCHEME_DIR=./scheme"
                 "gdb" "--args" "./target/debug/whelk")))

(define (default)
  "List the tasks this project defines."
  (list-tasks))

;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

(define-module (maak)
  #:declarative? #t
  #:use-module (maak maak)
  ;; The task named `run' shadows the shell helper of that name for the rest of
  ;; this file, so every command here is started through `dsl:run' -- the
  ;; helper under the name of the module it comes from -- instead.
  #:use-module ((maak dsl) #:prefix dsl:))

;; ===== Maak tasks =====

(define project-name
  'whelk)

(define (run)
  "Run whelk inside the Guix environment."
  ;; Nothing of the right name running is not a failure, so the command is
  ;; allowed to say so instead of the task dying on `pkill''s non-zero exit.
  (dsl:run (list "pkill" "whelk") #:allow-failure? #t)
  (time-machine-program-shell (list "whelk")
                              #:verbose? #t))

(define (test)
  "Build and run the whole test suite inside the Guix environment.

The shell wrapper only applies to the first command, so the pipeline is run by
one inner `sh -c' instead of a bare `&&': otherwise `ninja' would execute
outside the Guix environment.

The build directory is thrown away first: a tree configured by a different
Meson version makes both `meson setup' and `ninja' fail with 'build data file
... references functions or classes that don't exist'. Rebuilding from scratch
also means the passing tests are the ones that match the checked out sources."
  (time-machine-manifest-shell
   (list "/bin/sh" "-c"
         ;; One token, because it is one shell script.
         (~ "'~a'"
            (string-join
             (list "set -eu"
                   "rm -rf build"
                   "meson setup build"
                   "ninja -C build"
                   "meson test -C build --print-errorlogs")
             "; ")))
   #:verbose? #t))

(define c-packages
  (make-parameter
   (string-join (map symbol->string
                     '(gtk4 guile-3.0 gtk4-layer-shell-0 wayland-client
                            gio-unix-2.0))
                " ")))

(define (add-libs)
  "Regenerate .ccls for the LSP from the Guix store paths."
  (delete-file-recursively ".ccls")
  ;; A pipe and an append-redirection, so this is one of the lines that needs a
  ;; shell.  Every part is a word of it, and `tr''s second set is the `\n'
  ;; escape the shell hands over as a newline character.
  (time-machine-manifest-shell
   (append (list "pkg-config" "--cflags")
           (string-split (c-packages) #\space)
           (list "|" "tr" "' '" "'\\n'" ">>" ".ccls"))
   #:verbose? #t)
  (cat ".ccls"))

(define deb-packages
  (make-parameter (map symbol->string
                       '(build-essential debhelper
                                         devscripts
                                         cme
                                         lintian
                                         meson
                                         ninja-build
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
                                   meson
                                   ninja-build
                                   pkgconfig
                                   gtk4-devel
                                   gtk4-layer-shell-devel
                                   guile30-devel
                                   libwayland-client-devel))))

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

(define (meson-build)
  "Meson build."
  (time-machine-program-shell (list "meson" "setup" "build"))
  (time-machine-program-shell (list "ninja" "-C" "build")))

(define (debug-run)
  "Debug run whelk."
  (meson-build)
  (dsl:run (list "gdb" "--args" "./build/whelk")))

(define (default)
  "List the tasks this project defines."
  (list-tasks))

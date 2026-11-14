;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; Loading a settings file.  The file is evaluated in a fresh module that
;; imports (whelk core) and (whelk modules), so it can call `configure-shell!'
;; and `bar' with no incantation at the top, it can define whatever helpers it
;; likes without leaking them into the next load, and `define-module' is
;; shadowed because a settings file is not a module -- the same traversal
;; lambdock's loader performs for its docks.

;;; Code:

(define-module (whelk loader)
  #:use-module (whelk logger)
  #:use-module (whelk menudict)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:export (get-config-path
            default-config-dir
            load-settings-file!))

(define (default-config-dir)
  "Return whelk's configuration directory, following the XDG spec."
  (let ((xdg (getenv "XDG_CONFIG_HOME"))
        (home (getenv "HOME")))
    (cond
      ((and xdg (not (string-null? xdg))) (string-append xdg "/whelk"))
      (home (string-append home "/.config/whelk"))
      (else "/tmp/whelk"))))

(define (get-config-path)
  "Return the path of the primary settings file."
  (string-append (default-config-dir) "/settings.scm"))

(define (fresh-settings-module)
  "Return a module in which a settings file can be evaluated."
  (let ((anon (make-fresh-user-module)))
    (module-use! anon (resolve-module '(whelk core)))
    (module-use! anon (resolve-module '(whelk modules)))
    anon))

(define (load-settings-file! file-path)
  "Evaluate FILE-PATH as a whelk configuration.

Returns #t when the file was evaluated, #f when it is missing, and #f after
logging the reason when it raised.  A settings file that throws half way leaves
whatever it configured before the throw registered, which is why the caller
still asks for the bars afterwards rather than aborting the shell."
  (cond
    ((not (and file-path (string? file-path) (file-exists? file-path)))
     (log-debug "Loader" "No settings file at ~a" file-path)
     #f)
    (else
     (log-info "Loader" "Reading settings from ~a" file-path)
     ;; The menu dictionary starts from the built-in table on every load: a
     ;; reload must replace what the last version of the file added, not stack
     ;; another copy of it in front.
     (reset-menu-edits!)
     ;; A scheme-module's procedure belongs to the file that defined it; the
     ;; token the previous file handed to C must not resolve against this one.
     (reset-module-procedures!)
     (forget-shell-config! file-path)
     (let ((anon (fresh-settings-module)))
       (catch #t
              (lambda ()
                (save-module-excursion
                 (lambda ()
                   (set-current-module anon)
                   ;; A settings file must not switch modules: everything it
                   ;; defines has to stay in the module being loaded.
                   (eval '(define-syntax define-module
                            (syntax-rules ()
                              ((_ . rest) #f)))
                         anon)
                   (primitive-load file-path)))
                #t)
              (lambda (key . args)
                (log-error "Loader" "Settings file ~a failed: ~a ~s"
                           file-path key args)
                #f))))))

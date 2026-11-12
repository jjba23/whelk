;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; The public face of the configuration engine.  A settings file sees exactly
;; what this module re-exports: the module constructors, the bar record and its
;; accessors, `configure-shell!', and the entry points the C runtime calls
;; (`init-shell!', `load-shell-config!', `start-repl!').
;;
;; This is also where the fallback configuration lives.  A shell with no
;; settings file still has to be a shell, and a bar with a clock, the machine's
;; load and a tray is the smallest honest one -- the same courtesy lambdock pays
;; a fresh installation.

;;; Code:

(define-module (whelk core)
  #:declarative? #f
  #:use-module (ice-9 threads)
  #:use-module (system repl server)
  #:use-module (whelk logger)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:use-module (whelk loader)
  #:re-export (<shell-module>
               shell-module?
               shell-module-kind
               shell-module-options
               module
               module-option
               module-options-ref
               clock-module
               cpu-module
               memory-module
               battery-module
               backlight-module
               audio-module
               power-profile-module
               workspaces-module
               window-module
               language-module
               keyboard-module
               custom-module
               wifi-module
               bluetooth-module
               notifications-module
               tray-module
               <bar-config>
               bar-config?
               make-bar-config
               bar-config-name
               bar-config-edge
               bar-config-monitor
               bar-config-height
               bar-config-margin-top
               bar-config-margin-right
               bar-config-margin-bottom
               bar-config-margin-left
               bar-config-exclusive?
               bar-config-spacing
               bar-config-padding
               bar-config-font
               bar-config-font-size
               bar-config-icon-font
               bar-config-foreground
               bar-config-background
               bar-config-accent
               bar-config-border-color
               bar-config-radius
               bar-config-css
               bar-config-modules-left
               bar-config-modules-center
               bar-config-modules-right
               bar
               configure-shell!
               get-shell-config
               get-shell-bars
               get-shell-enable-repl?
               get-shell-repl-socket-path
               get-config-path
               load-settings-file!)
  #:export (init-shell! load-shell-config! default-shell! start-repl!))

;; =============================================================================
;; Loading
;; =============================================================================

(define* (load-shell-config! file-path)
  "Read FILE-PATH and register the shell it describes.

Called by the C runtime at startup and again whenever the configuration
directory changes, so a bar can be edited and seen without restarting the
compositor."
  (let ((loaded? (load-settings-file! file-path))
        (bars (get-shell-bars file-path)))
    (log-info "Core" "Configuration [~a]: ~a bar(s)"
              file-path (length bars))
    loaded?))

(define (default-shell!)
  "Register the fallback bar: clock, load, memory, battery and tray.

The order is the one a new machine can actually use: what time it is, how hard
it is working, what is left, and the status icons of everything else."
  (configure-shell!
   #:config-path 'default
   #:bars (list (bar #:name 'main
                     #:position 'top
                     #:height 32
                     #:right (list (cpu-module #:interval 2)
                                   (memory-module #:interval 5)
                                   (battery-module #:interval 30)
                                   (tray-module)
                                   (clock-module #:interval 1))))))

(define (init-shell!)
  "Entry point invoked by the C runtime once Guile is up.

Reads the primary settings file, and falls back to `default-shell!' when that
file left nothing configured (missing, empty, or broken)."
  (let ((path (get-config-path)))
    (load-shell-config! path)
    (when (null? (get-shell-bars path))
      (log-warn "Core"
                "No bars configured in ~a: using the fallback bar" path)
      (default-shell!))
    (start-repl! path)))

;; =============================================================================
;; REPL
;; =============================================================================
;;
;; The REPL runs on its own thread, like lambdock's: a configuration can be
;; changed while the shell is up, and every C primitive it calls marshals to the
;; GTK thread itself.  Only sockets that are already being served are tracked,
;; so reloading a settings file does not try to bind the same path twice.

(define *repl-sockets* '())

(define (make-directories! path)
  (unless (or (string-null? path) (file-exists? path))
    (make-directories! (dirname path))
    (catch 'system-error (lambda () (mkdir path)) (lambda _ #t))))

(define* (start-repl! . args)
  "Serve a REPL on the configured socket, when the configuration asks for one.

With no argument the default configuration is consulted; a settings path may be
passed instead (the C runtime passes the file it just loaded)."
  (let* ((key (if (null? args) 'default (car args)))
         (enabled? (get-shell-enable-repl? key))
         (socket-path (get-shell-repl-socket-path key)))
    (when (and enabled? (not (member socket-path *repl-sockets*)))
      (set! *repl-sockets* (cons socket-path *repl-sockets*))
      (log-info "Core" "Serving a REPL on ~a" socket-path)
      (call-with-new-thread
       (lambda ()
         (catch #t
                (lambda ()
                  (make-directories! (dirname socket-path))
                  (when (file-exists? socket-path)
                    (delete-file socket-path))
                  (spawn-server (make-unix-domain-server-socket
                                 #:path socket-path)))
                (lambda (key . args)
                  (log-error "Core" "REPL on ~a failed: ~a ~s"
                             socket-path key args))))))))

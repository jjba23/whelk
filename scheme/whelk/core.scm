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
;; settings file still has to be a shell, and the one it gets is the built-in
;; bar from (whelk defaults): the heks-linux Waybar bar, the global menu
;; included, so a fresh machine lands on the bar the rest of the desktop is
;; documented against rather than on a placeholder.

;;; Code:

(define-module (whelk core)
  #:declarative? #f
  #:use-module (ice-9 threads)
  #:use-module (system repl server)
  #:use-module (whelk defaults)
  #:use-module (whelk logger)
  #:use-module (whelk menu)
  #:use-module (whelk menudict)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:use-module (whelk themes)
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
               start-module
               menu-module
               menu-item
               menu-separator
               menu-submenu
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
               themes
               theme-palette
               theme-exists?
               configure-shell!
               get-shell-config
               get-shell-bars
               get-shell-menu
               get-shell-enable-repl?
               get-shell-repl-socket-path
               get-config-path
               load-settings-file!
               default-bar
               add-menu-app
               replace-menu-app
               menudict-for-id)
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
  "Register the built-in bar as the fallback configuration.

It is the bar settings/settings.scm hands out -- (default-bar) from (whelk
defaults), which is the heks-linux Waybar bar plus the global menu -- so an
installation with no settings file still gets the bar this project documents,
not a stripped-down stand-in for it."
  (default-configuration!))

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

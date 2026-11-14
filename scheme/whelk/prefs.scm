;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; GUI preferences are runtime state, not configuration.  The settings window
;; writes the alist below under XDG_STATE_HOME, and the loader layers it over
;; the bars it just read.  The primary settings file is never rewritten: a GUI
;; change is a reversible override, and deleting the override file is the
;; "reset to configuration" action.
;;
;; One file per settings file, like lambdock's preferences-<stem>.scm, so two
;; configurations can have two different GUI looks without knowing about each
;; other.

;;; Code:

(define-module (whelk prefs)
  #:declarative? #f
  #:use-module (ice-9 pretty-print)
  #:use-module (srfi srfi-1)
  #:use-module (whelk logger)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:use-module (whelk themes)
  #:export (preferences-path
            preferences-read
            save-preference!
            reset-preferences!
            apply-shell-preferences!))

;; =============================================================================
;; Where the overrides live
;; =============================================================================

(define (whelk-state-dir)
  "The directory whelk keeps runtime state in, following XDG."
  (let ((xdg (getenv "XDG_STATE_HOME"))
        (home (getenv "HOME")))
    (cond ((and xdg (not (string-null? xdg)))
           (string-append xdg "/whelk"))
          (home (string-append home "/.local/state/whelk"))
          (else "/tmp/whelk"))))

(define (settings-stem config-path)
  "CONFIG-PATH's file name without its .scm suffix."
  (let* ((base (basename config-path))
         (suffix ".scm"))
    (if (string-suffix? suffix base)
        (string-drop-right base (string-length suffix))
        base)))

(define (preferences-path config-path)
  "Return the override file belonging to CONFIG-PATH, or #f."
  (and (string? config-path)
       (not (string-null? config-path))
       (string-append (whelk-state-dir)
                      "/preferences-"
                      (settings-stem config-path)
                      ".scm")))

(define (make-directories! path)
  "Create PATH and its parents, ignoring a directory that already exists."
  (unless (or (string-null? path) (file-exists? path))
    (let ((parent (dirname path)))
      (unless (string=? parent path)
        (make-directories! parent)))
    (catch 'system-error
      (lambda () (mkdir path))
      (lambda _ #t))))

;; =============================================================================
;; Reading and writing
;; =============================================================================

(define (preference-pair? value)
  (and (pair? value)
       (symbol? (car value))))

(define (preferences-read config-path)
  "Return CONFIG-PATH's GUI overrides as an alist, newest first.

A missing file is not an error: it means the configuration is still in charge.
A malformed file is reported and ignored as a whole, so a half-written state
file cannot leave the shell with a half-applied look."
  (let ((path (preferences-path config-path)))
    (if (not (or (and path (file-exists? path))))
        '()
        (catch #t
          (lambda ()
            (call-with-input-file path
              (lambda (port)
                (let ((value (read port)))
                  (if (and (list? value) (every preference-pair? value))
                      (filter preference-pair? value)
                      (begin
                        (log-warn "Prefs"
                                  "Ignoring malformed preference file: ~a"
                                  path)
                        '()))))))
          (lambda (key . args)
            (log-warn "Prefs" "Could not read ~a: ~a ~s" path key args)
            '())))))

;; Only the keys the settings window actually edits.  A typo in a widget key
;; must not become a setting that the rest of whelk silently ignores.
(define %preference-keys
  '(bar-height bar-spacing bar-padding bar-edge bar-layer bar-exclusive?
    bar-font bar-font-size bar-theme
    wifi-show-ssid? wifi-interval
    notifications-show-count?
    menu-shortcuts? menu-app-name? menu-accelerator-style
    tray-max-items clock-format))

(define (save-preference! config-path key value)
  "Save one GUI preference for CONFIG-PATH and return #t on success.

KEY is a symbol from the settings window's vocabulary.  Existing values are
replaced rather than appended, so reading the file never depends on which
duplicate happens to win."
  (let ((path (preferences-path config-path)))
    (cond
      ((not path)
       (log-warn "Prefs" "Cannot save a preference without a settings path")
       #f)
      ((not (memq key %preference-keys))
       (log-warn "Prefs" "Ignoring unknown preference key: ~a" key)
       #f)
      (else
       (catch #t
         (lambda ()
           (make-directories! (dirname path))
           (let* ((old (preferences-read config-path))
                  (updated (cons (cons key value)
                                 (filter (lambda (pair)
                                           (not (eq? (car pair) key)))
                                         old))))
             (call-with-output-file path
               (lambda (port)
                 (display ";; whelk GUI preferences -- delete this file to reset.\n"
                          port)
                 (pretty-print updated port)))
             #t))
         (lambda (key* . args)
           (log-error "Prefs" "Could not save ~a: ~a ~s" path key* args)
           #f))))))

(define (reset-preferences! config-path)
  "Delete CONFIG-PATH's GUI overrides, returning #t when none are left."
  (let ((path (preferences-path config-path)))
    (if (not path)
        #f
        (catch #t
          (lambda ()
            (when (file-exists? path)
              (delete-file path))
            #t)
          (lambda (key . args)
            (log-warn "Prefs" "Could not reset ~a: ~a ~s" path key args)
            #f)))))

;; =============================================================================
;; Layering the overrides over a loaded bar
;; =============================================================================

(define (preference prefs key fallback)
  (let ((pair (assq key prefs)))
    (if pair (cdr pair) fallback)))

(define (integer-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (if (and (number? value) (>= value 0))
        (inexact->exact (round value))
        fallback)))

(define (string-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (if (and (string? value) (not (string-null? value)))
        value
        fallback)))

(define (boolean-preference prefs key fallback)
  (let ((pair (assq key prefs)))
    (if pair (if (cdr pair) #t #f) fallback)))

(define (symbol-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (cond ((symbol? value) value)
          ((string? value) (string->symbol value))
          (else fallback))))

(define (theme-preference prefs bar)
  (symbol-preference prefs 'bar-theme (bar-config-theme bar)))

(define (apply-bar bar prefs)
  (let* ((theme (theme-preference prefs bar))
         (palette (theme-palette theme))
         (theme-set? (and (assq 'bar-theme prefs) #t))
         (from-palette (lambda (key current)
                         (if theme-set?
                             (assq-ref palette key)
                             current)))
         (radius (if theme-set?
                     (or (assq-ref palette 'radius) 0)
                     (bar-config-radius bar))))
    (make-bar-config
     (bar-config-name bar)
     theme
     (normalize-edge (preference prefs 'bar-edge (bar-config-edge bar)))
     (normalize-layer (preference prefs 'bar-layer (bar-config-layer bar)))
     (bar-config-monitor bar)
     (bar-config-monitors bar)
     (bar-config-when bar)
     (integer-preference prefs 'bar-height (bar-config-height bar))
     (bar-config-margin-top bar)
     (bar-config-margin-right bar)
     (bar-config-margin-bottom bar)
     (bar-config-margin-left bar)
     (boolean-preference prefs 'bar-exclusive? (bar-config-exclusive? bar))
     (integer-preference prefs 'bar-spacing (bar-config-spacing bar))
     (integer-preference prefs 'bar-padding (bar-config-padding bar))
     (string-preference prefs 'bar-font (bar-config-font bar))
     (string-preference prefs 'bar-font-size (bar-config-font-size bar))
     (bar-config-icon-font bar)
     (from-palette 'foreground (bar-config-foreground bar))
     (from-palette 'background (bar-config-background bar))
     (from-palette 'accent (bar-config-accent bar))
     (from-palette 'border-color (bar-config-border-color bar))
     radius
     (from-palette 'css (bar-config-css bar))
     (map (lambda (mod) (apply-module mod prefs))
          (bar-config-modules-left bar))
     (map (lambda (mod) (apply-module mod prefs))
          (bar-config-modules-center bar))
     (map (lambda (mod) (apply-module mod prefs))
          (bar-config-modules-right bar)))))

(define (set-option-if-present mod prefs preference-key option-key)
  (let ((pair (assq preference-key prefs)))
    (if pair
        (module-set-option mod option-key (cdr pair))
        mod)))

(define (apply-module mod prefs)
  (case (shell-module-kind mod)
    ((wifi)
     (let ((mod (set-option-if-present mod prefs
                                       'wifi-show-ssid? 'show-ssid?)))
       (set-option-if-present mod prefs 'wifi-interval 'interval)))
    ((notifications)
     (let ((pair (assq 'notifications-show-count? prefs)))
       (if pair
           (module-set-option mod 'format
                              (if (cdr pair) "{icon} {count}" "{icon}"))
           mod)))
    ((menu)
     (let ((mod (set-option-if-present mod prefs
                                       'menu-shortcuts? 'shortcuts?)))
       (let ((mod (set-option-if-present mod prefs
                                         'menu-app-name? 'app-name?)))
         (set-option-if-present mod prefs
                                'menu-accelerator-style 'accelerator-style))))
    ((tray)
     (set-option-if-present mod prefs 'tray-max-items 'max-items))
    ((clock)
     (set-option-if-present mod prefs 'clock-format 'format))
    (else mod)))

(define (apply-shell-preferences! config-path)
  "Apply CONFIG-PATH's GUI overrides to the shell registered for it.

Returns the bars as they now stand.  Called after every settings load and by
the settings window's reload callback, so the two paths cannot drift apart."
  (let* ((prefs (preferences-read config-path))
         (bars (get-shell-bars config-path)))
    (if (or (null? prefs) (null? bars))
        bars
        (let ((updated (map (lambda (bar) (apply-bar bar prefs)) bars)))
          (configure-shell!
           #:config-path config-path
           #:bars updated
           ;; A GUI edit changes a bar, not the desktop: the widgets the
           ;; settings file registered are carried across unchanged.  Leaving
           ;; them out here re-registered the shell with no widgets at all,
           ;; which is how every desktop widget disappeared the first time a
           ;; preference was saved.
           #:widgets (get-shell-widgets config-path)
           #:shell-menu (get-shell-menu config-path)
           #:clipboard (get-shell-clipboard config-path)
           #:enable-repl? (get-shell-enable-repl? config-path)
           #:repl-socket-path (get-shell-repl-socket-path config-path))
          updated))))

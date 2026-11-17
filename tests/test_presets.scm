;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; Presets are bundles, and a bundle that switches the wrong things is worse
;; than no preset at all.  The shipped two are checked against the pieces they
;; claim to be: current must still be the shipped bar and widget set, and xp
;; must be a bottom taskbar with the focused window in it.

(use-modules (whelk defaults)
             (whelk modules)
             (whelk prefs)
             (whelk presets)
             (whelk presets current)
             (whelk presets xp)
             (whelk state)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define (kinds modules)
  (map shell-module-kind modules))

(check "the current preset is registered"
       (preset? (preset-ref 'current)))
(check "the xp preset is registered"
       (preset? (preset-ref 'xp)))
(check "the catalog names both presets"
       (and (memq 'current (presets)) (memq 'xp (presets))))

;; `current' is the shipped configuration under another name: the default bar
;; and the Golden Gate widget set, not a copy that can drift.
(define current (preset-ref 'current))
(define current-bars (preset-bars current))

(check "current carries one bar"
       (= (length current-bars) 1))
(check "current is the default bar"
       (equal? (bar->alist (car current-bars)) (bar->alist (default-bar))))
(check "current keeps the Golden Gate widgets"
       (and (= (length (preset-widgets current))
               (length (golden-gate-widgets)))
            (equal? (map widget-config-kind (preset-widgets current))
                    (map widget-config-kind (golden-gate-widgets)))))

;; The XP preset: bottom taskbar, Luna blue, focused window in the centre.
(define xp (preset-ref 'xp))
(define xp-taskbar (car (preset-bars xp)))

(check "xp carries one bar"
       (= (length (preset-bars xp)) 1))
(check "xp is a bottom bar"
       (eq? (bar-config-edge xp-taskbar) 'bottom))
(check "xp is named xp"
       (eq? (bar-config-name xp-taskbar) 'xp))
(check "xp wears Luna blue"
       (and (equal? (bar-config-background xp-taskbar) "#245edb")
            (equal? (bar-config-foreground xp-taskbar) "#ffffff")
            (= (bar-config-radius xp-taskbar) 0)))
(check "xp lists every open window in its centre"
       ;; A taskbar, not a title: the centre is the task list, which focuses a
       ;; window on a click and closes it on a middle click.
       (equal? (kinds (bar-config-modules-center xp-taskbar)) '(tasks)))
(check "xp keeps the tray, the time and the battery on the right"
       (let ((right (kinds (bar-config-modules-right xp-taskbar))))
         (and (memq 'tray right)
              (memq 'clock right)
              (memq 'battery right))))
(check "xp has a Start menu"
       (and (list? (preset-shell-menu xp))
            (not (null? (preset-shell-menu xp)))))
(check "xp does not put widgets on the desktop"
       (null? (preset-widgets xp)))

;; The one call a settings file makes.
(check "a preset can be applied"
       (use-preset! 'xp #:config-path 'presets-test))
(check "the applied preset is the one on the bar"
       (eq? (bar-config-name (car (get-shell-bars 'presets-test))) 'xp))
(check "the applied preset carries its menu"
       (not (null? (get-shell-menu 'presets-test))))

(check "an unknown preset is refused"
       (not (use-preset! 'no-such-preset #:config-path 'presets-missing)))
(check "and names do not resolve to a bundle"
       (not (preset-ref 'no-such-preset)))

;; ---------------------------------------------------------------------------
;; Overrides: a preset is a starting point, not a fork
;; ---------------------------------------------------------------------------
;;
;; Changing one colour or one height used to mean copying the preset module and
;; maintaining it.  The keyword arguments replace the bundle's own list, so the
;; bundle constructor is still the thing being edited.

(define taller-bar (xp-bar #:height 40))

(check "the overridden bar is the shell's bar"
       (begin
         (use-preset! 'xp #:config-path 'presets-override
                      #:bars (list taller-bar))
         (equal? (bar->alist (car (get-shell-bars 'presets-override)))
                 (bar->alist taller-bar))))
(check "one keyword keeps the rest of the bundle"
       ;; The override replaced the bar, not the preset: the preset's own
       ;; colours are what the caller did not touch.
       (let ((applied (car (get-shell-bars 'presets-override))))
         (and (= (bar-config-height applied) 40)
              (equal? (bar-config-background applied) "#245edb"))))
(check "widgets can be overridden to none"
       (begin
         (use-preset! 'current #:config-path 'presets-no-widgets #:widgets '())
         (null? (get-shell-widgets 'presets-no-widgets))))
(check "the preset's widgets are used when nothing overrides them"
       (begin
         (use-preset! 'current #:config-path 'presets-widgets)
         (= (length (get-shell-widgets 'presets-widgets))
            (length (golden-gate-widgets)))))

;; ---------------------------------------------------------------------------
;; Choosing a preset outside the settings file
;; ---------------------------------------------------------------------------
;;
;; `whelk --preset xp' must work on a machine whose settings file says something
;; else, and it must survive the restart that follows.  The choice lives in the
;; same override file as the window's, so one file is still the whole state.

(define root (or (getenv "TMPDIR") "/tmp"))
(define state-dir
  (string-append root "/whelk-presets-test-"
                 (number->string (current-time))))

(mkdir state-dir)
(setenv "XDG_STATE_HOME" state-dir)
;; A developer's environment must not decide what these checks see.
(setenv "WHELK_PRESET" "")

(define choice-path (string-append state-dir "/settings.scm"))

(check "nothing is chosen to begin with"
       (not (chosen-preset choice-path)))
(check "an unknown name is not remembered"
       (not (choose-preset! choice-path 'no-such-preset)))
(check "and leaves nothing behind"
       (not (chosen-preset choice-path)))

(check "a known name is remembered"
       (choose-preset! choice-path 'xp))
(check "and read back as the same name"
       (eq? (chosen-preset choice-path) 'xp))
(check "the choice is a preference, so the loader sees it"
       (eq? (preference-ref choice-path 'preset) 'xp))

;; The load applies it, over whatever the file registered.
(check "the remembered preset is what a load registers"
       (begin
         (configure-shell! #:config-path choice-path
                           #:bars (list (bar #:name 'mine)))
         (apply-preset-choice! choice-path)
         (eq? (bar-config-name (car (get-shell-bars choice-path))) 'xp)))

(check "WHELK_PRESET wins over the remembered choice for one run"
       (begin
         (setenv "WHELK_PRESET" "current")
         (apply-preset-choice! choice-path)
         (not (eq? (bar-config-name (car (get-shell-bars choice-path))) 'xp))))
(check "and the remembered choice is still xp"
       (eq? (chosen-preset choice-path) 'xp))

(check "WHELK_PRESET=forget follows the settings file instead"
       (begin
         (setenv "WHELK_PRESET" "forget")
         (configure-shell! #:config-path choice-path
                           #:bars (list (bar #:name 'mine)))
         (apply-preset-choice! choice-path)
         (eq? (bar-config-name (car (get-shell-bars choice-path))) 'mine)))
(setenv "WHELK_PRESET" "")

(check "a choice can be forgotten"
       (choose-preset! choice-path #f))
(check "and the shell is handed back to the settings file"
       (not (chosen-preset choice-path)))
(check "a forgotten choice leaves the other overrides alone"
       (begin
         (save-preference! choice-path 'bar-height 44)
         (choose-preset! choice-path 'xp)
         (choose-preset! choice-path #f)
         (eqv? (preference-ref choice-path 'bar-height) 44)))

(if (zero? failures)
    (begin (display "ok presets\n") (exit 0))
    (begin (format (current-error-port) "~a preset check(s) failed~%" failures)
           (exit 1)))

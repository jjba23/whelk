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
(define xp-bar (car (preset-bars xp)))

(check "xp carries one bar"
       (= (length (preset-bars xp)) 1))
(check "xp is a bottom bar"
       (eq? (bar-config-edge xp-bar) 'bottom))
(check "xp is named xp"
       (eq? (bar-config-name xp-bar) 'xp))
(check "xp wears Luna blue"
       (and (equal? (bar-config-background xp-bar) "#245edb")
            (equal? (bar-config-foreground xp-bar) "#ffffff")
            (= (bar-config-radius xp-bar) 0)))
(check "xp lists every open window in its centre"
       ;; A taskbar, not a title: the centre is the task list, which focuses a
       ;; window on a click and closes it on a middle click.
       (equal? (kinds (bar-config-modules-center xp-bar)) '(tasks)))
(check "xp keeps the tray, the time and the battery on the right"
       (let ((right (kinds (bar-config-modules-right xp-bar))))
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

(if (zero? failures)
    (begin (display "ok presets\n") (exit 0))
    (begin (format (current-error-port) "~a preset check(s) failed~%" failures)
           (exit 1)))

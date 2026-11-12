;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The default bar: the heks-linux Waybar bar with the global menu, and the same
;; bar the shipped example configures.  The two are written out separately, so
;; the suite is what keeps them from drifting apart.

(use-modules (whelk defaults)
             (whelk modules)
             (whelk state)
             (whelk loader))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define (kinds modules)
  (map shell-module-kind modules))

(define (shape b)
  (list (bar-config-name b)
        (bar-config-edge b)
        (bar-config-height b)
        (kinds (bar-config-modules-left b))
        (kinds (bar-config-modules-center b))
        (kinds (bar-config-modules-right b))))

(define built-in (default-bar))

;; The reference's left half, plus the global menu after the window: the
;; shell's own menu (native, not a rofi script), whose window you are in, then
;; what can be done to it.
(check "the left half reads start, window, menu-bar"
       (equal? (kinds (bar-config-modules-left built-in))
               '(start window menu)))

;; The reference's right half, in its order: the media and desktop items, the
;; hardware readouts, the status icons, then search and the clock.
(check "the right half keeps the reference's order"
       (equal? (kinds (bar-config-modules-right built-in))
               '(custom custom custom custom language custom custom custom
                 custom custom cpu memory backlight audio keyboard battery
                 power-profile wifi bluetooth tray notifications custom clock)))

(check "the bar is a top bar"
       (and (eq? (bar-config-edge built-in) 'top)
            (= (bar-config-height built-in) 32)))

;; The example a new machine copies configures that very bar.
(define example (cadr (command-line)))

(check "the example file loads" (load-settings-file! example))

(define bars (get-shell-bars example))

(check "the example configures one bar" (= (length bars) 1))
(check "the example is the default bar" (equal? (shape (car bars))
                                                 (shape built-in)))

(if (zero? failures)
    (begin (display "ok defaults\n") (exit 0))
    (begin (format (current-error-port) "~a default check(s) failed~%" failures)
           (exit 1)))

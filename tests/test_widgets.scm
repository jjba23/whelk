;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; Desktop widgets are configuration before they are surfaces: a settings file
;; says which standard widget, where on which monitor, how large, and with
;; which options.  These checks pin that contract before the C side draws it.

(use-modules (whelk state)
             (whelk defaults)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define clock (desktop-widget 'clock #:x 20 #:y 30))

(check "a clock is a <widget-config>"
       (widget-config? clock))
(check "the clock keeps its placement and standard size"
       (and (eq? (widget-config-kind clock) 'clock)
            (= (widget-config-x clock) 20)
            (= (widget-config-y clock) 30)
            (= (widget-config-width clock) 220)
            (= (widget-config-height clock) 240)))

(check "a calendar has its own standard size"
       (let ((calendar (desktop-widget 'calendar)))
         (and (= (widget-config-width calendar) 340)
              (= (widget-config-height calendar) 330))))

(define weather
  (desktop-widget 'weather
                  #:x 10
                  #:y 50
                  #:width 400
                  #:height 300
                  #:monitor "DP-1"
                  #:options '((location . "Amsterdam")
                              (latitude . 52.37)
                              (longitude . 4.89))))

(check "placement, size and monitor can be overridden"
       (and (= (widget-config-x weather) 10)
            (= (widget-config-y weather) 50)
            (= (widget-config-width weather) 400)
            (= (widget-config-height weather) 300)
            (equal? (widget-config-monitor weather) "DP-1")))

(check "a widget keeps its own option alist"
       (and (equal? (assq-ref (widget-config-options weather) 'location)
                    "Amsterdam")
            (= (assq-ref (widget-config-options weather) 'latitude) 52.37)))

;; The C bridge receives the same shape bars receive: a kind and an option
;; alist, plus the geometry a floating surface needs.
(define alist (widget->alist weather))

(check "the bridge view carries kind, placement and options"
       (and (eq? (assq-ref alist 'kind) 'weather)
            (= (assq-ref alist 'x) 10)
            (= (assq-ref alist 'y) 50)
            (= (assq-ref alist 'width) 400)
            (= (assq-ref alist 'height) 300)
            (equal? (assq-ref alist 'monitor) "DP-1")
            (equal? (assq-ref (assq-ref alist 'options) 'location)
                    "Amsterdam")))

;; A malformed option is dropped rather than allowed to reach a factory that
;; would then have to guess what a non-pair means.
(check "malformed widget options are ignored"
       (let ((w (desktop-widget 'notes #:options '(("title" . "ok") broken))))
         (and (equal? (assq-ref (widget-config-options w) 'title) "ok")
              (not (assq-ref (widget-config-options w) 'broken)))))

(define standard (golden-gate-widgets))

(check "the standard set is the eight Golden Gate widgets"
       (equal? (map widget-config-kind standard)
               '(clock calendar weather battery notes reminders world-clock
                 stocks)))

(check "every standard widget has a distinct desktop position"
       (let ((positions (map (lambda (w)
                               (cons (widget-config-x w)
                                     (widget-config-y w)))
                             standard)))
         (and (= (length positions) (length (delete-duplicates positions)))
              (every (lambda (w)
                       (and (>= (widget-config-x w) 0)
                            (>= (widget-config-y w) 0)))
                     standard))))

;; Dragging snaps to the same grid, so the shipped layout starts on it: a
;; card that has never been moved has the position the first drag would pick.
(check "the standard layout sits on the eight-pixel drag grid"
       (every (lambda (w)
                (and (zero? (modulo (widget-config-x w) 8))
                     (zero? (modulo (widget-config-y w) 8))))
              standard))

;; Every card has the whole mouse vocabulary, not only the primary click the
;; first implementation wired.
(check "a widget keeps all of its mouse actions"
       (let ((w (desktop-widget
                 'notes
                 #:options '((on-click . "a")
                             (on-click-middle . "b")
                             (on-click-right . "c")
                             (on-scroll-up . "d")
                             (on-scroll-down . "e")))))
         (equal? (widget-config-options w)
                 '((on-click . "a")
                   (on-click-middle . "b")
                   (on-click-right . "c")
                   (on-scroll-up . "d")
                   (on-scroll-down . "e")))))

(define (widget-overlap? a b)
  (and (< (widget-config-x a)
          (+ (widget-config-x b) (widget-config-width b)))
       (< (widget-config-x b)
          (+ (widget-config-x a) (widget-config-width a)))
       (< (widget-config-y a)
          (+ (widget-config-y b) (widget-config-height b)))
       (< (widget-config-y b)
          (+ (widget-config-y a) (widget-config-height a)))))

(check "the standard widgets do not overlap"
       (let loop ((rest standard))
         (or (null? rest)
             (and (not (any (lambda (b) (widget-overlap? (car rest) b))
                            (cdr rest)))
                  (loop (cdr rest))))))

(check "the standard set is sized for a desktop, not a bar"
       (every (lambda (w)
                (and (>= (widget-config-width w) 200)
                     (>= (widget-config-height w) 160)))
              standard))

;; The shell record stores widgets beside bars, and the bridge can ask for all
;; of them without knowing what a widget record is.
(configure-shell! #:widgets (list clock weather))

(check "configure-shell! stores widgets"
       (equal? (get-shell-widgets) (list clock weather)))
(check "the bridge can read every configured widget"
       (equal? (map (lambda (a) (assq-ref a 'kind))
                    (shell-widgets->alists))
               '(clock weather)))

(exit (if (zero? failures) 0 1))

;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The monitor rule is configuration: a settings file must be able to say
;; "this bar on the laptop, that one on the external, and none on a one-screen
;; machine" without a special form, and the bridge must receive that rule as
;; plain values.

(use-modules (whelk state)
             (whelk defaults))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define laptop
  (bar #:name 'laptop
       #:monitor "eDP-1"
       #:monitors '("eDP-1")
       #:when 'multi-monitor))

(check "a bar carries a connector list"
       (equal? (bar-config-monitors laptop) '("eDP-1")))
(check "a bar's output-count rule is normalized"
       (eq? (bar-config-when laptop) 'multi))
(check "the monitor rule reaches the bridge"
       (let ((alist (bar->alist laptop)))
         (and (equal? (assq-ref alist 'monitors) '("eDP-1"))
              (eq? (assq-ref alist 'when) 'multi))))

(define external
  (bar #:name 'external
       #:monitors '("DP-1" "HDMI-A-1")
       #:when 'multi))

(check "several connectors are kept in order"
       (equal? (bar-config-monitors external) '("DP-1" "HDMI-A-1")))

(check "a single connector is accepted as a one-item list"
       (equal? (bar-config-monitors
                (bar #:monitors "DP-1"))
               '("DP-1")))

(check "unknown conditions fall back to always"
       (eq? (bar-config-when (bar #:when 'sometimes)) 'always))

;; The standard bar is unchanged: no connector list, always on.
(check "the default bar keeps its old rule"
       (let ((standard (default-bar)))
         (and (null? (bar-config-monitors standard))
              (eq? (bar-config-when standard) 'always))))

(exit (if (zero? failures) 0 1))

;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The frosted-glass finish is configuration before it is a skin: a tint, its
;; opacity, and the four numbers that shape the glass.  These checks pin the
;; constructor's normalisation, the alist the bridge reads, and the fact that
;; saving an unrelated GUI preference does not drop the finish.

(use-modules (whelk core)
             (whelk prefs)
             (whelk state)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define root (or (getenv "TMPDIR") "/tmp"))
(define state-dir
  (string-append root "/whelk-frost-test-"
                 (number->string (current-time))))

(mkdir state-dir)
(setenv "XDG_STATE_HOME" state-dir)
(setenv "HOME" state-dir)

(define default (frosted-glass))

(check "the finish is on, with no tint of its own"
       (and (eq? (assq-ref default 'enabled?) #t)
            (string=? (assq-ref default 'tint) "")))
(check "the default glass is a soft warm wash"
       (and (= (assq-ref default 'opacity) 0.62)
            (= (assq-ref default 'warmth) 0.35)
            (= (assq-ref default 'grain) 0.06)
            (= (assq-ref default 'highlight) 0.30)
            (= (assq-ref default 'shadow) 0.35)))

(define custom
  (frosted-glass #:tint "#1a1410"
                 #:opacity 0.55
                 #:warmth 0.5
                 #:grain 0.1
                 #:highlight 0.4
                 #:shadow 0.6))

(check "a tint and its opacity are kept"
       (and (string=? (assq-ref custom 'tint) "#1a1410")
            (= (assq-ref custom 'opacity) 0.55)))
(check "the glass numbers are kept"
       (and (= (assq-ref custom 'warmth) 0.5)
            (= (assq-ref custom 'grain) 0.1)
            (= (assq-ref custom 'highlight) 0.4)
            (= (assq-ref custom 'shadow) 0.6)))

(define off (frosted-glass #:enabled? #f))
(check "the finish can be turned off" (eq? (assq-ref off 'enabled?) #f))

(define typo (frosted-glass #:tint 'not-a-colour))
(check "a non-string tint leaves the surface colours alone"
       (string=? (assq-ref typo 'tint) ""))

;; The shape the C bridge reads.
(check "the bridge sees no finish before one is registered"
       (null? (shell-frost->alist)))

(define config-path (string-append state-dir "/settings.scm"))

(define (write-config!)
  (call-with-output-file config-path
    (lambda (port)
      (display "(configure-shell!\n" port)
      (display " #:bars (list (bar #:name 'main #:right (list (clock-module))))\n" port)
      (display " #:frost (frosted-glass #:tint \"#141210\" #:opacity 0.5\n" port)
      (display "                      #:warmth 0.2 #:grain 0.05))\n" port))))

(write-config!)
(load-shell-config! config-path)

(check "configure-shell! stores the finish"
       (let ((frost (get-shell-frost config-path)))
         (and (equal? (assq-ref frost 'tint) "#141210")
              (= (assq-ref frost 'opacity) 0.5)
              (= (assq-ref frost 'warmth) 0.2)
              (= (assq-ref frost 'grain) 0.05))))

(check "the bridge view carries the finish"
       (let ((view (shell-frost->alist config-path)))
         (and (equal? (assq-ref view 'tint) "#141210")
              (eq? (assq-ref view 'enabled?) #t))))

;; A GUI save rebuilds the shell record.  It must not forget the glass.
(check "a GUI preference keeps the finish"
       (and (save-preference! config-path 'bar-height 40)
            (begin
              (load-shell-config! config-path)
              (let ((frost (get-shell-frost config-path)))
                (and (equal? (assq-ref frost 'tint) "#141210")
                     (= (assq-ref frost 'opacity) 0.5))))))

(if (> failures 0)
    (begin
      (format (current-error-port) "~a frost test(s) failed~%" failures)
      (exit 1))
    (begin
      (display "frost: all checks passed\n")
      (exit 0)))

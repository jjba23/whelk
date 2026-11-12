;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; Diagnostics for the configuration engine, in the shape the C runtime logs
;; with: [LEVEL][whelk][Scheme:Subsystem] message.
;;
;; `simple-format' rather than (ice-9 format): it is already in the core module,
;; it understands the ~a and ~s used here, and it does not install a replacement
;; binding for `format' over the core one -- a replacement Guile warns about on
;; every settings-file load.

;;; Code:

(define-module (whelk logger)
  #:export (log-debug log-info log-warn log-error))

(define (log-prefix level subsystem)
  (string-append "[" (symbol->string level) "][whelk][Scheme:" subsystem "] "))

(define (log-write! level subsystem fmt args)
  (catch #t
         (lambda ()
           (display (string-append (log-prefix level subsystem)
                                   (apply simple-format #f fmt args)
                                   "\n")
                    (current-error-port))
           (force-output (current-error-port)))
         (lambda _
           ;; A malformed format string must not kill the shell: fall back to
           ;; printing the arguments plainly.
           (display (string-append (log-prefix level subsystem)
                                   (simple-format #f "~s ~s" fmt args)
                                   "\n")
                    (current-error-port))
           (force-output (current-error-port)))))

(define (log-debug subsystem fmt . args)
  "Print a debug diagnostic, when WHELK_DEBUG is set."
  (when (getenv "WHELK_DEBUG")
    (log-write! 'DEBUG subsystem fmt args)))

(define (log-info subsystem fmt . args)
  "Print an informational diagnostic."
  (log-write! 'INFO subsystem fmt args))

(define (log-warn subsystem fmt . args)
  "Print a warning diagnostic."
  (log-write! 'WARN subsystem fmt args))

(define (log-error subsystem fmt . args)
  "Print an error diagnostic."
  (log-write! 'ERROR subsystem fmt args))

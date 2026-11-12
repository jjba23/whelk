;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The bar themes: every preset is a complete palette, and a name the build does
;; not know leaves the bar alone instead of failing it.

(use-modules (whelk themes)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define names (themes))

(check "there are several themes" (>= (length names) 8))

;; Every theme says what a bar needs to be readable: a surface, ink, an accent.
(for-each
 (lambda (name)
   (let ((palette (theme-palette name)))
     (check (format #f "~a has a background" name)
            (assq-ref palette 'background))
     (check (format #f "~a has a foreground" name)
            (assq-ref palette 'foreground))
     (check (format #f "~a has an accent" name)
            (assq-ref palette 'accent))))
 names)

;; The entry point the bar uses.
(check "a known theme exists" (theme-exists? 'glass-2001))
(check "an unknown theme does not" (not (theme-exists? 'no-such-material)))
(check "no theme is no palette" (null? (theme-palette #f)))
(check "an unknown theme is no palette" (null? (theme-palette 'no-such-material)))

;; Names are materials and years, never somebody's product.
(for-each
 (lambda (name)
   (let ((text (symbol->string name)))
     (check (format #f "~a is a plain name" name)
            (every (lambda (ch) (or (char-alphabetic? ch)
                                    (char-numeric? ch)
                                    (char=? ch #\-)))
                   (string->list text)))))
 names)

(if (zero? failures)
    (begin (display "ok themes\n") (exit 0))
    (begin (format (current-error-port) "~a theme check(s) failed~%" failures)
           (exit 1)))

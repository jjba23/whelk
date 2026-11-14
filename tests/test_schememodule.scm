;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; A scheme-module is a procedure that becomes a bar module.  The procedure
;; itself cannot cross the C bridge; a token can, and the token has to resolve
;; to the procedure the current settings file made.

(use-modules (whelk modules))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define calls 0)

(define reading
  (scheme-module
   #:update (lambda ()
              (set! calls (+ calls 1))
              (string-append "reading " (number->string calls)))
   #:interval 2))

(check "scheme-module is a shell module"
       (and (shell-module? reading)
            (eq? (shell-module-kind reading) 'scheme)))
(check "its update is still a procedure in Scheme"
       (procedure? (module-option reading 'update)))

(define bridge (module->alist reading))
(define options (assq-ref bridge 'options))
(define token (assq-ref options 'update))

(check "the bridge view carries a token, not a procedure"
       (and token (symbol? token) (not (procedure? token))))
(check "the token resolves to the procedure"
       (equal? (call-module-procedure token) "reading 1"))
(check "the procedure keeps its own state between calls"
       (equal? (call-module-procedure token) "reading 2"))

;; A module without a procedure is still a value: the shell logs it and leaves
;; it out of the bar rather than aborting the settings file.
(define broken (scheme-module #:update "not a procedure"))
(check "a non-procedure update does not claim to be one"
       (not (procedure? (module-option broken 'update))))

;; Tokens belong to the loaded configuration.
(reset-module-procedures!)
(check "a reload invalidates the old token"
       (not (call-module-procedure token)))
(check "a new procedure gets a fresh token"
       (let* ((fresh (scheme-module #:update (lambda () "new")))
              (fresh-token (assq-ref (assq-ref (module->alist fresh) 'options)
                                     'update)))
         (and fresh-token
              (equal? (call-module-procedure fresh-token) "new"))))

(exit (if (zero? failures) 0 1))

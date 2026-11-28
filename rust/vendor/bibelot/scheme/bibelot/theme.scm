;; bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT

;;; Commentary:
;;
;; The desktop widgets' structural theme.  Rust asks this module for
;; `widget-theme', an alist of keys and values:
;;
;;   card-padding    space inside every card, in pixels
;;   card-gap        the gap between a card's header, body and footer
;;   extra-css       rules appended after the structural card stylesheet
;;
;; The palette and material are the host shell's skin, so a theme here only
;; has to describe the card's own rhythm.  A user file at
;; $XDG_CONFIG_HOME/bibelot/theme.scm (normally ~/.config/bibelot/theme.scm)
;; is evaluated inside this module, so it can call `configure-widgets!' without
;; importing anything:
;;
;;   (configure-widgets! #:card-padding 18
;;                       #:card-gap 12
;;                       #:extra-css ".whelk-widget-clock-time { font-size: 34px; }")
;;
;; Values that are not set keep the defaults below, and a file that raises is
;; reported and ignored rather than half-applied.

;;; Code:

(define-module (bibelot theme)
  ;; The user's file is evaluated into this module with `load'.
  #:declarative? #f
  #:export (configure-widgets!
            widget-theme
            reset-widget-theme!))

(define %default-theme
  (list
   (cons 'card-padding 14)
   (cons 'card-gap 9)
   (cons 'extra-css "")))

(define (copy-theme theme)
  "Return a shallow copy of THEME, so the defaults stay the defaults."
  (map (lambda (pair) (cons (car pair) (cdr pair))) theme))

(define %theme (copy-theme %default-theme))

(define (set-theme! key value)
  "Set KEY to VALUE in the current theme, adding a key the defaults lack."
  (let ((cell (assq key %theme)))
    (if cell
        (set-cdr! cell value)
        (set! %theme (acons key value %theme)))))

(define* (configure-widgets! #:key
                             card-padding
                             card-gap
                             extra-css)
  "Change the desktop widgets' structural theme.

Only the keys given are changed.  Sizes are pixels; EXTRA-CSS is appended
after the generated card stylesheet."
  (when card-padding (set-theme! 'card-padding card-padding))
  (when card-gap (set-theme! 'card-gap card-gap))
  (when extra-css (set-theme! 'extra-css extra-css))
  %theme)

(define (widget-theme)
  "Return the current widget theme as an alist of symbols and values."
  %theme)

(define (reset-widget-theme!)
  "Forget every `configure-widgets!' call and return the defaults."
  (set! %theme (copy-theme %default-theme))
  %theme)

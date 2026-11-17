;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; A preset is everything a shell can be switched to in one word: its bars, its
;; desktop widgets, its shell menu and its clipboard settings.  `use-preset!'
;; registers one through the same `configure-shell!' a settings file calls, so
;; a preset and a hand-written configuration are the same kind of thing.
;;
;; The presets themselves live in their own modules -- (whelk presets current)
;; is the shipped bar, (whelk presets xp) is a Luna-blue taskbar -- and register
;; themselves when loaded.  A new preset is a new module, not an edit here.

;;; Code:

(define-module (whelk presets)
  #:use-module (srfi srfi-1)
  #:use-module (srfi srfi-9)
  #:use-module (whelk logger)
  #:use-module (whelk state)
  #:export (preset
            preset?
            preset-name
            preset-description
            preset-bars
            preset-widgets
            preset-shell-menu
            preset-clipboard
            register-preset!
            preset-ref
            presets
            use-preset!))

(define-record-type <preset>
  (%make-preset name description bars widgets shell-menu clipboard)
  preset?
  (name preset-name)
  (description preset-description)
  (bars preset-bars)
  (widgets preset-widgets)
  (shell-menu preset-shell-menu)
  (clipboard preset-clipboard))

(define* (preset #:key name (description "") (bars '()) (widgets '())
                 (shell-menu #f) (clipboard '()))
  "Bundle BARS, WIDGETS, SHELL-MENU and CLIPBOARD under NAME.

NAME is the symbol `use-preset!' looks up.  Every part is what
`configure-shell!' takes, so a preset can be as small as one bar or as complete
as the shipped configuration; #f for SHELL-MENU keeps the default menu."
  (unless (symbol? name)
    (error "preset: NAME must be a symbol" name))

  (%make-preset name description bars widgets shell-menu clipboard))

(define *presets* (make-hash-table))

(define (register-preset! bundle)
  "Make BUNDLE available to `use-preset!' under its name."
  (unless (preset? bundle)
    (error "register-preset!: not a <preset>" bundle))

  (hash-set! *presets* (preset-name bundle) bundle)
  bundle)

(define (preset-ref name)
  "Return the preset called NAME, or #f."
  (and name (hash-ref *presets* name #f)))

(define (presets)
  "Return every registered preset name, sorted."
  (sort (hash-map->list (lambda (name _bundle) name) *presets*)
        (lambda (a b) (string<? (symbol->string a) (symbol->string b)))))

(define* (use-preset! name #:key (config-path 'default))
  "Register the preset NAME as the shell described by CONFIG-PATH.

Returns #t when the preset exists and was registered, #f after logging the
names this build has when it does not.  A settings file is one line:

  (use-preset! 'current)"
  (let ((bundle (preset-ref name)))
    (cond
      ((not bundle)
       (log-error "Presets" "Unknown preset ~s; known presets: ~s"
                  name (presets))
       #f)
      (else
       (configure-shell! #:config-path config-path
                         #:bars (preset-bars bundle)
                         #:widgets (preset-widgets bundle)
                         #:shell-menu (preset-shell-menu bundle)
                         #:clipboard (preset-clipboard bundle))
       (log-info "Presets" "Using ~s: ~a bar(s), ~a widget(s)"
                 name (length (preset-bars bundle))
                 (length (preset-widgets bundle)))
       #t))))

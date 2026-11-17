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
;;
;; A preset can also be chosen without editing the settings file, which is what
;; makes a bundle a theme rather than a template: `WHELK_PRESET' and
;; `whelk --preset NAME' both go through `choose-preset!', and the choice is
;; remembered as the `preset' GUI preference so the next start follows it.

;;; Code:

(define-module (whelk presets)
  #:use-module (srfi srfi-1)
  #:use-module (srfi srfi-9)
  #:use-module (whelk logger)
  #:use-module (whelk prefs)
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
            use-preset!
            with-options
            chosen-preset
            environment-preset
            choose-preset!
            apply-preset-choice!))

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

(define (option-keys plist)
  "Return the keywords of PLIST, a flat `#:key value' list."
  (let loop ((rest plist) (keys '()))
    (if (or (null? rest) (null? (cdr rest)))
        (reverse keys)
        (loop (cddr rest) (cons (car rest) keys)))))

(define (with-options defaults overrides)
  "Return DEFAULTS, a flat `#:key value' list, with OVERRIDES' values in place.

`(with-options (list #:height 32 #:radius 0) (list #:height 40))' is
`(#:height 40 #:radius 0)'.  A preset's constructor uses it to take the bar's
own keywords without repeating every default, which is what lets a preset be a
theme: `(xp-bar #:height 40)' is the taskbar one notch taller."
  (let ((given (option-keys overrides)))
    (append overrides
            (let loop ((rest defaults) (kept '()))
              (cond
                ((or (null? rest) (null? (cdr rest))) (reverse kept))
                ((memq (car rest) given) (loop (cddr rest) kept))
                (else (loop (cddr rest)
                            (cons (cadr rest) (cons (car rest) kept)))))))))

(define* (use-preset! name #:key (config-path 'default)
                      (bars #f) (widgets #f) (shell-menu #f) (clipboard #f))
  "Register the preset NAME as the shell described by CONFIG-PATH.

Returns #t when the preset exists and was registered, #f after logging the
names this build has when it does not.  A settings file is one line:

  (use-preset! 'current)

BARS, WIDGETS, SHELL-MENU and CLIPBOARD override the bundle's own and are the
reason a bundle can be a theme: a preset is a starting point, and one colour or
one taller bar is a keyword away rather than a fork of the preset module.

  (use-preset! 'xp #:bars (list (xp-bar #:height 40)))

#f means \"the preset's own\", so `#:widgets '()' is the way to say a preset
should not put anything on the desktop."
  (let ((bundle (preset-ref name)))
    (cond
      ((not bundle)
       (log-error "Presets" "Unknown preset ~s; known presets: ~s"
                  name (presets))
       #f)
      (else
       (let ((chosen-bars (if bars bars (preset-bars bundle)))
             (chosen-widgets (if widgets widgets (preset-widgets bundle)))
             (chosen-menu (if shell-menu shell-menu (preset-shell-menu bundle)))
             (chosen-clipboard
              (if clipboard clipboard (preset-clipboard bundle))))
         (configure-shell! #:config-path config-path
                           #:bars chosen-bars
                           #:widgets chosen-widgets
                           #:shell-menu chosen-menu
                           #:clipboard chosen-clipboard)
         (log-info "Presets" "Using ~s: ~a bar(s), ~a widget(s)~a"
                   name (length chosen-bars) (length chosen-widgets)
                   (if (or bars widgets shell-menu clipboard)
                       " (with overrides)"
                       ""))
         #t)))))

;; =============================================================================
;; Choosing a preset outside the settings file
;; =============================================================================
;;
;; Two things can ask for a preset, in this order:
;;
;;   1. WHELK_PRESET, or `whelk --preset NAME', for the run that asks for it;
;;   2. the choice saved as the `preset' GUI preference, which is what
;;      `whelk --preset NAME' remembers and what the next start follows.
;;
;; Both beat whatever the settings file registered, because a switcher that only
;; works on files without a `use-preset!' line is not a switcher.  Deleting the
;; choice -- `whelk --preset forget', or the settings window's reset -- hands
;; the shell back to the settings file.

(define %preset-preference 'preset)

;; The one word that means \"no preset\": `WHELK_PRESET=forget' ignores the saved
;; choice for one run, exactly as `--preset forget' clears it for good.
(define %forget-preset 'forget)

(define (chosen-preset config-path)
  "Return the preset remembered for CONFIG-PATH, or #f when the file decides.

A state file is user-editable, so a value that is not a symbol or a string is
ignored rather than turned into a preset name."
  (let ((value (preference-ref config-path %preset-preference)))
    (cond ((symbol? value) value)
          ((and (string? value) (not (string-null? value)))
           (string->symbol value))
          (else #f))))

(define (environment-preset)
  "Return the preset WHELK_PRESET asks for, or #f when it is unset or empty."
  (let ((value (getenv "WHELK_PRESET")))
    (and value
         (not (string-null? value))
         (string->symbol value))))

(define* (choose-preset! config-path name)
  "Remember NAME as CONFIG-PATH's preset; with NAME #f, forget the choice.

Returns #t when the choice is now what was asked for and #f when the name is not
one this build has.  The caller reloads the shell, which is what applies it."
  (cond
    ((not name)
     (forget-preference! config-path %preset-preference)
     (log-info "Presets" "Following the settings file at ~a again" config-path)
     #t)
    ((not (preset? (preset-ref name)))
     (log-error "Presets" "Unknown preset ~s; known presets: ~s"
                name (presets))
     #f)
    ((save-preference! config-path %preset-preference name)
     (log-info "Presets" "Remembering ~s for ~a" name config-path)
     #t)
    (else #f)))

(define (apply-preset-choice! config-path)
  "Register the preset CONFIG-PATH's shell should be, when one is chosen.

The loader calls this between the settings file and the GUI overrides: a
one-run request wins over the saved choice, the saved choice wins over the
file, and the GUI overrides are layered on top of whichever bars win here."
  (let ((forced (environment-preset)))
    (cond
      ((eq? forced %forget-preset) #f)
      (else
       (let ((name (or forced (chosen-preset config-path))))
         (and name (use-preset! name #:config-path config-path)))))))

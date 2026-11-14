;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; Bar materials, as data.  A theme is a palette: four colours, a corner
;; radius, and the five material numbers the generated skin turns into light:
;; opacity, shadow blur, shine, reflection and flare.  `(bar #:theme
;; 'glass-2001 ...)' applies one, and any value the bar sets itself wins over
;; the theme's, so a theme is a starting point rather than a wall.
;;
;; The old themes used to carry a hand-written gradient in `css'.  That made
;; the material controls impossible: the gradient painted over the opacity and
;; shine the settings window changed.  Their signature is now expressed by the
;; numbers, and `css' is only for rules the material numbers cannot say.
;;
;; The names are the eras, not the products: a year and a material describe the
;; same look without borrowing anybody's mark, and a bar in 2026 should not have
;; to spell a 2001 marketing name to get its pinstripes back.  The same goes for
;; the community palettes in the last block: "frost" is the material, and the
;; project that popularised it keeps its own name to itself.

;;; Code:

(define-module (whelk themes)
  #:use-module (whelk logger)
  #:use-module (srfi srfi-1)
  #:export (themes
            theme-exists?
            theme-palette))

;; A theme's fields, in the order a bar's own options shadow them.
(define *themes* (make-hash-table))

(define (register-theme! name palette)
  "Record PALETTE under NAME."
  (hash-set! *themes* name palette))

(define (theme-exists? name)
  "Is NAME a theme this build knows?"
  (and name (hash-ref *themes* name #f) #t))

(define (theme-palette name)
  "Return the palette of NAME, or '() when there is no such theme.

A settings file that names a theme this build does not have still gets a bar:
the bar simply keeps its own colours, and the name is reported once."
  (cond ((not name) '())
        ((hash-ref *themes* name #f) (hash-ref *themes* name '()))
        (else
         (log-warn "Themes" "Unknown bar theme ~s: using the bar's own colours"
                   name)
         '())))

(define (themes)
  "Return every theme name, sorted."
  (sort (hash-map->list (lambda (name _palette) name) *themes*)
        (lambda (a b) (string<? (symbol->string a) (symbol->string b)))))

;; =============================================================================
;; The eras
;; =============================================================================
;;
;; Each palette says what the bar is made of, era by era: the pale pinstriped
;; plastic of the early 2000s, the brushed alloy that followed, the black glass
;; of the late 2000s, the flat sheets of the 2010s, the dark translucency of the
;; 2020s, and the tinted glass that is the current fashion.

(register-theme!
 'glass-2001
 '((background . "#e8ebef")
   (foreground . "#1b1f24")
   (accent . "#2f6fd0")
   (border-color . "#b4bcc7")
   (radius . 6)
   (opacity . 1.0)
   (blur . 3)
   (shine . 0.55)
   (reflection . 0.18)
   (flare . 0.08)
   ;; The one thing the material numbers cannot say: this era named the shell
   ;; with the heaviest weight it had.
   (css . ".whelk-bar .whelk-start-button { font-weight: 700; }\n")))

(register-theme!
 'metal-2003
 '((background . "#c9cccb")
   (foreground . "#1a1a1a")
   (accent . "#5b6770")
   (border-color . "#8f969a")
   (radius . 0)
   (opacity . 1.0)
   (blur . 2)
   (shine . 0.40)
   (reflection . 0.12)
   (flare . 0.05)
   (css . "")))

(register-theme!
 'gloss-2007
 '((background . "rgba(26, 26, 28, 0.82)")
   (foreground . "#f4f4f6")
   (accent . "#3b9eff")
   (border-color . "rgba(0, 0, 0, 0.92)")
   (radius . 10)
   (opacity . 1.0)
   (blur . 18)
   (shine . 0.38)
   (reflection . 0.10)
   (flare . 0.12)
   ;; The late-2000s menu bar was a square slab; its popovers kept the radius.
   ;; The generated material paints the radius and the configuration's own
   ;; rule is deliberately last, so this one still wins on the bar alone.
   (css . ".whelk-bar { border-radius: 0; }\n")))

(register-theme!
 'flat-2014
 '((background . "#f2f2f2")
   (foreground . "#1f1f1f")
   (accent . "#0a84ff")
   (border-color . "#d0d0d0")
   (radius . 0)
   (opacity . 1.0)
   (blur . 0)
   (shine . 0.0)
   (reflection . 0.0)
   (flare . 0.0)
   (css . "")))

(register-theme!
 'dark-2020
 '((background . "#1e1e1e")
   (foreground . "#f0f0f0")
   (accent . "#4b9bff")
   (border-color . "#2e2e2e")
   (radius . 12)
   (opacity . 1.0)
   (blur . 12)
   (shine . 0.08)
   (reflection . 0.04)
   (flare . 0.05)
   (css . "")))

(register-theme!
 'tinted-2024
 '((background . "rgba(246, 243, 255, 0.86)")
   (foreground . "#221f2b")
   (accent . "#6b4bff")
   (border-color . "rgba(255, 255, 255, 0.55)")
   (radius . 16)
   (opacity . 1.0)
   (blur . 24)
   (shine . 0.26)
   (reflection . 0.14)
   (flare . 0.12)
   (css . "")))

;; =============================================================================
;; The palettes people rice with
;; =============================================================================
;;
;; Cool, warm, pastel, night, paper, and none at all.  They are the same shape
;; as the eras above: a background, a foreground, one accent, a hairline, and
;; the five numbers that make the material.

(register-theme!
 'frost
 '((background . "#2e3440")
   (foreground . "#eceff4")
   (accent . "#88c0d0")
   (border-color . "#3b4252")
   (radius . 8)
   (opacity . 1.0)
   (blur . 14)
   (shine . 0.08)
   (reflection . 0.05)
   (flare . 0.06)
   (css . "")))

(register-theme!
 'ember
 '((background . "#282828")
   (foreground . "#ebdbb2")
   (accent . "#fabd2f")
   (border-color . "#3c3836")
   (radius . 6)
   (opacity . 1.0)
   (blur . 10)
   (shine . 0.08)
   (reflection . 0.05)
   (flare . 0.05)
   (css . "")))

(register-theme!
 'mocha
 '((background . "#1e1e2e")
   (foreground . "#cdd6f4")
   (accent . "#cba6f7")
   (border-color . "#313244")
   (radius . 10)
   (opacity . 1.0)
   (blur . 16)
   (shine . 0.10)
   (reflection . 0.06)
   (flare . 0.07)
   (css . "")))

(register-theme!
 'midnight
 '((background . "#1a1b26")
   (foreground . "#c0caf5")
   (accent . "#7aa2f7")
   (border-color . "#292e42")
   (radius . 8)
   (opacity . 1.0)
   (blur . 14)
   (shine . 0.08)
   (reflection . 0.05)
   (flare . 0.06)
   (css . "")))

(register-theme!
 'sandstone
 '((background . "#fdf6e3")
   (foreground . "#586e75")
   (accent . "#268bd2")
   (border-color . "#eee8d5")
   (radius . 4)
   (opacity . 1.0)
   (blur . 4)
   (shine . 0.18)
   (reflection . 0.08)
   (flare . 0.04)
   (css . "")))

(register-theme!
 'void
 '((background . "#000000")
   (foreground . "#ffffff")
   (accent . "#ffffff")
   (border-color . "#1a1a1a")
   (radius . 0)
   (opacity . 1.0)
   (blur . 0)
   (shine . 0.0)
   (reflection . 0.0)
   (flare . 0.0)
   (css . "")))

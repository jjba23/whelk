;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; Where a configuration is kept.  One <shell-config> per settings file, held in
;; a table keyed by that file's path, so several settings files can describe
;; several shells (a test file, a laptop's bar, a desktop's) without sharing
;; state -- the same arrangement lambdock uses for its docks.
;;
;; A bar is geometry plus three module lists.  Geometry is who it is and where
;; it goes; the three lists are what it says.  A desktop widget is its own
;; record and its own C surface because it has a place and a size instead of a
;; slot in a row; the shell record holds both lists.

;;; Code:

(define-module (whelk state)
  #:use-module (srfi srfi-1)
  #:use-module (srfi srfi-9)
  #:use-module (whelk logger)
  #:use-module (whelk modules)
  #:use-module (whelk themes)
  #:export (<bar-config>
            bar-config?
            make-bar-config
            bar-config-name
            bar-config-theme
            bar-config-edge
            bar-config-layer
            bar-config-monitor
            bar-config-monitors
            bar-config-when
            bar-config-height
            bar-config-margin-top
            bar-config-margin-right
            bar-config-margin-bottom
            bar-config-margin-left
            bar-config-exclusive?
            bar-config-auto-hide?
            bar-config-auto-hide-delay
            bar-config-keyboard?
            bar-config-spacing
            bar-config-padding
            bar-config-font
            bar-config-font-size
            bar-config-icon-font
            bar-config-foreground
            bar-config-background
            bar-config-accent
            bar-config-border-color
            bar-config-radius
            bar-config-opacity
            bar-config-blur
            bar-config-shine
            bar-config-reflection
            bar-config-flare
            bar-config-launcher-width
            bar-config-launcher-height
            bar-config-launcher-icon-size
            bar-config-launcher-font-scale
            bar-config-css
            bar-config-modules-left
            bar-config-modules-center
            bar-config-modules-right
            bar-config-overflow-expanded?
            bar-config-modules-overflow
            <widget-config>
            widget-config?
            widget-config-kind
            widget-config-x
            widget-config-y
            widget-config-width
            widget-config-height
            widget-config-monitor
            widget-config-size
            widget-config-anchor
            widget-config-options
            desktop-widget
            widget->alist
            shell-widgets->alists
            bar
            clipboard-settings
            idle-settings
            night-light
            wallpaper-settings
            get-shell-clipboard
            get-shell-idle
            get-shell-night-light
            get-shell-wallpaper
            get-shell-frost
            frosted-glass
            configure-shell!
            normalize-edge
            normalize-layer
            get-shell-config
            bind-shell-config!
            forget-shell-config!
            get-shell-bars
            get-shell-menu
            get-shell-widgets
            shell-menu->units
            get-shell-enable-repl?
            get-shell-repl-socket-path
            bar->alist
            shell-bars->alists
            shell-clipboard->alist
            shell-wallpaper->alist
            shell-frost->alist))

;; =============================================================================
;; The bar record
;; =============================================================================

(define-record-type <bar-config>
  (make-bar-config name theme edge layer monitor monitors when height
                   margin-top margin-right margin-bottom margin-left
                   exclusive? auto-hide? auto-hide-delay keyboard? spacing padding
                   font font-size icon-font
                   foreground background accent border-color radius
                   opacity blur shine reflection flare css
                   launcher-width launcher-height launcher-icon-size
                   launcher-font-scale
                   modules-left modules-center modules-right
                   overflow-expanded? modules-overflow)
  bar-config?
  (name bar-config-name)
  (theme bar-config-theme)
  (edge bar-config-edge)
  (layer bar-config-layer)
  (monitor bar-config-monitor)
  (monitors bar-config-monitors)
  (when bar-config-when)
  (height bar-config-height)
  (margin-top bar-config-margin-top)
  (margin-right bar-config-margin-right)
  (margin-bottom bar-config-margin-bottom)
  (margin-left bar-config-margin-left)
  (exclusive? bar-config-exclusive?)
  (auto-hide? bar-config-auto-hide?)
  (auto-hide-delay bar-config-auto-hide-delay)
  (keyboard? bar-config-keyboard?)
  (spacing bar-config-spacing)
  (padding bar-config-padding)
  (font bar-config-font)
  (font-size bar-config-font-size)
  (icon-font bar-config-icon-font)
  (foreground bar-config-foreground)
  (background bar-config-background)
  (accent bar-config-accent)
  (border-color bar-config-border-color)
  (radius bar-config-radius)
  (opacity bar-config-opacity)
  (blur bar-config-blur)
  (shine bar-config-shine)
  (reflection bar-config-reflection)
  (flare bar-config-flare)
  (launcher-width bar-config-launcher-width)
  (launcher-height bar-config-launcher-height)
  (launcher-icon-size bar-config-launcher-icon-size)
  (launcher-font-scale bar-config-launcher-font-scale)
  (css bar-config-css)
  (modules-left bar-config-modules-left)
  (modules-center bar-config-modules-center)
  (modules-right bar-config-modules-right)
  (overflow-expanded? bar-config-overflow-expanded?)
  (modules-overflow bar-config-modules-overflow))

;; =============================================================================
;; The desktop widget record
;; =============================================================================
;; A widget is not a bar module: it has its own surface, its own size and a
;; place on the desktop instead of a slot in a row.  The record keeps that
;; placement and an option alist, so a new widget kind is a C factory and a new
;; alist key rather than another Scheme accessor.

(define-record-type <widget-config>
  (make-widget-config kind x y width height monitor size anchor options)
  widget-config?
  (kind widget-config-kind)
  (x widget-config-x)
  (y widget-config-y)
  (width widget-config-width)
  (height widget-config-height)
  (monitor widget-config-monitor)
  (size widget-config-size)
  (anchor widget-config-anchor)
  (options widget-config-options))

;; The sizes the standard collection uses, in gallery order: Clock, Calendar,
;; Weather, Batteries, Notes, Reminders, World Clock, Stocks, Now Playing,
;; Photos, Files, News and System.  A widget can be any size it likes; these
;; are the defaults a bare (desktop-widget ...) gets, not limits.
(define %widget-sizes
  '((clock 220 240)
    (calendar 340 330)
    (weather 340 220)
    (battery 240 180)
    (notes 340 260)
    (reminders 340 300)
    (world-clock 260 260)
    (stocks 340 280)
    (now-playing 340 220)
    (visualizer 360 220)
    (media 360 260)
    (photos 280 260)
    (files 320 260)
    (news 340 300)
    (system 280 280)))

;; The three card families: a card's family scales its kind's standard size, so a
;; settings file says "the big weather card" instead of writing pixels it then
;; has to maintain.  The arithmetic is the same as `family_size' in
;; rust/src/widgets.rs -- half-up rounding, snapped to the 8-pixel grid, never
;; below 100 pixels -- so a card is the same size whichever half of the shell
;; asks.
(define %widget-families '(small medium large))

(define (normalize-widget-family family)
  "Return FAMILY as a symbol, warning and falling back to 'medium."
  (let ((name (cond ((symbol? family) family)
                    ((string? family) (string->symbol family))
                    (else family))))
    (if (memq name %widget-families)
        name
        (begin
          (log-warn "State" "Widget size should be small, medium or large, got: ~s"
                    family)
          'medium))))

(define (widget-family-size size family)
  "Return SIZE, a (width . height) pair, scaled into FAMILY.

  'medium is the kind's own size, untouched; the other two land on the grid a
  drag snaps to, which is the arithmetic `family_size' in rust/src/widgets.rs
  performs."
  (define (snap value)
    (max 100 (* 8 (quotient (+ value 4) 8))))
  (define (scale value)
    (case family
      ((small) (quotient (+ (* value 3) 2) 4))
      ((large) (quotient (+ (* value 3) 1) 2))
      (else value)))
  (if (eq? family 'medium)
      (cons (max 100 (car size)) (max 100 (cdr size)))
      (cons (snap (scale (car size))) (snap (scale (cdr size))))))

(define %widget-anchors
  '(top-left top top-right left center right bottom-left bottom bottom-right))

(define (normalize-widget-anchor anchor)
  "Return ANCHOR as a symbol from %widget-anchors, warning and falling back."
  (let ((name (cond ((symbol? anchor) anchor)
                    ((string? anchor) (string->symbol anchor))
                    (else anchor))))
    (cond ((memq name %widget-anchors) name)
          ((eq? name 'centre) 'center)
          (else
           (log-warn "State"
                     "Widget anchor should be one of ~s, got: ~s"
                     %widget-anchors anchor)
           'top-left))))

(define (normalize-widget-kind kind)
  "Return KIND as a symbol, warning when it is not a name at all."
  (cond ((symbol? kind) kind)
        ((string? kind) (string->symbol kind))
        (else
         (log-warn "State" "Widget kind should be a symbol, got: ~s" kind)
         'custom)))

(define (normalize-widget-options options)
  "Return OPTIONS as a clean alist of symbol keys and non-#f values.

An option with a #f value means \"leave it to the widget's own default\", the
same convention (whelk modules) uses.  A malformed pair is dropped instead of
reaching C as a shape it cannot read."
  (define (key key)
    (cond ((symbol? key) key)
          ((string? key) (string->symbol key))
          ((keyword? key) (keyword->symbol key))
          (else #f)))
  (if (not (list? options))
      '()
      (filter-map
       (lambda (pair)
         (and (pair? pair)
              (key (car pair))
              (not (eq? (cdr pair) #f))
              (cons (key (car pair)) (cdr pair))))
       options)))

(define* (desktop-widget kind #:key
                         (x 48)
                         (y 64)
                         (width #f)
                         (height #f)
                         (monitor "default")
                         (size 'medium)
                         (anchor 'top-left)
                         (options '()))
  "Describe one floating desktop widget.

KIND is one of 'clock, 'calendar, 'weather, 'battery, 'notes, 'reminders,
'world-clock, 'stocks, 'now-playing, 'photos, 'files, 'news, 'system,
'custom, or a kind a later whelk adds.  X and Y are
the distance from ANCHOR's edges on MONITOR (\"default\" or a connector such as
\"DP-1\").  ANCHOR is where the card is pinned -- 'top-left (the default), 'top,
'top-right, 'left, 'center, 'right, 'bottom-left, 'bottom or 'bottom-right -- so
a card in a corner keeps that corner on a differently sized screen.  SIZE is
'small, 'medium or 'large and scales the kind's standard size (three size
families, on the same 8-pixel grid); WIDTH and HEIGHT, when given, win over it.
OPTIONS is
an alist of the widget's own keys: the weather takes latitude, longitude and
location, Notes takes text or file, Reminders takes items, and every widget
accepts title, css, tooltip and the mouse actions on-click, on-click-middle,
on-click-right, on-scroll-up and on-scroll-down.  The C side ignores a key it
does not know, exactly as a module does.  Where a card sits is only the start:
the grip in its header drags it, and the position is saved as runtime state
rather than written back into this file."
  (let* ((kind (normalize-widget-kind kind))
         (family (normalize-widget-family size))
         (preset (or (assq kind %widget-sizes) '(default 320 220)))
         (scaled (widget-family-size (cons (cadr preset) (caddr preset)) family))
         (preset-width (car scaled))
         (preset-height (cdr scaled)))
    (make-widget-config kind
                        (max 0 (as-int x 48))
                        (max 0 (as-int y 64))
                        (max 100 (as-int width preset-width))
                        (max 100 (as-int height preset-height))
                        (cond ((symbol? monitor) (symbol->string monitor))
                              ((string? monitor) monitor)
                              (else "default"))
                        family
                        (normalize-widget-anchor anchor)
                        (normalize-widget-options options))))

;; =============================================================================
;; The shell record
;; =============================================================================

;; The shell menu is the popover behind the mark at the far left of the bar:
;; `(whelk menu)' units, or #f for "the default the shell ships".
(define-record-type <shell-config>
  (make-shell-config bars widgets shell-menu clipboard idle night-light wallpaper
                     frost enable-repl? repl-socket-path)
  shell-config?
  (bars shell-config-bars)
  (widgets shell-config-widgets)
  (shell-menu shell-config-shell-menu)
  (clipboard shell-config-clipboard)
  (idle shell-config-idle)
  (night-light shell-config-night-light)
  (wallpaper shell-config-wallpaper)
  (frost shell-config-frost)
  (enable-repl? shell-config-enable-repl?)
  (repl-socket-path shell-config-repl-socket-path))

(define %default-shell-config%
  (make-shell-config '() '() #f '() '() '() '() '() #f "/tmp/whelk-repl.sock"))

;; Keyed by settings file path; 'default is the shell with no settings file.
(define *shell-configs* (make-hash-table))

;; =============================================================================
;; Normalisation
;; =============================================================================

(define (normalize-edge position)
  "Return POSITION as one of the symbols 'top, 'bottom, 'left or 'right.

A Waybar configuration spells the same thing with strings, so a string is
accepted too; anything else falls back to 'top and says so rather than putting a
bar on an edge the compositor cannot place."
  (let* ((value (cond ((symbol? position) position)
                      ((string? position) (string->symbol position))
                      (else #f)))
         (known (and value (memq value '(top bottom left right)))))
    (if known
        value
        (begin
          (log-warn "State" "Unknown bar position ~s: using 'top" position)
          'top))))

(define (normalize-layer layer)
  "Return LAYER as one of the symbols 'background, 'bottom, 'top or 'overlay.

On niri, background and bottom surfaces move and zoom with the workspace
during the overview; top and overlay surfaces stay fixed over it.  Anything
unknown falls back to 'top, the layer a bar normally wants."
  (let* ((value (cond ((symbol? layer) layer)
                      ((string? layer) (string->symbol layer))
                      (else #f)))
         (known (and value (memq value '(background bottom top overlay)))))
    (if known
        value
        (begin
          (log-warn "State" "Unknown bar layer ~s: using 'top" layer)
          'top))))

(define (normalize-margins margin)
  "Expand MARGIN -- a number or a list of 1, 2 or 4 numbers -- to four values,
in CSS order (top right bottom left)."
  (define (as-int value) (if (number? value) (inexact->exact (round value)) 0))
  (if (list? margin)
      (case (length margin)
        ((0) (list 0 0 0 0))
        ((1) (let ((a (as-int (car margin)))) (list a a a a)))
        ((2) (let ((v (as-int (car margin))) (h (as-int (cadr margin))))
               (list v h v h)))
        ((3) (let ((t (as-int (car margin)))
                   (h (as-int (cadr margin)))
                   (b (as-int (caddr margin))))
               (list t h b h)))
        (else (map as-int (take margin 4))))
      (let ((a (as-int margin))) (list a a a a))))

(define (as-int value default)
  (if (number? value) (inexact->exact (round value)) default))

;; =============================================================================
;; The constructor a settings file calls
;; =============================================================================

(define (normalize-monitors monitors)
  "Return MONITORS as a list of connector names.

A settings file may write one connector, a list of them, or \"all\"; each
becomes the same list so the C side has one shape to evaluate."
  (define (name value)
    (cond ((string? value) value)
          ((symbol? value) (symbol->string value))
          (else #f)))
  (cond ((list? monitors)
         (filter-map name monitors))
        ((or (string? monitors) (symbol? monitors))
         (let ((value (name monitors)))
           (if (and value (not (string-null? value))) (list value) '())))
        (else '())))

(define (normalize-when condition)
  "Return CONDITION as 'always, 'single or 'multi."
  (let* ((value (cond ((symbol? condition) condition)
                      ((string? condition) (string->symbol condition))
                      (else #f)))
         (known (and value
                     (memq value '(always single single-monitor multi
                                   multi-monitor)))))
    (cond ((not known)
           (if value
               (log-warn "State"
                         "Unknown bar monitor condition ~s: using 'always"
                         condition))
           'always)
          ((memq value '(single single-monitor)) 'single)
          ((memq value '(multi multi-monitor)) 'multi)
          (else 'always))))

(define* (bar #:key
              (name 'main)
              (position 'top)
              (layer 'top)
              (monitor "all")
              (monitors '())
              (when 'always)
              (height 32)
              (margin 0)
              (exclusive? #t)
              (auto-hide? #f)
              (auto-hide-delay 900)
              (keyboard? #f)
              (spacing 6)
              (padding 12)
              (font #f)
              (font-size #f)
              (icon-font #f)
              (theme #f)
              (foreground #f)
              (background #f)
              (accent #f)
              (border-color #f)
              (radius #f)
              (opacity #f)
              (blur #f)
              (shine #f)
              (reflection #f)
              (flare #f)
              (css #f)
              (launcher-width 660)
              (launcher-height 640)
              (launcher-icon-size 24)
              (launcher-font-scale 100)
              (left '())
              (center '())
              (right '())
              (overflow '())
              (overflow-expanded? #f))
  "Describe one bar.

POSITION is the edge it is anchored to ('top, 'bottom, 'left, 'right).  HEIGHT
is its thickness -- the row's height on the top and bottom edges, its column's
width on the left and right ones.  MONITOR is \"all\", \"default\" or a connector
name such as \"DP-1\".  MONITORS is a list of connector names: the bar appears
on each of them and on no other output.  WHEN is the output-count rule --
'always, 'single or 'multi -- so a laptop bar can be hidden the moment an
external screen is plugged in, and an external bar can exist only while there
is more than one screen.  MARGIN is a number or a CSS-style list (top right
bottom left) of pixels between the bar and the screen edge.  EXCLUSIVE? reserves the
bar's thickness so maximised windows stop at it, which is what makes the bar a
bar rather than an overlay.  LEFT, CENTER and RIGHT are lists of modules from
(whelk modules).

LAYER is the layer-shell layer: 'top (the default), 'bottom, 'background or
'overlay.  On niri the overview keeps top and overlay surfaces fixed; bottom
and background surfaces move and zoom with the workspace.  A bottom bar is
still visible in the compositor's reserved edge, but its popovers can sit below
ordinary windows, so 'top is the safer default.

The four colours and the fonts are #f by default, which means \"use the
structural stylesheet's colour\": a machine that sets nothing still gets a
readable bar.

OPACITY multiplies the background colour's own alpha, so a theme that was
designed as 82% glass stays 82% at 1.0 and becomes lighter at 0.7.  BLUR is the
soft outer shadow radius in pixels.  SHINE, REFLECTION and FLARE are 0..1
strengths for the top highlight, the lower inner reflection and the accent
glow along the top edge.  A theme supplies these starting points; a keyword on
the bar overrides one without touching the others.

LAUNCHER-WIDTH, LAUNCHER-HEIGHT, LAUNCHER-ICON-SIZE and LAUNCHER-FONT-SCALE
are the launcher's starting geometry: pixels for the card and its icons, and a
percentage of the bar's font size for its text.  They are defaults; dragging
the launcher's grip or using its Icons and Text buttons writes overrides, and
the Launcher page edits the same four keys.

OVERFLOW is a list of modules kept behind a chevron at the end of the right
row.  The area is laid out horizontally only while it is open, and
OVERFLOW-EXPANDED? decides whether it starts open; the chevron toggles it after
that.  The modules are built with the bar, so their readings keep updating and
their clicks keep working while the area is closed."
  (let* ((margins (normalize-margins margin))
         (palette (theme-palette theme))
         ;; A theme is a starting point: whatever the bar sets itself wins.
         (from-theme (lambda (key explicit)
                       (if explicit explicit (assq-ref palette key))))
         (as-unit (lambda (value fallback)
                    (if (number? value)
                        (min 1.0 (max 0.0 (exact->inexact value)))
                        fallback)))
         (as-double (lambda (value fallback)
                      (if (number? value) (exact->inexact value) fallback))))
    ;; `when' is also this function's keyword argument, so the conditional is
    ;; spelled with `if' rather than shadowing the syntax form.
    (if (and (not (list? left)) (not (list? center)) (not (list? right))
             (not (list? overflow)))
        (log-warn "State" "Bar ~a: module lists should be lists" name))
    (make-bar-config name
                     (cond ((symbol? theme) theme)
                           ((string? theme) (string->symbol theme))
                           (else #f))
                     (normalize-edge position)
                     (normalize-layer layer)
                     (cond ((symbol? monitor) (symbol->string monitor))
                           ((string? monitor) monitor)
                           (else "all"))
                     (normalize-monitors monitors)
                     (normalize-when when)
                     (as-int height 32)
                     (list-ref margins 0)
                     (list-ref margins 1)
                     (list-ref margins 2)
                     (list-ref margins 3)
                     (if exclusive? #t #f)
                     (if auto-hide? #t #f)
                     (as-int auto-hide-delay 900)
                     (if keyboard? #t #f)
                     (as-int spacing 6)
                     (as-int padding 12)
                     font
                     font-size
                     icon-font
                     (from-theme 'foreground foreground)
                     (from-theme 'background background)
                     (from-theme 'accent accent)
                     (from-theme 'border-color border-color)
                     (as-int (from-theme 'radius radius) 0)
                     (as-unit (from-theme 'opacity opacity) 1.0)
                     (as-int (from-theme 'blur blur) 0)
                     (as-unit (from-theme 'shine shine) 0.0)
                     (as-unit (from-theme 'reflection reflection) 0.0)
                     (as-unit (from-theme 'flare flare) 0.0)
                     (from-theme 'css css)
                     (as-int launcher-width 660)
                     (as-int launcher-height 640)
                     (as-int launcher-icon-size 24)
                     (as-int launcher-font-scale 100)
                     (if (list? left) left '())
                     (if (list? center) center '())
                     (if (list? right) right '())
                     (if overflow-expanded? #t #f)
                     (if (list? overflow) overflow '()))))

;; =============================================================================
;; Configuration entry point
;; =============================================================================

(define* (clipboard-settings #:key
                             (persist #f)
                             (persist-only-pinned? #f)
                             (clear-on-lock? #f)
                             (max-entries 50)
                             (max-entry-bytes (* 32 1024 1024))
                             (max-total-bytes (* 128 1024 1024))
                             (ttl-seconds 0))
  "Describe the clipboard history's privacy and memory policy.

PERSIST is #f (memory only), #t (keep the history under the user's state
directory), or a file path.  PERSIST-ONLY-PINNED? makes persistence a vault
for copies pinned with Ctrl+P rather than a transcript of everything copied.
CLEAR-ON-LOCK? watches logind's lock state and clears unpinned copies when the
session locks; pinned copies survive.
MAX-ENTRIES and MAX-TOTAL-BYTES bound memory; unpinned entries are evicted
oldest first.  TTL-SECONDS, when positive, expires unpinned entries."
  (list (cons 'configured? #t)
        (cons 'persist (cond ((string? persist) persist)
                             (persist "auto")
                             (else #f)))
        (cons 'persist-only-pinned? (if persist-only-pinned? #t #f))
        (cons 'clear-on-lock? (if clear-on-lock? #t #f))
        (cons 'max-entries max-entries)
        (cons 'max-entry-bytes max-entry-bytes)
        (cons 'max-total-bytes max-total-bytes)
        (cons 'ttl-seconds ttl-seconds)))

(define* (frosted-glass #:key
                        (enabled? #t)
                        (tint "")
                        (opacity 0.62)
                        (warmth 0.35)
                        (grain 0.06)
                        (highlight 0.30)
                        (shadow 0.35))
  "Describe the frosted-glass finish every whelk surface shares.

The blur itself is the compositor's work: a Wayland surface cannot blur what
is behind it, so niri, Hyprland or SwayFX needs a layer rule for the
`whelk-rust' namespace (see the materials page of the manual).  What whelk can
paint is the glass: TINT laid over each surface's own background at OPACITY,
two warm gradients whose strength is WARMTH, a grain of GRAIN that hides the
banding a blurred backdrop shows, a hairline HIGHLIGHT along the top edge and a
soft drop SHADOW under floating panes.  An empty TINT keeps every surface's
configured colour and adds only the finish.

All the numbers are 0.0 to 1.0; a value outside that range is clamped.  #f for
ENABLED? turns the whole finish off, which is what a settings file that never
mentions `#:frost' gets."
  (list (cons 'enabled? (if enabled? #t #f))
        (cons 'tint (if (string? tint) tint ""))
        (cons 'opacity opacity)
        (cons 'warmth warmth)
        (cons 'grain grain)
        (cons 'highlight highlight)
        (cons 'shadow shadow)))

(define (clock->minutes text fallback)
  "Return \"HH:MM\" as minutes from midnight, or FALLBACK when it is not one."
  (if (and (string? text)
           (= (string-length text) 5)
           (char=? (string-ref text 2) #\:)
           (every char-numeric? (list (string-ref text 0)
                                      (string-ref text 1)
                                      (string-ref text 3)
                                      (string-ref text 4))))
      (let ((hour (string->number (substring text 0 2)))
            (minute (string->number (substring text 3 5))))
        (if (and (< hour 24) (< minute 60))
            (+ (* hour 60) minute)
            (begin
              (log-warn "State" "Unknown clock ~s: using ~a" text fallback)
              fallback)))
      (begin
        (log-warn "State" "Unknown clock ~s: using ~a" text fallback)
        fallback)))

(define (night-light-mode value)
  "Return VALUE as 'schedule or 'always, warning about anything else."
  (let ((mode (cond ((symbol? value) value)
                    ((string? value) (string->symbol value))
                    (else #f))))
    (if (memq mode '(schedule always))
        mode
        (begin
          (log-warn "State" "Unknown night-light mode ~s: using 'schedule" value)
          'schedule))))

(define* (night-light #:key
                      (enabled? #f)
                      (mode 'schedule)
                      (temperature 3400)
                      (start "19:00")
                      (end "07:00")
                      (transition 30))
  "Describe whelk's night light.

ENABLED? turns the filter on.  TEMPERATURE is the white point in kelvin; 6500 is
neutral, and the useful range is 1000 to 6500, where lower is warmer.  MODE is
'schedule (the default) or 'always.  START and END are \"HH:MM\" clocks: the
window may cross midnight, and a window whose two ends are the same minute is
read as all day.  TRANSITION is how many minutes each fade takes; a fade that
would not fit in the window is clamped to half of it, so the two ends meet in
the middle rather than overlapping.

The filter is applied by the display server, not drawn over the desktop:
wlr-gamma-control-unstable-v1 on a Wayland compositor that offers it (niri,
sway, Hyprland, river), or the RandR gamma table on X11.  A compositor that
keeps its gamma to itself is reported in the log; whelk does not pretend an
overlay is the same thing."
  (list (cons 'configured? #t)
        (cons 'enabled? (if enabled? #t #f))
        (cons 'mode (night-light-mode mode))
        (cons 'temperature
              (if (number? temperature)
                  (inexact->exact (max 1000 (min 6500 (round temperature))))
                  3400))
        (cons 'start (clock->minutes start (* 19 60)))
        (cons 'end (clock->minutes end (* 7 60)))
        (cons 'transition
              (if (number? transition)
                  (inexact->exact (max 0 (min 1440 (round transition))))
                  30))))

(define* (idle-settings #:key (lock-after 0))
  "Describe whelk's idle and inhibit policy.

LOCK-AFTER is the number of seconds logind must report the session idle before
whelk locks it.  Zero is the compatible default: lock on the first idle hint,
which is what the shell did before it had an idle manager.  A negative value
turns timed locking off, leaving the explicit lock paths (logind's Lock signal,
a VT switch, suspend and `whelk --lock') in charge.

`whelk --inhibit' takes a real logind idle inhibitor for as long as it is on;
do-not-disturb is a separate flag and never pauses this timer."
  (list (cons 'configured? #t)
        (cons 'lock-after (if (number? lock-after)
                              (inexact->exact (round lock-after))
                              0))))

;; =============================================================================
;; The wallpaper
;; =============================================================================
;; A wallpaper is a file, a collection of files, a clock and a transition.  The
;; constructor below normalises all four into plain values: paths are expanded
;; (`~/Pictures' is how a person writes a home directory), the animation and
;; the fit are symbols from a known list, and the numbers are clamped to what
;; the C side can use.  The engine treats an unknown symbol as the documented
;; default rather than as a broken shell.

(define (expand-home-path value)
  "Return VALUE as a path with a leading ~/ expanded, or #f."
  (let ((path (cond ((string? value) value)
                    ((symbol? value) (symbol->string value))
                    (else #f)))
        (home (getenv "HOME")))
    (cond ((not path) #f)
          ((and home
                (not (string-null? home))
                (>= (string-length path) 2)
                (string=? (substring path 0 2) "~/"))
           (string-append home (substring path 1)))
          (else path))))

(define (wallpaper-path-list paths)
  "Return PATHS as a list of non-empty, home-expanded file names."
  (define (keep value)
    (let ((path (expand-home-path value)))
      (and path (not (string-null? path)) path)))
  (cond ((list? paths) (filter-map keep paths))
        ((or (string? paths) (symbol? paths))
         (let ((path (keep paths)))
           (if path (list path) '())))
        (else '())))

(define (wallpaper-integer value fallback minimum maximum)
  (max minimum (min maximum (as-int value fallback))))

(define %wallpaper-animations
  '(none crossfade fade slide-left slide-right slide-up slide-down zoom))

(define (wallpaper-animation value)
  "Return VALUE as a known transition symbol, warning when it is not one."
  (let ((animation (cond ((symbol? value) value)
                         ((string? value) (string->symbol value))
                         (else #f))))
    (if (memq animation %wallpaper-animations)
        animation
        (begin
          (log-warn "State"
                    "Unknown wallpaper animation ~s: using 'crossfade" value)
          'crossfade))))

(define (wallpaper-fit value)
  "Return VALUE as 'cover, 'contain or 'fill, warning otherwise."
  (let ((fit (cond ((symbol? value) value)
                   ((string? value) (string->symbol value))
                   (else #f))))
    (if (memq fit '(cover contain fill))
        fit
        (begin
          (log-warn "State" "Unknown wallpaper fit ~s: using 'cover" value)
          'cover))))

(define* (wallpaper-settings #:key
                             (enabled? #f)
                             (paths '())
                             (interval 900)
                             (shuffle? #t)
                             (animation 'crossfade)
                             (animation-duration 1200)
                             (fit 'cover)
                             (ken-burns? #f)
                             (video? #t)
                             (mute-video? #t)
                             (current #f)
                             (picker-width 960)
                             (picker-height 640)
                             (picker-columns 4))
  "Describe whelk's own wallpaper engine.

ENABLED? lets whelk draw the background.  PATHS is a directory, a file, or a
list of them: directories are walked (skipping hidden entries and directory
symlinks), and a file is taken as it is.  Supported images are PNG, JPEG,
WebP, AVIF, GIF, BMP, TIFF and SVG; supported videos are MP4, WebM, Matroska,
MOV, M4V, Ogg, AVI and MPEG, played through GTK's media backend.  A `~/' at
the start of a path is your home directory.

INTERVAL is the seconds between automatic changes; 900 is fifteen minutes, and
0 stops the clock.  SHUFFLE? visits the collection in a random order instead of
its sorted one.  A manual choice through the picker resets the countdown.

ANIMATION is the transition between two pictures: 'none, 'crossfade (the
default), 'fade (through black), 'slide-left, 'slide-right, 'slide-up,
'slide-down or 'zoom.  ANIMATION-DURATION is its length in milliseconds.
FIT is 'cover (fill the output and crop), 'contain (show the whole picture
with black bars) or 'fill (stretch).  KEN-BURNS? slowly zooms a still picture.
VIDEO? includes movies in the collection; MUTE-VIDEO? keeps their sound off.

CURRENT is the file to start on.  The picker saves the picture you choose as a
GUI preference, and that choice wins over CURRENT on the next start.

PICKER-WIDTH, PICKER-HEIGHT and PICKER-COLUMNS are the picker card's compact
size and its thumbnails per row; the card expands to the output from there.

#+begin_src scheme
  (wallpaper-settings #:enabled? #t
                      #:paths (list \"~/Pictures/Wallpapers\"
                                    \"~/Videos/walls\")
                      #:interval 900
                      #:animation 'crossfade
                      #:animation-duration 1600
                      #:fit 'cover)
#+end_src"
  (let ((start (and current (expand-home-path current))))
    (list (cons 'configured? #t)
          (cons 'enabled? (if enabled? #t #f))
          (cons 'paths (wallpaper-path-list paths))
          (cons 'interval (wallpaper-integer interval 900 0 604800))
          (cons 'shuffle? (if shuffle? #t #f))
          (cons 'animation (wallpaper-animation animation))
          (cons 'animation-duration
                (wallpaper-integer animation-duration 1200 0 10000))
          (cons 'fit (wallpaper-fit fit))
          (cons 'ken-burns? (if ken-burns? #t #f))
          (cons 'video? (if video? #t #f))
          (cons 'mute-video? (if mute-video? #t #f))
          (cons 'current (and start (not (string-null? start)) start))
          (cons 'picker-width
                (wallpaper-integer picker-width 960 320 3840))
          (cons 'picker-height
                (wallpaper-integer picker-height 640 240 2160))
          (cons 'picker-columns
                (wallpaper-integer picker-columns 4 1 12)))))

(define* (configure-shell! #:key
                           (config-path 'default)
                           (bars '())
                           (widgets '())
                           (shell-menu #f)
                           (clipboard '())
                           (idle '())
                           (night-light '())
                           (wallpaper '())
                           (frost '())
                           (enable-repl? #f)
                           (repl-socket-path "/tmp/whelk-repl.sock"))
  "Register BARS as the shell described by settings file CONFIG-PATH.

SHELL-MENU is the popover behind the mark at the far left of the bar, as
`(whelk menu)' units -- the shell's own equivalent of a menu bar's first menu.
Left #f it is `default-shell-menu' from `(whelk defaults)': about this system,
the ways into it, recent items, force quit, and the ways out of the session,
followed by the launcher and the reload and quit a configuration needs.  The
network, Bluetooth and notification panels are modules and CLI commands, not
session-menu entries.

WIDGETS is a list of `(desktop-widget ...)' records.  They become floating
layer surfaces on the desktop, rebuilt with the bars on every reload.

CLIPBOARD is `(clipboard-settings)' or an alist with the same keys: it sets the
history's persistence, caps and retention.  The empty default keeps the
environment-variable behaviour.

IDLE is `(idle-settings)' or an alist with the same keys: how long the session
may be idle before whelk locks it.  The empty default is the compatible
zero-second policy, which locks on the first idle hint.

NIGHT-LIGHT is `(night-light)' or an alist with the same keys: the white point,
the clock window that applies it and the fade at each end.  The empty default
is an off filter with 3400 K from 19:00 to 07:00, so a configuration that never
mentions it leaves the screen alone.

WALLPAPER is `(wallpaper-settings)' or an alist with the same keys: whelk's
own background, its collection, rotation and transition.  The empty default
leaves the desktop's wallpaper alone; the picker still opens and lists what the
configuration names.

ENABLE-REPL? starts a Guile REPL on a Unix socket when the shell is up, which is
how a configuration is changed without restarting the compositor:
`echo '(load-shell-config! ...)' | socat - UNIX-CONNECT:/tmp/whelk-repl.sock'."
  (let* ((valid (filter bar-config? (if (list? bars) bars '())))
         (valid-widgets
          (filter widget-config? (if (list? widgets) widgets '())))
         (cfg (make-shell-config valid valid-widgets shell-menu
                                 (if (list? clipboard) clipboard '())
                                 (if (list? idle) idle '())
                                 (if (list? night-light) night-light '())
                                 (if (list? wallpaper) wallpaper '())
                                 (if (list? frost) frost '())
                                 enable-repl?
                                 repl-socket-path)))
    (when (and (list? bars) (not (= (length valid) (length bars))))
      (log-warn "State"
                "~a of ~a bars were not <bar-config> records: they are ignored"
                (- (length bars) (length valid)) (length bars)))

    (when (and (list? widgets)
               (not (= (length valid-widgets) (length widgets))))
      (log-warn "State"
                "~a of ~a widgets were not <widget-config> records: they are ignored"
                (- (length widgets) (length valid-widgets)) (length widgets)))

    (hash-set! *shell-configs* config-path cfg)

    ;; The first settings file to load is also the fallback for anything that
    ;; asks for a configuration without naming one.
    (when (or (eq? config-path 'default)
              (not (hash-ref *shell-configs* 'default #f)))
      (hash-set! *shell-configs* 'default cfg))

    (log-info "State"
              "Registered [~a]: ~a bar(s) ~a, ~a widget(s), repl: ~a"
              config-path
              (length valid)
              (map (lambda (b)
                     (simple-format #f "~a(~a, ~a modules)"
                             (bar-config-name b)
                             (bar-config-edge b)
                             (+ (length (bar-config-modules-left b))
                                (length (bar-config-modules-center b))
                                (length (bar-config-modules-right b))
                                (length (bar-config-modules-overflow b)))))
                   valid)
              (length valid-widgets)
              enable-repl?)))

(define (get-shell-config . args)
  "Return the <shell-config> for ARGS (a settings path), or the default one."
  (let* ((key (if (null? args) 'default (car args)))
         (cfg (or (hash-ref *shell-configs* key #f)
                  (hash-ref *shell-configs* 'default #f))))
    (or cfg %default-shell-config%)))

(define (forget-shell-config! config-path)
  "Drop CONFIG-PATH's registration, if it has one.

The loader calls this before evaluating a settings file: a reload must not
leave the previous run's bars under the path when the new file registers only
the default configuration."
  (when (and config-path
             (not (eq? config-path 'default))
             (hash-ref *shell-configs* config-path #f))
    (hash-remove! *shell-configs* config-path)))

(define (bind-shell-config! config-path)
  "Give CONFIG-PATH its own copy of the default configuration when it has none.

Most settings files call `configure-shell!' without a path, which registers
the default configuration.  Binding that configuration under the load path is
what lets a reload replace the bars the GUI preference layer registered, rather
than layering on top of the previous run."
  (when (and config-path
             (not (eq? config-path 'default))
             (not (hash-ref *shell-configs* config-path #f)))
    (let ((cfg (hash-ref *shell-configs* 'default #f)))
      (when cfg
        (hash-set! *shell-configs* config-path cfg)))))

(define (get-shell-bars . args)
  "Return the configured bars, as a list of <bar-config> records."
  (shell-config-bars (apply get-shell-config args)))

(define (get-shell-menu . args)
  "Return the configured shell menu units, or #f for the default one."
  (shell-config-shell-menu (apply get-shell-config args)))

(define (get-shell-widgets . args)
  "Return the configured desktop widgets, as <widget-config> records."
  (shell-config-widgets (apply get-shell-config args)))

(define (get-shell-clipboard . args)
  "Return the clipboard-settings alist, or '() when it was not configured."
  (shell-config-clipboard (apply get-shell-config args)))

(define (get-shell-idle . args)
  "Return the idle-settings alist, or '() when it was not configured."
  (shell-config-idle (apply get-shell-config args)))

(define (get-shell-night-light . args)
  "Return the night-light alist, or '() when it was not configured."
  (shell-config-night-light (apply get-shell-config args)))

(define (get-shell-wallpaper . args)
  "Return the wallpaper-settings alist, or '() when it was not configured."
  (shell-config-wallpaper (apply get-shell-config args)))

(define (get-shell-frost . args)
  "Return the frosted-glass alist, or '() when it was not configured."
  (shell-config-frost (apply get-shell-config args)))

(define (shell-menu->units . args)
  "Return the shell menu as the C runtime reads it: a list of menu units.

The record holds Scheme, so this is a pass-through; it exists because the
bridge asks for a named procedure rather than poking at a record's field, and
because a settings file may want to read the menu it configured."
  (let ((menu (apply get-shell-menu args)))
    (if (and (list? menu) (not (null? menu)))
        menu
        '())))

(define (get-shell-enable-repl? . args)
  (shell-config-enable-repl? (apply get-shell-config args)))

(define (get-shell-repl-socket-path . args)
  (shell-config-repl-socket-path (apply get-shell-config args)))

;; =============================================================================
;; The bridge's view
;; =============================================================================
;;
;; SRFI-9 accessors are macros; the C bridge cannot call a macro, so a bar is
;; handed over as an alist of plain values whose module lists are alists too.
;; Everything the C side needs is here, which is also why adding a field to the
;; record is a one-line change rather than a new C accessor.

(define (bar->alist b)
  "Return BAR as a plain alist for the C runtime."
  (list (cons 'name (bar-config-name b))
        (cons 'theme (bar-config-theme b))
        (cons 'edge (bar-config-edge b))
        (cons 'layer (bar-config-layer b))
        (cons 'monitor (bar-config-monitor b))
        (cons 'monitors (bar-config-monitors b))
        (cons 'when (bar-config-when b))
        (cons 'height (bar-config-height b))
        (cons 'margin-top (bar-config-margin-top b))
        (cons 'margin-right (bar-config-margin-right b))
        (cons 'margin-bottom (bar-config-margin-bottom b))
        (cons 'margin-left (bar-config-margin-left b))
        (cons 'exclusive? (bar-config-exclusive? b))
        (cons 'auto-hide? (bar-config-auto-hide? b))
        (cons 'auto-hide-delay (bar-config-auto-hide-delay b))
        (cons 'keyboard? (bar-config-keyboard? b))
        (cons 'spacing (bar-config-spacing b))
        (cons 'padding (bar-config-padding b))
        (cons 'font (bar-config-font b))
        (cons 'font-size (bar-config-font-size b))
        (cons 'icon-font (bar-config-icon-font b))
        (cons 'foreground (bar-config-foreground b))
        (cons 'background (bar-config-background b))
        (cons 'accent (bar-config-accent b))
        (cons 'border-color (bar-config-border-color b))
        (cons 'radius (bar-config-radius b))
        (cons 'opacity (bar-config-opacity b))
        (cons 'blur (bar-config-blur b))
        (cons 'shine (bar-config-shine b))
        (cons 'reflection (bar-config-reflection b))
        (cons 'flare (bar-config-flare b))
        (cons 'css (bar-config-css b))
        (cons 'launcher-width (bar-config-launcher-width b))
        (cons 'launcher-height (bar-config-launcher-height b))
        (cons 'launcher-icon-size (bar-config-launcher-icon-size b))
        (cons 'launcher-font-scale (bar-config-launcher-font-scale b))
        (cons 'modules-left
              (map module->alist (bar-config-modules-left b)))
        (cons 'modules-center
              (map module->alist (bar-config-modules-center b)))
        (cons 'modules-right
              (map module->alist (bar-config-modules-right b)))
        (cons 'overflow-expanded? (bar-config-overflow-expanded? b))
        (cons 'overflow
              (map module->alist (bar-config-modules-overflow b)))))

(define (shell-bars->alists . args)
  "Return the configured bars as alists, for the C runtime."
  (map bar->alist (apply get-shell-bars args)))

(define (shell-clipboard->alist . args)
  "Return the clipboard settings as an alist, or '() when unconfigured."
  (let ((settings (apply get-shell-clipboard args)))
    (if (list? settings) settings '())))

(define (shell-wallpaper->alist . args)
  "Return the wallpaper settings as an alist, or '() when unconfigured."
  (let ((settings (apply get-shell-wallpaper args)))
    (if (list? settings) settings '())))

(define (shell-frost->alist . args)
  "Return the frosted-glass settings as an alist, or '() when unconfigured."
  (let ((settings (apply get-shell-frost args)))
    (if (list? settings) settings '())))

(define (widget->alist w)
  "Return WIDGET as the plain alist the C bridge reads."
  (list (cons 'kind (widget-config-kind w))
        (cons 'x (widget-config-x w))
        (cons 'y (widget-config-y w))
        (cons 'width (widget-config-width w))
        (cons 'height (widget-config-height w))
        (cons 'monitor (widget-config-monitor w))
        (cons 'size (widget-config-size w))
        (cons 'anchor (widget-config-anchor w))
        (cons 'options (widget-config-options w))))

(define (shell-widgets->alists . args)
  "Return the configured widgets as alists, for the C runtime."
  (map widget->alist (apply get-shell-widgets args)))

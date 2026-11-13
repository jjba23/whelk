;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
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
;; it goes; the three lists are what it says.  Widgets that are not a bar
;; (notifications, a launcher, a floating panel) are deliberately not part of
;; this record yet: they will be their own record and their own C surface, which
;; is why the surface layer in C takes a generic description rather than a bar.

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
            bar-config-height
            bar-config-margin-top
            bar-config-margin-right
            bar-config-margin-bottom
            bar-config-margin-left
            bar-config-exclusive?
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
            bar-config-css
            bar-config-modules-left
            bar-config-modules-center
            bar-config-modules-right
            bar
            clipboard-settings
            get-shell-clipboard
            configure-shell!
            normalize-edge
            normalize-layer
            get-shell-config
            bind-shell-config!
            forget-shell-config!
            get-shell-bars
            get-shell-menu
            shell-menu->units
            get-shell-enable-repl?
            get-shell-repl-socket-path
            bar->alist
            shell-bars->alists
            shell-clipboard->alist))

;; =============================================================================
;; The bar record
;; =============================================================================

(define-record-type <bar-config>
  (make-bar-config name theme edge layer monitor height
                   margin-top margin-right margin-bottom margin-left
                   exclusive? spacing padding
                   font font-size icon-font
                   foreground background accent border-color radius css
                   modules-left modules-center modules-right)
  bar-config?
  (name bar-config-name)
  (theme bar-config-theme)
  (edge bar-config-edge)
  (layer bar-config-layer)
  (monitor bar-config-monitor)
  (height bar-config-height)
  (margin-top bar-config-margin-top)
  (margin-right bar-config-margin-right)
  (margin-bottom bar-config-margin-bottom)
  (margin-left bar-config-margin-left)
  (exclusive? bar-config-exclusive?)
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
  (css bar-config-css)
  (modules-left bar-config-modules-left)
  (modules-center bar-config-modules-center)
  (modules-right bar-config-modules-right))

;; =============================================================================
;; The shell record
;; =============================================================================

;; The shell menu is the popover behind the mark at the far left of the bar:
;; `(whelk menu)' units, or #f for "the default the shell ships".
(define-record-type <shell-config>
  (make-shell-config bars shell-menu clipboard enable-repl? repl-socket-path)
  shell-config?
  (bars shell-config-bars)
  (shell-menu shell-config-shell-menu)
  (clipboard shell-config-clipboard)
  (enable-repl? shell-config-enable-repl?)
  (repl-socket-path shell-config-repl-socket-path))

(define %default-shell-config%
  (make-shell-config '() #f '() #f "/tmp/whelk-repl.sock"))

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

(define* (bar #:key
              (name 'main)
              (position 'top)
              (layer 'top)
              (monitor "all")
              (height 32)
              (margin 0)
              (exclusive? #t)
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
              (css #f)
              (left '())
              (center '())
              (right '()))
  "Describe one bar.

POSITION is the edge it is anchored to ('top, 'bottom, 'left, 'right).  HEIGHT
is its thickness -- the row's height on the top and bottom edges, its column's
width on the left and right ones.  MONITOR is \"all\", \"default\" or a connector
name such as \"DP-1\".  MARGIN is a number or a CSS-style list (top right bottom
left) of pixels between the bar and the screen edge.  EXCLUSIVE? reserves the
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
readable bar."
  (let* ((margins (normalize-margins margin))
         (palette (theme-palette theme))
         ;; A theme is a starting point: whatever the bar sets itself wins.
         (from-theme (lambda (key explicit)
                       (if explicit explicit (assq-ref palette key)))))
    (when (and (not (list? left)) (not (list? center)) (not (list? right)))
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
                     (as-int height 32)
                     (list-ref margins 0)
                     (list-ref margins 1)
                     (list-ref margins 2)
                     (list-ref margins 3)
                     (if exclusive? #t #f)
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
                     (from-theme 'css css)
                     (if (list? left) left '())
                     (if (list? center) center '())
                     (if (list? right) right '()))))

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

(define* (configure-shell! #:key
                           (config-path 'default)
                           (bars '())
                           (shell-menu #f)
                           (clipboard '())
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

CLIPBOARD is `(clipboard-settings)' or an alist with the same keys: it sets the
history's persistence, caps and retention.  The empty default keeps the
environment-variable behaviour.

ENABLE-REPL? starts a Guile REPL on a Unix socket when the shell is up, which is
how a configuration is changed without restarting the compositor:
`echo '(load-shell-config! ...)' | socat - UNIX-CONNECT:/tmp/whelk-repl.sock'."
  (let* ((valid (filter bar-config? (if (list? bars) bars '())))
         (cfg (make-shell-config valid shell-menu
                                 (if (list? clipboard) clipboard '())
                                 enable-repl?
                                 repl-socket-path)))
    (when (and (list? bars) (not (= (length valid) (length bars))))
      (log-warn "State"
                "~a of ~a bars were not <bar-config> records: they are ignored"
                (- (length bars) (length valid)) (length bars)))

    (hash-set! *shell-configs* config-path cfg)

    ;; The first settings file to load is also the fallback for anything that
    ;; asks for a configuration without naming one.
    (when (or (eq? config-path 'default)
              (not (hash-ref *shell-configs* 'default #f)))
      (hash-set! *shell-configs* 'default cfg))

    (log-info "State"
              "Registered [~a]: ~a bar(s) ~a, repl: ~a"
              config-path
              (length valid)
              (map (lambda (b)
                     (simple-format #f "~a(~a, ~a modules)"
                             (bar-config-name b)
                             (bar-config-edge b)
                             (+ (length (bar-config-modules-left b))
                                (length (bar-config-modules-center b))
                                (length (bar-config-modules-right b)))))
                   valid)
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

(define (get-shell-clipboard . args)
  "Return the clipboard-settings alist, or '() when it was not configured."
  (shell-config-clipboard (apply get-shell-config args)))

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
        (cons 'height (bar-config-height b))
        (cons 'margin-top (bar-config-margin-top b))
        (cons 'margin-right (bar-config-margin-right b))
        (cons 'margin-bottom (bar-config-margin-bottom b))
        (cons 'margin-left (bar-config-margin-left b))
        (cons 'exclusive? (bar-config-exclusive? b))
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
        (cons 'css (bar-config-css b))
        (cons 'modules-left
              (map module->alist (bar-config-modules-left b)))
        (cons 'modules-center
              (map module->alist (bar-config-modules-center b)))
        (cons 'modules-right
              (map module->alist (bar-config-modules-right b)))))

(define (shell-bars->alists . args)
  "Return the configured bars as alists, for the C runtime."
  (map bar->alist (apply get-shell-bars args)))

(define (shell-clipboard->alist . args)
  "Return the clipboard settings as an alist, or '() when unconfigured."
  (let ((settings (apply get-shell-clipboard args)))
    (if (list? settings) settings '())))

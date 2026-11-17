;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; GUI preferences are runtime state, not configuration.  The settings window
;; writes the alist below under XDG_STATE_HOME, and the loader layers it over
;; the bars it just read.  The primary settings file is never rewritten: a GUI
;; change is a reversible override, and deleting the override file is the
;; "reset to configuration" action.
;;
;; One file per settings file, like lambdock's preferences-<stem>.scm, so two
;; configurations can have two different GUI looks without knowing about each
;; other.

;;; Code:

(define-module (whelk prefs)
  #:declarative? #f
  #:use-module (ice-9 pretty-print)
  #:use-module (ice-9 string-fun)
  #:use-module (srfi srfi-1)
  #:use-module (whelk logger)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:use-module (whelk themes)
  #:export (preferences-path
            preferences-read
            preference-ref
            save-preference!
            reset-preferences!
            toggle-night-light!
            apply-shell-preferences!))

;; =============================================================================
;; Where the overrides live
;; =============================================================================

(define (whelk-state-dir)
  "The directory whelk keeps runtime state in, following XDG."
  (let ((xdg (getenv "XDG_STATE_HOME"))
        (home (getenv "HOME")))
    (cond ((and xdg (not (string-null? xdg)))
           (string-append xdg "/whelk"))
          (home (string-append home "/.local/state/whelk"))
          (else "/tmp/whelk"))))

(define (settings-stem config-path)
  "CONFIG-PATH's file name without its .scm suffix."
  (let* ((base (basename config-path))
         (suffix ".scm"))
    (if (string-suffix? suffix base)
        (string-drop-right base (string-length suffix))
        base)))

(define (preferences-path config-path)
  "Return the override file belonging to CONFIG-PATH, or #f."
  (and (string? config-path)
       (not (string-null? config-path))
       (string-append (whelk-state-dir)
                      "/preferences-"
                      (settings-stem config-path)
                      ".scm")))

(define (make-directories! path)
  "Create PATH and its parents, ignoring a directory that already exists."
  (unless (or (string-null? path) (file-exists? path))
    (let ((parent (dirname path)))
      (unless (string=? parent path)
        (make-directories! parent)))
    (catch 'system-error
      (lambda () (mkdir path))
      (lambda _ #t))))

;; =============================================================================
;; Reading and writing
;; =============================================================================

(define (preference-pair? value)
  (and (pair? value)
       (symbol? (car value))))

(define (preferences-read config-path)
  "Return CONFIG-PATH's GUI overrides as an alist, newest first.

A missing file is not an error: it means the configuration is still in charge.
A malformed file is reported and ignored as a whole, so a half-written state
file cannot leave the shell with a half-applied look."
  (let ((path (preferences-path config-path)))
    (if (not (or (and path (file-exists? path))))
        '()
        (catch #t
          (lambda ()
            (call-with-input-file path
              (lambda (port)
                (let ((value (read port)))
                  (if (and (list? value) (every preference-pair? value))
                      (filter preference-pair? value)
                      (begin
                        (log-warn "Prefs"
                                  "Ignoring malformed preference file: ~a"
                                  path)
                        '()))))))
          (lambda (key . args)
            (log-warn "Prefs" "Could not read ~a: ~a ~s" path key args)
            '())))))

;; Only the keys the settings window actually edits.  A typo in a widget key
;; must not become a setting that the rest of whelk silently ignores.
(define %preference-keys
  '(bar-height bar-spacing bar-padding bar-edge bar-layer bar-exclusive?
    bar-overflow-expanded?
    bar-font bar-font-size bar-theme bar-foreground bar-background bar-accent
    bar-border-color bar-radius bar-opacity bar-blur bar-shine bar-reflection
    bar-flare
    launcher-width launcher-height launcher-icon-size launcher-font-scale
    wifi-show-ssid? wifi-interval
    notifications-show-count?
    menu-shortcuts? menu-app-name? menu-accelerator-style
    tray-max-items clock-format
    night-light-enabled? night-light-mode night-light-temperature
    night-light-start night-light-end night-light-transition
    wallpaper-selected
    launcher-default-mode
    launcher-key-window launcher-key-run launcher-key-calc launcher-key-file
    launcher-key-web launcher-key-ssh launcher-key-quit
    launcher-key-clipboard launcher-key-menuitem))

;; A module page edits one module on the first bar.  Its keys carry the
;; module's kind and its occurrence among modules of that kind:
;; `module-clock-0-format' is the first clock and `module-clock-1-format' the
;; second.  Occurrences are counted left, centre, right, overflow, which is the
;; order the settings window walks.  Adding or reordering a module of another
;; kind cannot move a key; only another module of the same kind can.
(define (decimal-string? text)
  (and (not (string-null? text))
       (every (lambda (character) (char-numeric? character))
              (string->list text))))

(define (module-preference key)
  "Return (KIND OCCURRENCE . OPTION) for a module override symbol, or #f."
  (and (symbol? key)
       (let* ((name (symbol->string key))
              (parts (and (string-prefix? "module-" name)
                          (string-split (substring name 7) #\-))))
         (and parts
              (let loop ((kind-parts '()) (rest parts))
                (and (pair? rest)
                     (if (decimal-string? (car rest))
                         (let ((kind (reverse kind-parts))
                               (option (cdr rest)))
                           (and (pair? kind)
                                (pair? option)
                                (let ((occurrence
                                       (string->number (car rest))))
                                  (and (exact-integer? occurrence)
                                       (>= occurrence 0)
                                       (cons (string->symbol
                                              (string-join kind "-"))
                                             (cons occurrence
                                                   (string->symbol
                                                    (string-join option
                                                                 "-"))))))))
                         (loop (cons (car rest) kind-parts)
                               (cdr rest)))))))))

(define (valid-preference-key? key)
  (or (memq key %preference-keys)
      (and (module-preference key) #t)))

(define (save-preference! config-path key value)
  "Save one GUI preference for CONFIG-PATH and return #t on success.

KEY is a symbol from the settings window's vocabulary.  Existing values are
replaced rather than appended, so reading the file never depends on which
duplicate happens to win."
  (let ((path (preferences-path config-path)))
    (cond
      ((not path)
       (log-warn "Prefs" "Cannot save a preference without a settings path")
       #f)
      ((not (valid-preference-key? key))
       (log-warn "Prefs" "Ignoring unknown preference key: ~a" key)
       #f)
      (else
       (catch #t
         (lambda ()
           (make-directories! (dirname path))
           (let* ((old (preferences-read config-path))
                  (updated (cons (cons key value)
                                 (filter (lambda (pair)
                                           (not (eq? (car pair) key)))
                                         old))))
             (call-with-output-file path
               (lambda (port)
                 (display ";; whelk GUI preferences -- delete this file to reset.\n"
                          port)
                 (pretty-print updated port)))
             #t))
         (lambda (key* . args)
           (log-error "Prefs" "Could not save ~a: ~a ~s" path key* args)
           #f))))))

(define (reset-preferences! config-path)
  "Delete CONFIG-PATH's GUI overrides, returning #t when none are left."
  (let ((path (preferences-path config-path)))
    (if (not path)
        #f
        (catch #t
          (lambda ()
            (when (file-exists? path)
              (delete-file path))
            #t)
          (lambda (key . args)
            (log-warn "Prefs" "Could not reset ~a: ~a ~s" path key args)
            #f)))))

;; =============================================================================
;; Layering the overrides over a loaded bar
;; =============================================================================

(define (preference prefs key fallback)
  (let ((pair (assq key prefs)))
    (if pair (cdr pair) fallback)))

(define (preference-ref config-path key)
  "Return the saved value for KEY, or #f when nothing overrides it."
  (let ((pair (assq key (preferences-read config-path))))
    (if pair (cdr pair) #f)))

(define (integer-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (if (and (number? value) (>= value 0))
        (inexact->exact (round value))
        fallback)))

(define (string-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (if (and (string? value) (not (string-null? value)))
        value
        fallback)))

(define (boolean-preference prefs key fallback)
  (let ((pair (assq key prefs)))
    (if pair (if (cdr pair) #t #f) fallback)))

(define (symbol-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (cond ((symbol? value) value)
          ((string? value) (string->symbol value))
          (else fallback))))

(define (number-preference prefs key fallback)
  (let ((value (preference prefs key fallback)))
    (if (number? value) (exact->inexact value) fallback)))

(define (unit-preference prefs key fallback)
  (min 1.0 (max 0.0 (number-preference prefs key fallback))))

(define (theme-preference prefs bar)
  (symbol-preference prefs 'bar-theme (bar-config-theme bar)))

(define (apply-module-preferences mod prefs kind occurrence)
  "Apply `module-KIND-OCCURRENCE-OPTION' overrides to MOD.

`visible?' is the settings window's switch: it is stored inverted as the
module's own `hidden?' option, which the bar builder drops before it makes a
widget.  Keeping the module in the bar description is what lets the window list
a hidden module and turn it back on."
  (let ((prefix (string-append "module-" (symbol->string kind) "-"
                               (number->string occurrence) "-")))
    (let loop ((pairs prefs) (mod mod))
      (if (null? pairs)
          mod
          (let* ((key (caar pairs))
                 (name (symbol->string key)))
            (if (not (string-prefix? prefix name))
                (loop (cdr pairs) mod)
                (let ((option (string->symbol
                               (substring name (string-length prefix)))))
                  (loop (cdr pairs)
                        (if (eq? option 'visible?)
                            (module-set-option
                             mod 'hidden? (not (cdr (car pairs))))
                            (module-set-option mod option
                                               (cdr (car pairs))))))))))))

(define (next-module-occurrence! counter kind)
  "Return KIND's next 0-based occurrence in the bar and advance COUNTER."
  (let* ((counts (cdr counter))
         (pair (assq kind counts)))
    (if pair
        (let ((occurrence (cdr pair)))
          (set-cdr! pair (+ occurrence 1))
          occurrence)
        (begin
          (set-cdr! counter (cons (cons kind 1) counts))
          0))))

(define (apply-module-group mods prefs counter overrides?)
  "Apply the global reading preferences and the per-module overrides.

COUNTER is one mutable pair shared by all four groups, so every module of a
kind is numbered by its occurrence across the whole first bar, exactly as the
settings window numbers it."
  (let loop ((mods mods) (out '()))
    (if (null? mods)
        (reverse out)
        (let* ((kind (shell-module-kind (car mods)))
               (occurrence (next-module-occurrence! counter kind))
               (mod (apply-module (car mods) prefs))
               (mod (if overrides?
                        (apply-module-preferences mod prefs kind occurrence)
                        mod)))
          (loop (cdr mods) (cons mod out))))))

(define (apply-bar bar prefs module-overrides?)
  (let* ((theme (theme-preference prefs bar))
         (palette (theme-palette theme))
         (theme-set? (and (assq 'bar-theme prefs) #t))
         (from-palette (lambda (key current)
                         (if theme-set?
                             (assq-ref palette key)
                             current)))
         (radius (if theme-set?
                     (or (assq-ref palette 'radius) 0)
                     (bar-config-radius bar)))
         (material (lambda (key current)
                     (if theme-set?
                         (or (assq-ref palette key) current)
                         current)))
         (color (lambda (key current)
                  (string-preference prefs key
                                     (if theme-set?
                                         (or (assq-ref palette key) current)
                                         current))))
         (counter (cons 'module-kind-counts '())))
    (make-bar-config
     (bar-config-name bar)
     theme
     (normalize-edge (preference prefs 'bar-edge (bar-config-edge bar)))
     (normalize-layer (preference prefs 'bar-layer (bar-config-layer bar)))
     (bar-config-monitor bar)
     (bar-config-monitors bar)
     (bar-config-when bar)
     (integer-preference prefs 'bar-height (bar-config-height bar))
     (bar-config-margin-top bar)
     (bar-config-margin-right bar)
     (bar-config-margin-bottom bar)
     (bar-config-margin-left bar)
     (boolean-preference prefs 'bar-exclusive? (bar-config-exclusive? bar))
     (integer-preference prefs 'bar-spacing (bar-config-spacing bar))
     (integer-preference prefs 'bar-padding (bar-config-padding bar))
     (string-preference prefs 'bar-font (bar-config-font bar))
     (string-preference prefs 'bar-font-size (bar-config-font-size bar))
     (bar-config-icon-font bar)
     (color 'bar-foreground
            (if theme-set? (assq-ref palette 'foreground)
                (bar-config-foreground bar)))
     (color 'bar-background
            (if theme-set? (assq-ref palette 'background)
                (bar-config-background bar)))
     (color 'bar-accent
            (if theme-set? (assq-ref palette 'accent)
                (bar-config-accent bar)))
     (color 'bar-border-color
            (if theme-set? (assq-ref palette 'border-color)
                (bar-config-border-color bar)))
     (integer-preference prefs 'bar-radius radius)
     (unit-preference prefs 'bar-opacity
                      (material 'opacity (bar-config-opacity bar)))
     (max 0 (min 64 (integer-preference
                     prefs 'bar-blur
                     (material 'blur (bar-config-blur bar)))))
     (unit-preference prefs 'bar-shine
                      (material 'shine (bar-config-shine bar)))
     (unit-preference prefs 'bar-reflection
                      (material 'reflection (bar-config-reflection bar)))
     (unit-preference prefs 'bar-flare
                      (material 'flare (bar-config-flare bar)))
     (from-palette 'css (bar-config-css bar))
     (integer-preference prefs 'launcher-width
                         (bar-config-launcher-width bar))
     (integer-preference prefs 'launcher-height
                         (bar-config-launcher-height bar))
     (integer-preference prefs 'launcher-icon-size
                         (bar-config-launcher-icon-size bar))
     (integer-preference prefs 'launcher-font-scale
                         (bar-config-launcher-font-scale bar))
     (apply-module-group (bar-config-modules-left bar) prefs counter
                         module-overrides?)
     (apply-module-group (bar-config-modules-center bar) prefs counter
                         module-overrides?)
     (apply-module-group (bar-config-modules-right bar) prefs counter
                         module-overrides?)
     ;; The chevron's own switch is remembered like any other bar preference: a
     ;; reload must not close a shelf the reader opened.
     (boolean-preference prefs 'bar-overflow-expanded?
                         (bar-config-overflow-expanded? bar))
     (apply-module-group (bar-config-modules-overflow bar) prefs counter
                         module-overrides?))))

(define (set-option-if-present mod prefs preference-key option-key)
  (let ((pair (assq preference-key prefs)))
    (if pair
        (module-set-option mod option-key (cdr pair))
        mod)))

(define (apply-module mod prefs)
  (case (shell-module-kind mod)
    ((wifi)
     (let ((mod (set-option-if-present mod prefs
                                       'wifi-show-ssid? 'show-ssid?)))
       (set-option-if-present mod prefs 'wifi-interval 'interval)))
    ((notifications)
     (let ((pair (assq 'notifications-show-count? prefs)))
       (if pair
           (module-set-option mod 'format
                              (if (cdr pair) "{icon} {count}" "{icon}"))
           mod)))
    ((menu)
     (let ((mod (set-option-if-present mod prefs
                                       'menu-shortcuts? 'shortcuts?)))
       (let ((mod (set-option-if-present mod prefs
                                         'menu-app-name? 'app-name?)))
         (set-option-if-present mod prefs
                                'menu-accelerator-style 'accelerator-style))))
    ((tray)
     (set-option-if-present mod prefs 'tray-max-items 'max-items))
    ((clock)
     (set-option-if-present mod prefs 'clock-format 'format))
    (else mod)))

(define %night-light-preference-keys
  '(night-light-enabled? night-light-mode night-light-temperature
    night-light-start night-light-end night-light-transition))

(define (apply-night-light-preferences prefs settings)
  "Layer the Night light page's overrides over SETTINGS.

SETTINGS is the alist `(night-light ...)' registered in the primary file.  A
configuration that predates the feature is given the constructor's defaults,
but only when the page has actually written one of its keys: a GUI edit to some
unrelated bar setting must not invent a night-light configuration."
  (define (overridden?)
    (any (lambda (key) (assq key prefs)) %night-light-preference-keys))
  (if (and (null? settings) (not (overridden?)))
      '()
      (let ((base (if (null? settings) (night-light) settings)))
        (list (cons 'configured? #t)
              (cons 'enabled?
                    (boolean-preference prefs 'night-light-enabled?
                                        (assq-ref base 'enabled?)))
              (cons 'mode (symbol-preference prefs 'night-light-mode
                                             (assq-ref base 'mode)))
              (cons 'temperature
                    (integer-preference prefs 'night-light-temperature
                                        (assq-ref base 'temperature)))
              (cons 'start (integer-preference prefs 'night-light-start
                                               (assq-ref base 'start)))
              (cons 'end (integer-preference prefs 'night-light-end
                                             (assq-ref base 'end)))
              (cons 'transition
                    (integer-preference prefs 'night-light-transition
                                        (assq-ref base 'transition)))))))

(define (night-light-enabled? config-path)
  "Whether CONFIG-PATH's night light is enabled after its overrides.

This is the same layering the shell uses, so a keybinding toggles what the
Night light page shows rather than whatever the primary file happened to say
before a GUI override was saved."
  (let* ((settings (get-shell-night-light config-path))
         (resolved (apply-night-light-preferences
                    (preferences-read config-path) settings)))
    (and (pair? resolved)
         (eq? (assq-ref resolved 'enabled?) #t))))

(define (toggle-night-light! config-path)
  "Flip CONFIG-PATH's night-light switch and save it as a GUI override.

Returns 1 when the night light is now on, 0 when it is now off, or -1 when the
override could not be written.  The caller reloads the shell, exactly as the
Night light page does after a switch; keeping the write and the reload separate
means this stays usable from a headless command line and from Guile tests."
  (let ((enabled (not (night-light-enabled? config-path))))
    (if (save-preference! config-path 'night-light-enabled? enabled)
        (if enabled 1 0)
        -1)))

(define (apply-shell-preferences! config-path)
  "Apply CONFIG-PATH's GUI overrides to the shell registered for it.

Returns the bars as they now stand.  Called after every settings load and by
the settings window's reload callback, so the two paths cannot drift apart."
  (let* ((prefs (preferences-read config-path))
         (bars (get-shell-bars config-path)))
    (if (or (null? prefs) (null? bars))
        bars
        ;; The window edits the first bar's modules; a second bar keeps its own
        ;; configuration rather than having the first bar's clock hide a clock
        ;; in a different bar.
        (let* ((position 0)
               (updated
                (map (lambda (bar)
                       (let ((updated (apply-bar bar prefs (= position 0))))
                         (set! position (+ position 1))
                         updated))
                     bars)))
          (configure-shell!
           #:config-path config-path
           #:bars updated
           ;; A GUI edit changes a bar, not the desktop: the widgets the
           ;; settings file registered are carried across unchanged.  Leaving
           ;; them out here re-registered the shell with no widgets at all,
           ;; which is how every desktop widget disappeared the first time a
           ;; preference was saved.
           #:widgets (get-shell-widgets config-path)
           #:shell-menu (get-shell-menu config-path)
           #:clipboard (get-shell-clipboard config-path)
           ;; A GUI edit is a bar edit, but the shell record it re-registers must
           ;; still carry the policies the settings file owns.  Dropping idle
           ;; here reset the lock timer on every preference save; dropping the
           ;; night light would turn it off for the same reason.
           #:idle (get-shell-idle config-path)
           #:night-light (apply-night-light-preferences
                          prefs (get-shell-night-light config-path))
           ;; The wallpaper engine is not a bar option, but it lives in the
           ;; same shell record: a GUI save must not drop it.  The frosted
           ;; glass is the same story -- losing it on every control change
           ;; would flash every pane back to its flat skin.
           #:wallpaper (get-shell-wallpaper config-path)
           #:frost (get-shell-frost config-path)
           #:enable-repl? (get-shell-enable-repl? config-path)
           #:repl-socket-path (get-shell-repl-socket-path config-path))
          updated))))

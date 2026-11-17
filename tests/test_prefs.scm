;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; GUI preference overrides: the settings window writes them to a state file,
;; and the loader layers them on top of the primary settings file.  These are
;; the things a widget test cannot see: persistence, precedence and the restore
;; path when the override is deleted.

(use-modules (whelk core)
             (whelk prefs)
             (whelk state)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define root (or (getenv "TMPDIR") "/tmp"))
(define state-dir
  (string-append root "/whelk-prefs-test-"
                 (number->string (current-time))))
(define config-path (string-append state-dir "/settings.scm"))

(mkdir state-dir)
(setenv "XDG_STATE_HOME" state-dir)
(setenv "HOME" state-dir)

(define (write-config! with-cpu?)
  (call-with-output-file config-path
    (lambda (port)
      (display "(configure-shell!\n" port)
      (display " #:widgets (list (desktop-widget 'clock #:x 20 #:y 30))\n" port)
      (display " #:bars\n" port)
      (display " (list (bar #:name 'main #:position 'top #:height 32\n" port)
      (display "             #:spacing 8 #:padding 12 #:theme 'gloss-2007\n" port)
      (display "             #:right\n" port)
      (display "             (list\n" port)
      (when with-cpu?
        (display "                   (cpu-module)\n" port))
      (display "                   (wifi-module #:show-ssid? #f #:interval 5)\n" port)
      (display "                   (notifications-module #:format \"{icon}\")\n" port)
      (display "                   (menu-module #:shortcuts? #t #:app-name? #t\n" port)
      (display "                                #:accelerator-style 'cua)\n" port)
      (display "                   (tray-module #:max-items 6)\n" port)
      (display "                   (clock-module #:format \"%H:%M\")\n" port)
      (display "                   (clock-module #:format \"%S\"))))\n" port)
      (display " #:idle (idle-settings #:lock-after 120)\n" port)
      (display " #:night-light (night-light #:enabled? #f #:temperature 3200\n" port)
      (display "                           #:start \"20:30\" #:end \"06:15\"\n" port)
      (display "                           #:transition 45))\n" port))))

(write-config! #f)
(load-shell-config! config-path)

(check "the file's spacing starts at eight"
       (= (bar-config-spacing (car (get-shell-bars config-path))) 8))

;; A preference is a bar edit.  It must not unregister the desktop widgets the
;; settings file configured: that regression left every card off the desktop
;; as soon as anything was changed in the settings window.
(check "a settings file's widgets are registered"
       (= (length (get-shell-widgets config-path)) 1))

(define initial-night (get-shell-night-light config-path))

(check "the night light is registered from the settings file"
       (and (eq? (assq-ref initial-night 'configured?) #t)
            (= (assq-ref initial-night 'temperature) 3200)
            (= (assq-ref initial-night 'start) (+ (* 20 60) 30))
            (= (assq-ref initial-night 'end) (+ (* 6 60) 15))
            (= (assq-ref initial-night 'transition) 45)))
(check "a fresh configuration has no GUI overrides"
       (null? (preferences-read config-path)))
(check "the override file is named after the settings file"
       (string-suffix? "/preferences-settings.scm"
                       (preferences-path config-path)))

;; A keybinding uses the same switch as the Night light page: toggle the
;; resolved value, save it as an override, then reload.  The clock and white
;; point must survive the switch, or turning the filter off and on again would
;; silently reset the policy.
(check "toggling the night light on returns 1"
       (= (toggle-night-light! config-path) 1))
(check "the toggle saves the enabled override"
       (eq? (assq-ref (preferences-read config-path) 'night-light-enabled?) #t))
(apply-shell-preferences! config-path)
(check "the resolved night light is on with its configured clock"
       (let ((light (get-shell-night-light config-path)))
         (and (eq? (assq-ref light 'enabled?) #t)
              (= (assq-ref light 'temperature) 3200)
              (= (assq-ref light 'start) (+ (* 20 60) 30))
              (= (assq-ref light 'end) (+ (* 6 60) 15)))))
(check "toggling the night light off returns 0"
       (= (toggle-night-light! config-path) 0))
(apply-shell-preferences! config-path)
(check "the resolved night light is off"
       (not (eq? (assq-ref (get-shell-night-light config-path) 'enabled?) #t)))

;; A GUI edit is written first and survives a reload: this is the property that
;; makes the window useful without ever rewriting the primary settings file.
(save-preference! config-path 'bar-height 44)
(save-preference! config-path 'bar-theme 'frost)
(save-preference! config-path 'bar-opacity 0.55)
(save-preference! config-path 'bar-blur 28)
(save-preference! config-path 'bar-shine 0.21)
(save-preference! config-path 'bar-reflection 0.32)
(save-preference! config-path 'bar-flare 0.43)
(save-preference! config-path 'bar-radius 20)
(save-preference! config-path 'bar-background "#102030")
(save-preference! config-path 'bar-edge 'bottom)
(save-preference! config-path 'wifi-show-ssid? #t)
(save-preference! config-path 'wifi-interval 2)
(save-preference! config-path 'notifications-show-count? #t)
(save-preference! config-path 'menu-shortcuts? #f)
(save-preference! config-path 'menu-accelerator-style 'emacs)
(save-preference! config-path 'tray-max-items 9)
(save-preference! config-path 'clock-format "%H:%M:%S")
;; A module card writes the kind and occurrence, not the position: two clocks
;; are two cards, and reordering another kind cannot move either key.
(check "a positional module key is refused"
       (not (save-preference! config-path 'module-0-visible? #t)))
(save-preference! config-path 'module-wifi-0-visible? #f)
(save-preference! config-path 'module-clock-0-format "%H:%M:%S")
(save-preference! config-path 'module-clock-0-interval 2)
(save-preference! config-path 'module-clock-1-format "%S seconds")
(save-preference! config-path 'launcher-default-mode 'window)
(save-preference! config-path 'launcher-key-run "")
(save-preference! config-path 'night-light-enabled? #t)
(save-preference! config-path 'night-light-temperature 2700)
(save-preference! config-path 'night-light-mode 'always)
(save-preference! config-path 'night-light-start (+ (* 21 60) 15))
(save-preference! config-path 'night-light-end (+ (* 6 60) 45))
(save-preference! config-path 'night-light-transition 15)
(save-preference! config-path 'launcher-width 740)
(save-preference! config-path 'launcher-height 680)
(save-preference! config-path 'launcher-icon-size 34)
(save-preference! config-path 'launcher-font-scale 130)
(save-preference! config-path 'not-a-real-setting #t)

(check "the override file exists"
       (file-exists? (preferences-path config-path)))
(check "the newest edit replaces its key"
       (equal? (assq-ref (preferences-read config-path) 'bar-height) 44))

(apply-shell-preferences! config-path)

(check "applying a preference keeps the desktop widgets"
       (equal? (map widget-config-kind (get-shell-widgets config-path))
               '(clock)))

(define applied (car (get-shell-bars config-path)))
(define frost (theme-palette 'frost))

(check "a GUI height overrides the settings file"
       (= (bar-config-height applied) 44))
(check "a GUI edge overrides the settings file"
       (eq? (bar-config-edge applied) 'bottom))
(check "a GUI theme is recorded" (eq? (bar-config-theme applied) 'frost))
(check "a GUI theme replaces the theme palette"
       (equal? (bar-config-foreground applied)
               (assq-ref frost 'foreground)))
(check "a GUI colour overrides the theme palette"
       (equal? (bar-config-background applied) "#102030"))
(check "a GUI material overrides every theme value"
       (and (= (bar-config-opacity applied) 0.55)
            (= (bar-config-blur applied) 28)
            (= (bar-config-shine applied) 0.21)
            (= (bar-config-reflection applied) 0.32)
            (= (bar-config-flare applied) 0.43)
            (= (bar-config-radius applied) 20)))
(check "an untouched value still comes from the settings file"
       (= (bar-config-spacing applied) 8))

(define (modules-by-kind bar kind)
  (filter (lambda (mod) (eq? (shell-module-kind mod) kind))
          (append (bar-config-modules-left bar)
                  (bar-config-modules-center bar)
                  (bar-config-modules-right bar)
                  (bar-config-modules-overflow bar))))

(define (module-by-kind bar kind)
  (let ((mods (modules-by-kind bar kind)))
    (and (pair? mods) (car mods))))

(check "Wi-Fi show-ssid is overridden"
       (eq? (module-option (module-by-kind applied 'wifi) 'show-ssid?) #t))
(check "Wi-Fi interval is overridden"
       (= (module-option (module-by-kind applied 'wifi) 'interval) 2))
(check "the notification count is overridden"
       (equal? (module-option (module-by-kind applied 'notifications) 'format)
               "{icon} {count}"))
(check "menu shortcuts are overridden"
       (eq? (module-option (module-by-kind applied 'menu) 'shortcuts?) #f))
(check "menu accelerator style is overridden"
       (eq? (module-option (module-by-kind applied 'menu) 'accelerator-style)
            'emacs))
(check "the tray limit is overridden"
       (= (module-option (module-by-kind applied 'tray) 'max-items) 9))
(let ((clocks (modules-by-kind applied 'clock)))
  (check "the first clock's format is overridden"
         (equal? (module-option (car clocks) 'format) "%H:%M:%S"))
  (check "the first clock's interval is overridden"
         (= (module-option (car clocks) 'interval) 2))
  (check "the second clock keeps its own format"
         (equal? (module-option (second clocks) 'format) "%S seconds")))
(check "a hidden module stays in the bar description"
       (and (module-by-kind applied 'wifi)
            (eq? (module-option (module-by-kind applied 'wifi) 'hidden?) #t)))
(check "a malformed module key is refused"
       (not (save-preference! config-path 'module-nope-format "x")))
(check "the launcher's default mode is recorded"
       (eq? (preference-ref config-path 'launcher-default-mode) 'window))
(check "a cleared launcher key is stored as empty"
       (equal? (preference-ref config-path 'launcher-key-run) ""))
(check "the launcher's size and zoom are overrides"
       (and (= (bar-config-launcher-width applied) 740)
            (= (bar-config-launcher-height applied) 680)
            (= (bar-config-launcher-icon-size applied) 34)
            (= (bar-config-launcher-font-scale applied) 130)))

(let ((night (get-shell-night-light config-path)))
  (check "the Night light page's switch is an override"
         (eq? (assq-ref night 'enabled?) #t))
  (check "the Night light page's temperature is an override"
         (= (assq-ref night 'temperature) 2700))
  (check "the Night light page's mode is an override"
         (eq? (assq-ref night 'mode) 'always))
  (check "the Night light page's clock ends are overrides"
         (and (= (assq-ref night 'start) (+ (* 21 60) 15))
              (= (assq-ref night 'end) (+ (* 6 60) 45))))
  (check "the Night light page's fade is an override"
         (= (assq-ref night 'transition) 15)))

;; Re-registering the shell for a preference must not drop the idle policy the
;; settings file owns either: that was the same class of bug as the widgets.
(check "a preference save keeps the idle policy"
       (= (assq-ref (get-shell-idle config-path) 'lock-after) 120))

;; Inserting another kind before the modules must not move their keys: the old
;; positional format would have hidden or renamed the wrong module here.
(write-config! #t)
(load-shell-config! config-path)

(define reordered (car (get-shell-bars config-path)))

(check "the added module does not take the Wi-Fi override"
       (and (module-by-kind reordered 'cpu)
            (eq? (module-option (module-by-kind reordered 'wifi) 'hidden?) #t)))
(let ((clocks (modules-by-kind reordered 'clock)))
  (check "the first clock keeps its format after the settings change"
         (equal? (module-option (car clocks) 'format) "%H:%M:%S"))
  (check "the second clock keeps its format after the settings change"
         (equal? (module-option (second clocks) 'format) "%S seconds")))

;; A module that was hidden can be shown again without editing the file.
(save-preference! config-path 'module-wifi-0-visible? #t)
(apply-shell-preferences! config-path)
(check "showing a module again clears its hidden flag"
       (not (module-option (module-by-kind (car (get-shell-bars config-path))
                                           'wifi)
                           'hidden?)))

;; Reloading the primary file must re-apply the overrides, not lose them.
(load-shell-config! config-path)
(check "a reload keeps the GUI height"
       (= (bar-config-height (car (get-shell-bars config-path))) 44))

;; Deleting the override restores the settings file exactly.
(reset-preferences! config-path)
(check "reset removes the override file"
       (not (file-exists? (preferences-path config-path))))
(load-shell-config! config-path)

(define restored (car (get-shell-bars config-path)))

(check "reset restores the settings height" (= (bar-config-height restored) 32))
(check "reset restores the settings edge" (eq? (bar-config-edge restored) 'top))
(check "reset restores the settings theme"
       (eq? (bar-config-theme restored) 'gloss-2007))
(check "reset restores the theme material"
       (let ((gloss (theme-palette 'gloss-2007)))
         (and (= (bar-config-opacity restored) (assq-ref gloss 'opacity))
              (= (bar-config-blur restored) (assq-ref gloss 'blur))
              (= (bar-config-shine restored) (assq-ref gloss 'shine))
              (= (bar-config-reflection restored)
                 (assq-ref gloss 'reflection))
              (= (bar-config-flare restored) (assq-ref gloss 'flare))
              (equal? (bar-config-background restored)
                      (assq-ref gloss 'background)))))
(check "reset restores the settings reading"
       (equal? (module-option (module-by-kind restored 'notifications) 'format)
               "{icon}"))
(check "reset restores the clock format"
       (equal? (module-option (module-by-kind restored 'clock) 'format)
               "%H:%M"))
(check "reset drops the module's hidden flag"
       (not (module-option (module-by-kind restored 'wifi) 'hidden?)))
(check "reset drops the launcher key"
       (not (preference-ref config-path 'launcher-key-run)))
(check "reset restores the launcher defaults"
       (and (= (bar-config-launcher-width restored) 660)
            (= (bar-config-launcher-height restored) 640)
            (= (bar-config-launcher-icon-size restored) 24)
            (= (bar-config-launcher-font-scale restored) 100)))
(check "reset restores the untouched spacing" (= (bar-config-spacing restored) 8))

(let ((night (get-shell-night-light config-path)))
  (check "reset restores the night light temperature"
         (= (assq-ref night 'temperature) 3200))
  (check "reset restores the night light switch"
         (eq? (assq-ref night 'enabled?) #f)))
(check "reset restores the idle policy"
       (= (assq-ref (get-shell-idle config-path) 'lock-after) 120))

(delete-file config-path)
(rmdir (string-append state-dir "/whelk"))
(rmdir state-dir)

(if (zero? failures)
    (begin (display "ok prefs\n") (exit 0))
    (begin (format (current-error-port) "~a preference check(s) failed~%"
                   failures)
           (exit 1)))

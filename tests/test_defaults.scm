;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The default bar: the heks-linux Waybar status items with the application-name
;; menu, and the same bar the shipped example configures.  The two are written
;; out separately, so the suite is what keeps them from drifting apart.

(use-modules (whelk defaults)
             (whelk modules)
             (whelk state)
             (whelk themes)
             (whelk loader)
             (whelk menudict)
             (srfi srfi-1)
             (srfi srfi-13))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define (kinds modules)
  (map shell-module-kind modules))

(define (shape b)
  (list (bar-config-name b)
        (bar-config-edge b)
        (bar-config-height b)
        (kinds (bar-config-modules-left b))
        (kinds (bar-config-modules-center b))
        (kinds (bar-config-modules-right b))
        (kinds (bar-config-modules-overflow b))))

(define built-in (default-bar))

;; The shipped look is the dark graphite glass of the late 2000s.  Its CSS
;; keeps the menu bar square; the palette radius still applies to popovers.
(define gloss (theme-palette 'gloss-2007))

(check "the default bar wears gloss-2007"
       (and (equal? (bar-config-foreground built-in)
                    (assq-ref gloss 'foreground))
            (equal? (bar-config-background built-in)
                    (assq-ref gloss 'background))
            (equal? (bar-config-accent built-in)
                    (assq-ref gloss 'accent))))

(check "the default bar keeps the theme's popover radius"
       (= (bar-config-radius built-in) (assq-ref gloss 'radius)))

(check "gloss-2007 squares only the menu bar"
       (string-contains (assq-ref gloss 'css) "border-radius: 0;"))

;; The theme supplies the surface's material, not a hard-coded CSS gradient:
;; a bar that names a theme gets that theme's opacity, shadow, shine,
;; reflection and flare unless it asks for its own.
(check "the default bar carries the theme's material"
       (and (= (bar-config-opacity built-in) (assq-ref gloss 'opacity))
            (= (bar-config-blur built-in) (assq-ref gloss 'blur))
            (= (bar-config-shine built-in) (assq-ref gloss 'shine))
            (= (bar-config-reflection built-in) (assq-ref gloss 'reflection))
            (= (bar-config-flare built-in) (assq-ref gloss 'flare))))

(check "a bar can override one material value"
       (and (= (bar-config-shine (bar #:shine 0.2)) 0.2)
            (= (bar-config-flare (bar #:flare 0.3)) 0.3)))

;; The night light is off unless a configuration asks for it, and its defaults
;; are the documented ones rather than an accidental zero.
(let ((night (night-light)))
  (check "a night light is off by default"
         (eq? (assq-ref night 'enabled?) #f))
  (check "its default white point is 3400 K"
         (= (assq-ref night 'temperature) 3400))
  (check "its default window is 19:00 to 07:00"
         (and (= (assq-ref night 'start) (* 19 60))
              (= (assq-ref night 'end) (* 7 60))))
  (check "its default fade is thirty minutes"
         (= (assq-ref night 'transition) 30)))

(check "a clock outside the day is clamped to the range"
       (= (assq-ref (night-light #:temperature 9000) 'temperature) 6500))
(check "an unknown mode falls back to the schedule"
       (eq? (assq-ref (night-light #:mode 'sometimes) 'mode) 'schedule))

;; The bar's left half is a menu bar's: the machine's own menu, then that
;; application's File and Edit, with a whelk where the apple is.  The window's
;; name is an optional module, not part of the default bar.
(check "the left half is the shell menu and the application's menus"
       (equal? (kinds (bar-config-modules-left built-in))
               '(start menu)))

;; The two dials are glyphs by default; the number is one #:format away.  A bar
;; that spells out "45%" at rest makes the eye read it every time it passes.
(define (default-reading kind)
  (let find ((modules (append (bar-config-modules-left built-in)
                              (bar-config-modules-center built-in)
                              (bar-config-modules-right built-in)
                              (bar-config-modules-overflow built-in))))
    (cond ((null? modules) #f)
          ((eq? (shell-module-kind (car modules)) kind) (car modules))
          (else (find (cdr modules))))))

(check "the volume reading is the speaker, not a percentage"
       (equal? (module-option (default-reading 'audio) 'format) "{icon}"))
(check "the brightness reading is the sun, not a percentage"
       (equal? (module-option (default-reading 'backlight) 'format) "{icon}"))

;; BAT0 is not universal (this project's test machine calls its pack BAT1), so
;; the default lets /sys/class/power_supply pick the first battery instead of
;; hiding the reading when the name differs.
(check "the battery auto-detects its pack"
       (not (module-option (default-reading 'battery) 'device)))

;; The clipboard tile is the shell's own history, not a glyph that opens
;; clipman.  Its glyph is the reading by default; the depth is opt-in through
;; #:format so a full history does not turn the tile into a number.
(check "the clipboard tile hides its depth by default"
       (not (string-contains (module-option (default-reading 'clipboard) 'format)
                             "{count}")))

;; The headphones tile is the way into the music player, not a transport
;; menu: a left click asks the running Emacs for an Orpheus frame, while the
;; module's own play/pause stays off that button.
(check "the headphones tile opens Orpheus"
       (string-contains (or (module-option (default-reading 'mpris) 'on-click)
                            "")
                        "(orpheus)"))

;; A reading is the shell's, not a daemon's: a custom tile prints its glyph, and
;; the player, the notification centre and the clipboard depth come from native
;; modules -- no `playerctl` in a command somewhere under the bar.
(define (default-modules b)
  (append (bar-config-modules-left b)
          (bar-config-modules-center b)
          (bar-config-modules-right b)
          (bar-config-modules-overflow b)))

(check "no default tile asks another program for a reading"
       (every (lambda (m)
                (let ((command (and (eq? (shell-module-kind m) 'custom)
                                    (module-option m 'exec))))
                  (or (not command)
                      (string-contains command "printf"))))
              (default-modules built-in)))
(check "the default bar's now-playing tile is MPRIS, not a subprocess"
       (let ((player (find (lambda (m) (eq? (shell-module-kind m) 'mpris))
                           (default-modules built-in))))
         (and player
              (equal? (module-option player 'format) "{icon}"))))

;; The session menu is whelk's own: a power button must not fail because a
;; desktop has not installed an external menu program.
(check "the battery tile opens whelk's session menu"
       (equal? (module-option (default-reading 'battery) 'on-click)
               "whelk --session-menu"))

;; And what that first menu opens is configuration, not a C table.
(check "the shell menu has the machine's items"
       (let ((units (default-shell-menu)))
         (and (pair? units)
              (equal? (car units) '(item "About This System" "shell:about"))
              (member '(item "Control Panel" "shell:settings") units)
              (member '(submenu "Recent Items" "shell:recent-items") units)
              (member '(item "Force Quit…" "shell:force-quit") units))))

;; The control panel is whelk's own settings window, not the desktop's
;; settings application.
(check "the shell menu does not open the desktop settings app"
       (not (member '(item "System Settings…" "gnome-control-center")
                    (default-shell-menu))))

;; Panels are still reachable through their command line and keybindings; the
;; shell menu is a session menu, not a directory of every surface.
(check "the shell menu has no panel entries"
       (let ((units (default-shell-menu)))
         (not (any (lambda (unit)
                     (member unit '((item "Network" "shell:network")
                                    (item "Bluetooth" "shell:bluetooth")
                                    (item "Notifications"
                                          "shell:notifications"))))
                   units))))

;; The reference's right half, in its order: the media and desktop items, the
;; hardware readouts, the status icons, then search and the clock.  The
;; clipboard and do-not-disturb tiles are whelk's own modules, so the defaults
;; ask the shell for their state instead of querying another program.
(check "the right half keeps the reference's order"
       (equal? (kinds (bar-config-modules-right built-in))
               '(mpris clipboard custom dnd custom custom memory audio keyboard
                 battery power-profile wifi bluetooth tray notifications wallpaper
                 custom clock)))

;; The readings that are consulted rather than glanced at -- the load, the ways
;; into the machine's own tools, the keyboard layout and the brightness -- live
;; on the row's shelf instead of taking its width all day.
(check "the reference shelf is the configured overflow"
       (equal? (kinds (bar-config-modules-overflow built-in))
               '(cpu custom language backlight)))

(check "the overflow starts closed"
       (not (bar-config-overflow-expanded? built-in)))

(check "a bar can open its overflow at start"
       (bar-config-overflow-expanded? (bar #:overflow-expanded? #t)))

(check "the bridge carries the overflow"
       (let ((alist (bar->alist (bar #:overflow-expanded? #t
                                     #:overflow (list (cpu-module))))))
         (and (eq? (assq-ref alist 'overflow-expanded?) #t)
              (= (length (assq-ref alist 'overflow)) 1))))

(check "the shipped battery hides its number while charging"
       (equal? (module-option (default-reading 'battery)
                              'show-capacity-when-charging?)
               #f))

(check "the bar is a top bar"
       (and (eq? (bar-config-edge built-in) 'top)
            (= (bar-config-height built-in) 32)))

(check "the bar is on the fixed top layer by default"
       (eq? (bar-config-layer built-in) 'top))

(check "a bar can move with niri's overview"
       (eq? (bar-config-layer (bar #:name 'moving #:layer 'bottom)) 'bottom))

(check "the bridge carries the configured layer"
       (eq? (assq-ref (bar->alist (bar #:name 'moving #:layer 'bottom))
                      'layer)
            'bottom))

;; The example a new machine copies configures that very bar.
(define example (cadr (command-line)))

(check "the example file loads" (load-settings-file! example))

(define bars (get-shell-bars example))

(check "the example configures one bar" (= (length bars) 1))
(check "the example ships the Golden Gate widget set"
       (= (length (get-shell-widgets example)) 8))
(check "the example is the default bar" (equal? (shape (car bars))
                                                 (shape built-in)))
(check "the example wears the default theme"
       (and (equal? (bar-config-foreground (car bars))
                    (bar-config-foreground built-in))
            (equal? (bar-config-background (car bars))
                    (bar-config-background built-in))
            (equal? (bar-config-accent (car bars))
                    (bar-config-accent built-in))
            (= (bar-config-radius (car bars))
               (bar-config-radius built-in))))


;; ============================================================================
;; The dictionary from a settings file
;; ============================================================================
;; The dictionary is Scheme, so a settings file configures it the same way it
;; configures the bar: one load, no rebuild.  This writes the file a user would
;; write, loads it through the loader, and asks the same question the runtime
;; asks.

(define menu-settings
  (string-append (or (getenv "TMPDIR") "/tmp") "/whelk-menu-settings-test.scm"))

(call-with-output-file menu-settings
  (lambda (port)
    (display "(add-menu-app \"Steam\" '(\"steam\")\n" port)
    (display "              '((\"File\" (\"Settings\" \"Ctrl+,\" \"ctrl\" \"comma\"))))\n" port)
    (display "(replace-menu-app \"firefox\" \"Firefox, mine\"\n" port)
    (display "                  '(\"firefox\" \"org.mozilla.firefox\")\n" port)
    (display "                  '((\"File\" (\"Open\" \"Ctrl+O\" \"ctrl\" \"o\"))))\n" port)
    (display "(add-menu-app \"Emacsish\" '(\"emacsish\")\n" port)
    (display "              '((\"File\" (\"Save\" \"C-x C-s\"))))\n" port)
    (display "(add-menu-app \"Broken\" '(\"broken\") '((\"File\" (\"No chord\"))))\n" port)))

(check "a settings file with menu entries loads" (load-settings-file! menu-settings))

(check "the file's addition is in the dictionary"
       (not (eq? (menudict-for-id "steam") #f)))
(check "the file's override wins"
       (let ((app (menudict-for-id "org.mozilla.firefox")))
         (and app (string=? (assq-ref app 'name) "Firefox, mine"))))
(check "a multi-key chord is a valid item"
       (not (eq? (menudict-for-id "emacsish") #f)))
(check "the file's bad entry is not in the dictionary"
       (eq? (menudict-for-id "broken") #f))

;; Reported, not dropped in silence: the load says which entry it refused and
;; why, on stderr, where the runtime's other diagnostics go.
(define menu-report
  (let ((port (open-output-string)))
    (parameterize ((current-error-port port)) (load-settings-file! menu-settings))
    (get-output-string port)))

(check "the load reports the bad entry" (string-contains menu-report "Broken"))
(check "the report is a warning"
       (string-contains menu-report "[WARN][whelk][Scheme:Menus]"))

(delete-file menu-settings)

(if (zero? failures)
    (begin (display "ok defaults\n") (exit 0))
    (begin (format (current-error-port) "~a default check(s) failed~%" failures)
           (exit 1)))

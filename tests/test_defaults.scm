;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
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
        (kinds (bar-config-modules-right b))))

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
                              (bar-config-modules-right built-in))))
    (cond ((null? modules) #f)
          ((eq? (shell-module-kind (car modules)) kind) (car modules))
          (else (find (cdr modules))))))

(check "the volume reading is the speaker, not a percentage"
       (equal? (module-option (default-reading 'audio) 'format) "{icon}"))
(check "the brightness reading is the sun, not a percentage"
       (equal? (module-option (default-reading 'backlight) 'format) "{icon}"))

;; The clipboard tile is the shell's own history, not a glyph that opens
;; clipman; its depth is part of the reading.
(check "the clipboard tile shows its depth"
       (string-contains (module-option (default-reading 'clipboard) 'format)
                        "{count}"))

;; What the reference delegated to mako and clipman is native now: no tile on
;; the default bar may spawn either, because neither is installed any more.
(define (default-modules b)
  (append (bar-config-modules-left b)
          (bar-config-modules-center b)
          (bar-config-modules-right b)))

(check "no default tile shells out to mako or clipman"
       (not (any (lambda (m)
                   (let ((command (and (eq? (shell-module-kind m) 'custom)
                                       (module-option m 'exec))))
                     (and command
                          (or (string-contains command "makoctl")
                              (string-contains command "clipman")))))
                 (default-modules built-in))))

;; And what that first menu opens is configuration, not a C table.
(check "the shell menu has the machine's items"
       (let ((units (default-shell-menu)))
         (and (pair? units)
              (equal? (car units) '(item "About This System" "shell:about"))
              (member '(submenu "Recent Items" "shell:recent-items") units)
              (member '(item "Force Quit…" "shell:force-quit") units))))

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
;; ask the shell for their state instead of spawning clipman or makoctl.
(check "the right half keeps the reference's order"
       (equal? (kinds (bar-config-modules-right built-in))
               '(custom clipboard custom dnd language custom custom custom
                 custom custom cpu memory backlight audio keyboard battery
                 power-profile wifi bluetooth tray notifications custom clock)))

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

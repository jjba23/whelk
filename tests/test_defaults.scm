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
             (whelk loader)
             (whelk menudict)
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

;; The bar's left half is a menu bar's: the machine's own menu, the name of the
;; window in front, then that application's File and Edit.  macOS' order, with
;; a whelk where the apple is.
(check "the left half is the shell menu, the window and its menus"
       (equal? (kinds (bar-config-modules-left built-in))
               '(start window menu)))

;; And what that first menu opens is configuration, not a C table.
(check "the shell menu has the machine's items"
       (let ((units (default-shell-menu)))
         (and (pair? units)
              (equal? (car units) '(item "About This System" "shell:about"))
              (member '(submenu "Recent Items" "shell:recent-items") units)
              (member '(item "Force Quit…" "shell:force-quit") units))))

;; The reference's right half, in its order: the media and desktop items, the
;; hardware readouts, the status icons, then search and the clock.
(check "the right half keeps the reference's order"
       (equal? (kinds (bar-config-modules-right built-in))
               '(custom custom custom custom language custom custom custom
                 custom custom cpu memory backlight audio keyboard battery
                 power-profile wifi bluetooth tray notifications custom clock)))

(check "the bar is a top bar"
       (and (eq? (bar-config-edge built-in) 'top)
            (= (bar-config-height built-in) 32)))

;; The example a new machine copies configures that very bar.
(define example (cadr (command-line)))

(check "the example file loads" (load-settings-file! example))

(define bars (get-shell-bars example))

(check "the example configures one bar" (= (length bars) 1))
(check "the example is the default bar" (equal? (shape (car bars))
                                                 (shape built-in)))


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

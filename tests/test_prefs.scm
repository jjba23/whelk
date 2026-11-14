;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
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

(call-with-output-file config-path
  (lambda (port)
    (display "(configure-shell!\n" port)
    (display " #:widgets (list (desktop-widget 'clock #:x 20 #:y 30))\n" port)
    (display " #:bars\n" port)
    (display " (list (bar #:name 'main #:position 'top #:height 32\n" port)
    (display "             #:spacing 8 #:padding 12 #:theme 'gloss-2007\n" port)
    (display "             #:right\n" port)
    (display "             (list (wifi-module #:show-ssid? #f #:interval 5)\n" port)
    (display "                   (notifications-module #:format \"{icon}\")\n" port)
    (display "                   (menu-module #:shortcuts? #t #:app-name? #t\n" port)
    (display "                                #:accelerator-style 'cua)\n" port)
    (display "                   (tray-module #:max-items 6)\n" port)
    (display "                   (clock-module #:format \"%H:%M\")))))\n" port)))

(load-shell-config! config-path)

(check "the file's spacing starts at eight"
       (= (bar-config-spacing (car (get-shell-bars config-path))) 8))

;; A preference is a bar edit.  It must not unregister the desktop widgets the
;; settings file configured: that regression left every card off the desktop
;; as soon as anything was changed in the settings window.
(check "a settings file's widgets are registered"
       (= (length (get-shell-widgets config-path)) 1))
(check "a fresh configuration has no GUI overrides"
       (null? (preferences-read config-path)))
(check "the override file is named after the settings file"
       (string-suffix? "/preferences-settings.scm"
                       (preferences-path config-path)))

;; A GUI edit is written first and survives a reload: this is the property that
;; makes the window useful without ever rewriting the primary settings file.
(save-preference! config-path 'bar-height 44)
(save-preference! config-path 'bar-theme 'frost)
(save-preference! config-path 'bar-edge 'bottom)
(save-preference! config-path 'wifi-show-ssid? #t)
(save-preference! config-path 'wifi-interval 2)
(save-preference! config-path 'notifications-show-count? #t)
(save-preference! config-path 'menu-shortcuts? #f)
(save-preference! config-path 'menu-accelerator-style 'emacs)
(save-preference! config-path 'tray-max-items 9)
(save-preference! config-path 'clock-format "%H:%M:%S")
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
       (and (equal? (bar-config-background applied)
                    (assq-ref frost 'background))
            (equal? (bar-config-foreground applied)
                    (assq-ref frost 'foreground))))
(check "an untouched value still comes from the settings file"
       (= (bar-config-spacing applied) 8))

(define (module-by-kind bar kind)
  (let find ((mods (append (bar-config-modules-left bar)
                            (bar-config-modules-center bar)
                            (bar-config-modules-right bar))))
    (cond ((null? mods) #f)
          ((eq? (shell-module-kind (car mods)) kind) (car mods))
          (else (find (cdr mods))))))

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
(check "the clock format is overridden"
       (equal? (module-option (module-by-kind applied 'clock) 'format)
               "%H:%M:%S"))

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
(check "reset restores the settings reading"
       (equal? (module-option (module-by-kind restored 'notifications) 'format)
               "{icon}"))
(check "reset restores the untouched spacing" (= (bar-config-spacing restored) 8))

(delete-file config-path)
(rmdir (string-append state-dir "/whelk"))
(rmdir state-dir)

(if (zero? failures)
    (begin (display "ok prefs\n") (exit 0))
    (begin (format (current-error-port) "~a preference check(s) failed~%"
                   failures)
           (exit 1)))

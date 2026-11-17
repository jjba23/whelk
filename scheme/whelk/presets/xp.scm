;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; A taskbar in the Windows XP arrangement: Start at the left, the focused
;; window's title in the middle, the notification area and a 24-hour clock at
;; the right, on Luna blue.  It is a preset of ordinary whelk modules, not a
;; separate program: the window title is (window-module), which reads niri and
;; sway over IPC on Wayland and falls back to xprop on X11, so the same bar
;; tracks windows in either session.
;;
;; It is a starting point rather than a finished imitation: a real XP taskbar
;; lists every open window as a button, which whelk does not have a module for
;; yet.  The task-button row is the piece a future `tasks-module' would fill.

;;; Code:

(define-module (whelk presets xp)
  #:use-module (whelk menu)
  #:use-module (whelk modules)
  #:use-module (whelk presets)
  #:use-module (whelk state)
  #:export (xp-bar xp-preset))

(define icon-font "Font Awesome 6 Free Solid")

(define (xp-shell-menu)
  "Return a Start menu in the Windows XP order, using whelk's own actions."
  (list
   (menu-item "All Programs…" "shell:launcher")
   (menu-separator)
   (menu-item "My Computer" "xdg-open \"$HOME\"")
   (menu-item "Control Panel" "shell:settings")
   (menu-item "Search…" "shell:launcher")
   (menu-item "Run…" "shell:launcher")
   (menu-separator)
   (menu-item "About This System" "shell:about")
   (menu-separator)
   (menu-item "Lock" "shell:lock")
   (menu-item "Shut Down…" "systemctl poweroff")))

(define* (xp-bar #:key (name 'xp))
  "Return the XP taskbar: one bottom bar in Luna blue.

The centre is the focused window, so the bar follows the session on Wayland
(niri and sway) and on X11 (xprop) without a second configuration."
  (bar
   #:name name
   #:position 'bottom
   #:height 32
   #:margin 0
   #:exclusive? #t
   #:spacing 2
   #:padding 0
   #:font "Tahoma"
   #:font-size "9pt"
   #:icon-font icon-font

   ;; Windows XP's Luna taskbar: a strong blue surface, white labels and
   ;; square corners, with the theme's default highlight turned down.
   #:background "#245edb"
   #:foreground "#ffffff"
   #:accent "#ffd45e"
   #:border-color "#0b3fa8"
   #:radius 0
   #:opacity 1.0
   #:blur 0
   #:shine 0.35
   #:reflection 0.0
   #:flare 0.0

   #:left (list (start-module #:tooltip "Start"))
   #:center (list (window-module #:format "{title}"
                                 #:max-length 60
                                 #:tooltip "{title}\n{app-id}"))
   #:right (list (tray-module #:max-items 4 #:spacing 2)
                 (wifi-module #:format "{icon}")
                 (battery-module #:format "{icon} {capacity}%")
                 (audio-module)
                 (clock-module #:format "%H:%M"
                               #:tooltip "{calendar}"))))

(define* (xp-preset #:key (name 'xp))
  "Return the Windows-XP-shaped preset: the Luna taskbar and its Start menu."
  (preset #:name name
          #:description
          "A Luna-blue bottom taskbar with Start, the focused window, the notification area and a 24-hour clock."
          #:bars (list (xp-bar #:name name))
          #:widgets '()
          #:shell-menu (xp-shell-menu)))

(register-preset! (xp-preset))

;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; The bar whelk builds when there is no settings file, and the one
;; settings/settings.scm hands out to be copied and edited: the heks-linux
;; Waybar bar's status items, with one application-name menu in place of the
;; separate shell mark and window title.
;;
;; A menu bar's left half is whose window you are in and what can be done to it,
;; so it is one item: the application's short name, whose popover holds the
;; shell's own actions, followed by the application's File/Edit/View.  The
;; status items follow on the right in the reference's order.
;;
;; It is written here once so a fresh machine and the example cannot drift
;; apart.  A settings file that wants this bar says
;;
;;   (configure-shell! #:bars (list (default-bar)))
;;
;; and a file that wants something else writes its own `(bar ...)'.

;;; Code:

(define-module (whelk defaults)
  #:use-module (whelk menu)
  #:use-module (whelk modules)
  #:use-module (whelk state)
  #:export (default-bar default-shell-menu golden-gate-widgets
            default-configuration!))

(define icon-font "Font Awesome 6 Free Solid")

(define (default-shell-menu)
  "Return the shell's own menu: what the mark at the far left opens.

It is a menu bar's first menu, in this desktop's words: what this machine is,
the ways into it, what was opened recently, what to do about a window that will
not leave, and the ways out of the session -- followed by the reload and quit a
configuration needs while it is being written.  That is the Apple menu's shape
and its order; the entries are
this shell's, and every one of them is something it can actually do.

A settings file that wants a different first menu passes its own
`#:shell-menu' to `configure-shell!' -- the list below is not privileged, it is
just the one that ships."
  (let ((user (or (getenv "USER") "")))
    (list
     (menu-item "About This System" "shell:about")
     (menu-item "Control Panel" "shell:settings")
     (menu-item "Software…" "gnome-software")
     (menu-separator)
     (menu-submenu "Recent Items" "shell:recent-items")
     (menu-separator)
     (menu-item "Force Quit…" "shell:force-quit")
     (menu-separator)
     (menu-item "Sleep" "systemctl suspend")
     (menu-item "Restart…" "systemctl reboot")
     (menu-item "Shut Down…" "systemctl poweroff")
     (menu-separator)
     (menu-item "Lock Screen" "shell:lock")
     (menu-item (format #f "Log Out ~a…" user)
                "loginctl terminate-session \"$XDG_SESSION_ID\"")
     (menu-separator)
     (menu-item "Search…" "shell:launcher")
     (menu-separator)
     (menu-item "Reload settings" "whelk --reload")
     (menu-item "Quit whelk" "whelk --quit"))))

(define (default-bar)
  "Return the built-in bar: the heks-linux Waybar bar and the global menu."
  (bar
   #:name 'main
   #:position 'top
   #:monitor "all"
   #:height 32
   #:margin 0
   #:exclusive? #t
   #:spacing 6
   #:padding 12
   #:font "Inter"
   #:font-size "11pt"
   #:icon-font icon-font
   ;; The default material: dark graphite glass with the old top highlight.
   ;; Its CSS squares the menu bar while leaving popovers at the theme's radius.
   #:theme 'gloss-2007

   ;; ---------------------------------------------------------------- left --
   ;; A menu bar's left half: the machine's own menu behind a mark (instead of
   ;; an apple, the whelk), then that application's File, Edit and View.  The
   ;; shell menu is (default-shell-menu) unless the configuration writes its
   ;; own.  A bar that also wants the window's name puts (window-module) back
   ;; between the two; the module is still there for it.
   #:left (list
           (start-module)
           (menu-module #:shortcuts? #t
                        #:app-name? #f
                        #:accelerator-style 'cua))

   ;; -------------------------------------------------------------- center --
   ;; The reference keeps no centre: a menu bar is two halves.  A workspace
   ;; switcher fits here, and is the one module this bar leaves out:
   ;;
   ;;   #:center (list (workspaces-module #:interval 1
   ;;                                     #:hide-empty? #f
   ;;                                     #:format "{index}"))
   #:center '()

   ;; --------------------------------------------------------------- right --
   #:right (list
            ;; Now playing: the glyph, and the track in the tooltip.
            (custom-module #:exec "playerctl metadata --format '{{title}} — {{artist}}' 2>/dev/null || echo 'Nothing playing'"
                           #:interval 2
                           #:max-length 40
                           #:icon "\uf025"
                           #:format "{icon}"
                           #:tooltip "Now playing: {text}
Click: open Orpheus · Right-click: play or pause · Middle: next track"
                           #:on-click "emacsclient -c -a '' --eval \"(progn (require 'orpheus) (orpheus))\""
                           #:on-click-right "playerctl play-pause"
                           #:on-click-middle "playerctl next")

            ;; Clipboard history: its depth, and the picker on a click.
            (clipboard-module #:icon "\uf328"
                              #:format "{icon}"
                              #:tooltip "Clipboard: {count} item(s)
Click: browse the history")

            ;; Password manager.
            (custom-module #:exec "printf '\\uf084'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf084"
                           #:tooltip "1Password: passwords, passkeys and one-time codes
Click: open it"
                           #:on-click "1password --silent")

            ;; Do not disturb: on the bar while whelk is holding notifications.
            (dnd-module #:icon "\uf1f6"
                        #:format "{icon}"
                        #:tooltip "Do not disturb is on — click to let notifications through")

            ;; Keyboard layout: two letters, from niri.
            (language-module #:codes "us,nl"
                             #:interval 2
                             #:format " {text}"
                             #:tooltip "Keyboard layout: {text}
Click: next layout · Right-click: previous layout"
                             #:on-click "niri msg action switch-layout next"
                             #:on-click-right "niri msg action switch-layout prev")

            ;; Desktop appearance, wallpaper, documentation, capture, keys.
            (custom-module #:exec "printf '\\uf53f'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf53f"
                           #:tooltip "Palette: browse the palettes
Click: browse · Right-click: the material settings"
                           #:on-click "heks-toon palettes"
                           #:on-click-right "whelk --settings")
            (custom-module #:exec "printf '\\uf03e'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf03e"
                           #:tooltip "Wallpaper: browse the collection
Click: browse · Right-click: shuffle"
                           #:on-click "heks-toon walls"
                           #:on-click-right "heks-wallpaper random")
            (custom-module #:exec "printf '\\uf518'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf518"
                           #:tooltip "Heks documentation
Click: read · Right-click: the API reference"
                           #:on-click "heks-toon docs"
                           #:on-click-right "xdg-open 'https://jointhefreeworld.org/api-docs/heks-linux/API.html'")
            (custom-module #:exec "printf '\\uf030'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf030"
                           #:tooltip "Capture — screenshot, GIF or video
Click: choose what to take · Right-click: edit the last capture"
                           #:on-click "heks-capture choose"
                           #:on-click-right "heks-capture studio")
            (custom-module #:exec "printf '\\uf059'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf059"
                           #:tooltip "Every keyboard shortcut
Click: the reference · Right-click: the project page"
                           #:on-click "heks-toon keys"
                           #:on-click-right "xdg-open 'https://codeberg.org/jjba23/heks-linux'")

            ;; Hardware readouts.
            (cpu-module #:icon "\uf2db"
                        #:format "{icon} {usage}%"
                        #:interval 2
                        #:tooltip "CPU: {usage}% of every core
Click: system monitor · Right-click: btop"
                        #:on-click "gnome-system-monitor"
                        #:on-click-right "wezterm start -- btop")
            (memory-module #:icon "\uf0c9"
                           #:format "{icon} {percentage}%"
                           #:interval 5
                           #:tooltip "RAM: {used} GiB of {total} GiB ({percentage}%)
Available: {avail} GiB
Swap: {swap-used} GiB of {swap-total} GiB
Click: system monitor · Right-click: btop"
                           #:on-click "gnome-system-monitor"
                           #:on-click-right "wezterm start -- btop")
            (backlight-module #:icon "\uf185"
                              #:format "{icon}"
                              #:interval 2
                              #:tooltip "Brightness: {percent}%
Click: brighter · Right-click: full · Scroll: adjust"
                              #:on-click "brightnessctl set 5%+"
                              #:on-click-right "brightnessctl set 100%"
                              #:on-scroll-up "brightnessctl set 5%+"
                              #:on-scroll-down "brightnessctl set 5%-")
            (audio-module #:icon "\uf028"
                          #:muted-icon "\uf026"
                          #:format "{icon}"
                          #:muted-format "{icon}"
                          #:interval 2
                          #:tooltip "Output: the default sink
Volume: {volume}%
Click: mixer · Right-click: mute · Middle: mute the microphone · Scroll: volume"
                          #:on-click "pavucontrol"
                          #:on-click-right "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"
                          #:on-click-middle "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"
                          #:on-scroll-up "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+"
                          #:on-scroll-down "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-")
            (keyboard-module #:format "⇪"
                             #:interval 1
                             #:tooltip "Caps Lock is on")
            ;; No #:device: the first battery the kernel reports, whether that
            ;; is BAT0, BAT1 or a pack added later.  A desktop hides it.
            (battery-module #:icon "\uf240"
                            #:format "{icon} {capacity}%"
                            #:charging-format "⚡ {capacity}%"
                            #:warning 30
                            #:critical 15
                            #:interval 30
                            #:tooltip "Battery: {capacity}% — {state}"
                            #:on-click "whelk --session-menu")
            (power-profile-module #:icon "\uf0e7"
                                  #:format "{icon}"
                                  #:interval 5
                                  #:tooltip "Power profile: {profile}
Click: next profile"
                                  #:on-click "powerprofilesctl cycle")
            ;; The network, read here rather than by an applet: these two make
            ;; the tray hide nm-applet and Blueman (see #:hide on tray-module).
            ;; The link is shown, not the network's name; #:show-ssid? #t
            ;; names it again.
            (wifi-module #:icon "\uf1eb"
                         #:show-ssid? #f
                         #:disabled-format "{icon} off"
                         #:interval 5)
            (bluetooth-module #:icon "\uf293"
                              #:format "{icon}"
                              #:disabled-format "{icon} off"
                              #:interval 10
                              #:tooltip "Bluetooth: {count} device(s)
Click: devices")
            (tray-module #:spacing 6 #:max-items 6)

            ;; The notifications bell: the centre on a click, do-not-disturb
            ;; on a right click.
            (notifications-module #:icon "\uf0f3"
                                  #:format "{icon}"
                                  #:tooltip "Notifications: {count}
Click: centre · Right-click: do not disturb")

            ;; The two at the far right: search, then the clock.
            (custom-module #:exec "printf '\\uf002'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf002"
                           #:tooltip "Search applications
Click: the launcher · Right-click: switch windows"
                           #:on-click "whelk --launcher"
                           #:on-click-right "whelk --launcher --launcher-mode=window")
            (clock-module #:format "%a %d %b  %H:%M"
                          #:interval 5
                          #:tooltip "{calendar}
Click: calendar · Right-click: copy the time"
                          #:on-click "gnome-calendar"
                          #:on-click-right "wl-copy \"$(date '+%Y-%m-%d %H:%M:%S')\""))))

(define (golden-gate-widgets)
  "Return the standard macOS Golden Gate desktop-widget collection.

The set Apple ships in its widget gallery that has a native source here:
Clock, Calendar, Weather, Batteries, Notes, Reminders, World Clock and Stocks.
They are placed as a starting layout down the left and middle of a 1080p
desktop; every one is an ordinary `(desktop-widget ...)', so a configuration
can move it, resize it, change its options or leave it out.  Weather defaults
to San Francisco because the collection is named for the Golden Gate; set its
latitude, longitude and location to your own.  Stocks default to three US
tickers and can be replaced with any symbol Yahoo Finance knows."
  (list
   (desktop-widget 'clock
                   #:x 32 #:y 64
                   #:options '((style . analog)
                               (show-seconds? . #t)
                               (on-click . "gnome-clocks")))
   (desktop-widget 'calendar
                   #:x 280 #:y 64
                   #:options '((on-click . "gnome-calendar")))
   (desktop-widget 'weather
                   #:x 632 #:y 64
                   #:options '((location . "San Francisco")
                               (latitude . 37.7749)
                               (longitude . -122.4194)
                               (units . metric)))
   (desktop-widget 'battery
                   #:x 32 #:y 320
                   #:options '((on-click . "gnome-power-statistics")))
   (desktop-widget 'notes
                   #:x 32 #:y 520
                   #:width 220
                   #:height 260
                   #:options '((title . "Notes")
                               (text . "Golden Gate widgets\nEdit settings.scm to make this note yours.")))
   (desktop-widget 'reminders
                   #:x 280 #:y 416
                   #:options '((items . ("Review the release"
                                         "Water the plants"
                                         "Read the Guile manual"))))
   (desktop-widget 'world-clock
                   #:x 632 #:y 304
                   #:width 260
                   #:height 220
                   #:options '((cities . "Cupertino=America/Los_Angeles,London=Europe/London,Tokyo=Asia/Tokyo")))
   (desktop-widget 'stocks
                   #:x 632 #:y 536
                   #:width 300
                   #:height 220
                   #:options '((symbols . ("AAPL" "MSFT" "NVDA"))))))

(define (default-configuration!)
  "Register the built-in bar and Golden Gate widget set as the fallback."
  (configure-shell! #:config-path 'default
                    #:bars (list (default-bar))
                    #:widgets (golden-gate-widgets)))

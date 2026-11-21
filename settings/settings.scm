;; whelk -- example configuration
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>
;;
;; A 1:1 replacement for the heks-linux Waybar bar: one application-name menu
;; on the left (the shell's own actions and the focused application's
;; File/Edit/View), the status items on the right, in the order the reference
;; keeps them.  This is the same bar whelk builds with no settings file --
;; (default-bar) from (whelk defaults) -- so a fresh machine starts here instead
;; of on a placeholder.
;;
;; Copy this file to ~/.config/whelk/settings.scm and change it there; whelk
;; reloads the bar when the file is saved.  This is the `current' preset written
;; out: to switch to another one instead, replace the call below with
;; (use-preset! 'xp) -- see docs/configuration.org, "Presets".

(define icon-font "Font Awesome 6 Free Solid")

(configure-shell!
 #:bars
 (list
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
   ;; The same dark graphite glass as (default-bar); its CSS keeps the menu
   ;; bar square and lets popovers use the theme's rounded corners.
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
   ;; switcher fits here, and is the one module this file leaves out:
   ;;
   ;;   #:center (list (workspaces-module #:interval 1
   ;;                                     #:hide-empty? #f
   ;;                                     #:format "{index}"))
   #:center '()

   ;; --------------------------------------------------------------- right --
   #:right (list
            ;; Now playing: the player spoken to directly over MPRIS, so the
            ;; tile is live and there is no playerctl in the middle.
            (mpris-module #:icon "\uf025"
                          #:format "{icon}"
                          #:tooltip "Now playing: {title} — {artist}
{position} / {length} · {state}
Click: open Orpheus · Middle: next · Right-click: previous"
                          #:on-click "emacsclient -c -a '' --eval \"(progn (require 'orpheus) (orpheus))\"")

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


            ;; Capture and the keyboard-shortcut reference.
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
            (memory-module #:icon "\uf0c9"
                           #:format "{icon} {percentage}%"
                           #:interval 5
                           #:tooltip "RAM: {used} GiB of {total} GiB ({percentage}%)
Available: {avail} GiB
Swap: {swap-used} GiB of {swap-total} GiB
Click: system monitor · Right-click: btop"
                           #:on-click "gnome-system-monitor"
                           #:on-click-right "wezterm start -- btop")
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
            ;; Auto-detect the pack: BAT0 on many machines, BAT1 on others.
            (battery-module #:icon "\uf240"
                            #:charging-icon "\uf0e7"
                            #:format "{icon} {capacity}%"
                            #:show-capacity-when-charging? #f
                            #:warning 30
                            #:critical 15
                            #:interval 30
                            #:tooltip "Battery: {capacity}% — {state}
{detail}"
                            #:on-click "whelk --session-menu")
            ;; A leaf, not a bolt: the battery wears the bolt while it charges,
            ;; and two bolts a few pixels apart read as one broken thing.  The
            ;; click is the module's own now -- next profile, and the right
            ;; button lists them all.
            (power-profile-module #:icon "\uf06c"
                                  #:format "{icon}"
                                  #:interval 5
                                  #:tooltip "Power profile: {profile}
Click: next profile · Right-click: all profiles")
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
                              #:tooltip "Bluetooth: {count} device(s)\nClick: devices")
            (tray-module #:spacing 6 #:max-items 6)

            ;; The notifications bell: the centre on a click, do-not-disturb
            ;; on a right click.
            (notifications-module #:icon "\uf0f3"
                                  #:format "{icon}"
                                  #:tooltip "Notifications: {count}
Click: centre · Right-click: do not disturb")

            ;; The wallpapers, picked by whelk itself: a click opens the card
            ;; of thumbnails, a right click the full picker.  No heks-toon, no
            ;; second program -- the picture is applied through whelk's own
            ;; GUI preference.
            (wallpaper-module #:icon "\uf03e"
                              #:format "{icon}"
                              #:tooltip "Wallpaper: pick one
Click: the wallpapers · Right-click: the full picker")

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
                          #:on-click-right "wl-copy \"$(date '+%Y-%m-%d %H:%M:%S')\""))

   ;; ------------------------------------------------------------- overflow --
   ;; Reference, not status: the load, the ways into the machine's own tools,
   ;; and the two hardware dials that are set once in a while.  They are built
   ;; with the bar but stay behind a chevron at the end of the right row, and
   ;; open horizontally when it is clicked.  #:overflow-expanded? #t opens the
   ;; area at start.
   #:overflow (list
               (cpu-module #:icon "\uf2db"
                           #:format "{icon} {usage}%"
                           #:interval 2
                           #:tooltip "CPU: {usage}% of every core
Click: system monitor · Right-click: btop"
                           #:on-click "gnome-system-monitor"
                           #:on-click-right "wezterm start -- btop")
               (custom-module #:exec "printf '\\uf518'"
                              #:interval 0
                              #:format "{icon}"
                              #:icon "\uf518"
                              #:tooltip "Heks documentation
Click: read · Right-click: the API reference"
                              #:on-click "heks-toon docs"
                              #:on-click-right "xdg-open 'https://jointhefreeworld.org/api-docs/heks-linux/index.html'")
               ;; Leaving the language reading to its own gestures: a click
               ;; switches to the next layout, the right button lists them all.
               (language-module #:codes "us,nl"
                                #:interval 2
                                #:format " {text}"
                                #:tooltip "Keyboard layout: {name}
Click: next layout · Right-click: all layouts")
               ;; Reference, not status: the characters a layout has no key
               ;; for -- accented letters, emoji, ASCII smiles and kawaii
               ;; faces in one card.  A click opens it and clicking a
               ;; character copies it, so the paste is the very next thing;
               ;; #:sections trims the card to the parts a reader uses.
               ;; Uncomment to put it on the shelf.
               ;; (character-module #:icon "☺"
               ;;                   #:format "{icon}"
               ;;                   #:sections "accents,emoji,smiles,kawaii"
               ;;                   #:tooltip "Characters: letters, emoji, smiles")
               (backlight-module #:icon "\uf185"
                                 #:format "{icon}"
                                 #:interval 2
                                 #:tooltip "Brightness: {percent}%
Click: brighter · Right-click: full · Scroll: adjust"
                                 #:on-click "brightnessctl set 5%+"
                                 #:on-click-right "brightnessctl set 100%"
                                 #:on-scroll-up "brightnessctl set 5%+"
                                 #:on-scroll-down "brightnessctl set 5%-"))))

 ;; Idle locking.  0 (the default) locks as soon as logind reports the
 ;; session idle, 300 waits five minutes, -1 never locks on idle.  The lock
 ;; key, loginctl and suspend still work either way.
 ;; #:idle (idle-settings #:lock-after 300)

 ;; Night light: warm the screen when the day is over.  The display server's
 ;; gamma tables are moved towards 3400 K from 19:00, faded back by 07:00, and
 ;; the same policy is on the settings window's Night light page.  Set
 ;; #:enabled? to #t to turn it on; 6500 K is neutral and lower is warmer.
 #:night-light (night-light #:enabled? #f
                           #:temperature 3400
                           #:start "19:00"
                           #:end "07:00"
                           #:transition 30)

 ;; Wallpaper engine.  A path can be a directory (walked recursively) or a
 ;; single file, and ~ is expanded.  The engine draws behind the bar on every
 ;; monitor, rotates through the collection every 900 seconds and crossfades
 ;; between pictures.  PNG, JPEG, WebP, AVIF, GIF and SVG stills are decoded
 ;; by gdk-pixbuf; GIF/WebP animations and videos loop too.  Set #:enabled? to
 ;; #f to leave the compositor's own background alone.  The picker is
 ;; `whelk --wallpaper-picker`; bind that command in the compositor.
 #:wallpaper (wallpaper-settings #:enabled? #t
                                #:paths (list "~/Pictures/Wallpapers")
                                #:interval 900
                                #:shuffle? #t
                                #:animation 'crossfade
                                #:animation-duration 1200
                                #:fit 'cover
                                #:ken-burns? #f
                                #:video? #t
                                #:mute-video? #t
                                #:picker-width 960
                                #:picker-height 640
                                #:picker-columns 4)

 ;; The standard macOS Golden Gate desktop-widget collection: Clock,
 ;; Calendar, Weather, Batteries, Notes, Reminders, World Clock and Stocks.
 ;; Each is a (desktop-widget ...) record, so remove, move or resize them here.
 #:widgets (golden-gate-widgets))

;; whelk -- example configuration
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>
;;
;; A 1:1 replacement for the heks-linux Waybar bar: the machine's own menu, the
;; focused window and the global menu on the left, the status items on the
;; right, in the order the reference keeps them.  This is the same bar whelk
;; builds with no settings file -- (default-bar) from (whelk defaults) -- so a
;; fresh machine starts here instead of on a placeholder.
;;
;; Copy this file to ~/.config/whelk/settings.scm and change it there; whelk
;; reloads the bar when the file is saved.

(define icon-font "Font Awesome 6 Free Solid")

(define (rofi-relaunch command)
  "Run COMMAND after handing any open rofi's seat to it."
  (format #f "pkill rofi 2>/dev/null; ~a" command))

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
   #:foreground "#f2f3f5"
   #:background "rgba(27, 27, 29, 0.82)"
   #:accent "#8fd16a"
   #:border-color "rgba(255, 255, 255, 0.10)"
   #:radius 0

   ;; ---------------------------------------------------------------- left --
   ;; A menu bar's left half is the shell's own menu and what is in front of
   ;; you.  The mark is the whelk, drawn in the bar's own ink; #:label swaps it
   ;; for any glyph the icon font has.
   #:left (list
           ;; The shell's own menu, natively: one glyph, and behind it the
           ;; launcher, the panels, a reload and the way out.  No rofi, no
           ;; subprocess to hand the click to -- the popover is GTK and every
           ;; entry in it is the shell's own.
           (start-module #:tooltip "whelk — the shell's own menu
Click: the launcher, the panels, a reload and the way out")
           (window-module #:format "{title}"
                          #:max-length 50
                          #:interval 1
                          #:tooltip "{app-id}
{title}"
                          #:on-click "niri msg action focus-workspace-next")
           ;; The global menu: the application in front's own File, Edit and
           ;; View, from its D-Bus menubar where it exports one and from its
           ;; desktop actions and the shared shortcuts where it does not.
           (menu-module #:shortcuts #t
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
            ;; Now playing: the glyph, and the track in the tooltip.
            (custom-module #:exec "playerctl metadata --format '{{title}} — {{artist}}' 2>/dev/null || echo 'Nothing playing'"
                           #:interval 2
                           #:max-length 40
                           #:icon "\uf025"
                           #:format "{icon}"
                           #:tooltip "Now playing: {text}
Click: transport controls · Right-click: play or pause · Middle: next track"
                           #:on-click (rofi-relaunch "heks-media-menu")
                           #:on-click-right "playerctl play-pause"
                           #:on-click-middle "playerctl next")

            ;; Clipboard history.
            (custom-module #:exec "printf '\\uf328'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf328"
                           #:tooltip "Clipboard history
Click: browse the history"
                           #:on-click (rofi-relaunch "clipman pick --tool=rofi"))

            ;; Password manager.
            (custom-module #:exec "printf '\\uf084'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf084"
                           #:tooltip "1Password: passwords, passkeys and one-time codes
Click: open it"
                           #:on-click (rofi-relaunch "1password --silent"))

            ;; Do not disturb: only on the bar while mako is quiet.
            (custom-module #:exec "makoctl mode 2>/dev/null | grep -q do-not-disturb && printf '\\uf1f6'"
                           #:interval 3
                           #:format "{icon}"
                           #:icon "\uf1f6"
                           #:tooltip "Do not disturb is on — click to let notifications through"
                           #:on-click "makoctl mode -t do-not-disturb")

            ;; Keyboard layout: two letters, from niri.
            (language-module #:codes "us,nl"
                             #:interval 2
                             #:format " {text}"
                             #:tooltip "Keyboard layout: {text}
Click: choose a layout · Right-click: previous layout"
                             #:on-click (rofi-relaunch "heks-language-menu")
                             #:on-click-right "niri msg action switch-layout prev")

            ;; Desktop appearance, wallpaper, documentation, capture, keys.
            (custom-module #:exec "printf '\\uf53f'"
                           #:interval 0
                           #:format "{icon}"
                           #:icon "\uf53f"
                           #:tooltip "Palette: browse the palettes
Click: browse · Right-click: the quick picker"
                           #:on-click "heks-toon palettes"
                           #:on-click-right (rofi-relaunch "heks-theme-menu"))
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
                              #:format "{icon} {percent}%"
                              #:interval 2
                              #:tooltip "Brightness: {percent}%
Click: brighter · Right-click: full · Scroll: adjust"
                              #:on-click "brightnessctl set 5%+"
                              #:on-click-right "brightnessctl set 100%"
                              #:on-scroll-up "brightnessctl set 5%+"
                              #:on-scroll-down "brightnessctl set 5%-")
            (audio-module #:icon "\uf0a1"
                          #:muted-icon "\uf6a9"
                          #:format "{icon} {volume}%"
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
            (battery-module #:device "BAT0"
                            #:icon "\uf240"
                            #:format "{icon} {capacity}%"
                            #:charging-format "⚡ {capacity}%"
                            #:warning 30
                            #:critical 15
                            #:interval 30
                            #:tooltip "Battery: {capacity}% — {state}"
                            #:on-click (rofi-relaunch "rofi -show power"))
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
                              #:format "{icon} {count}"
                              #:disabled-format "{icon} off"
                              #:interval 10
                              #:tooltip "Bluetooth: {count} device(s)\nClick: devices")
            (tray-module #:spacing 6)

            ;; The notifications bell: the centre on a click, do-not-disturb
            ;; on a right click.
            (notifications-module #:icon "\uf0f3"
                                  #:format "{icon} {count}"
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
                          #:on-click-right "wl-copy \"$(date '+%Y-%m-%d %H:%M:%S')\"")))))

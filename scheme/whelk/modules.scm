;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; The module half of the configuration DSL: a <shell-module> is a kind symbol
;; plus an alist of options, and every constructor below is one line over the
;; same record.  That shape is deliberate -- the C side reads a kind and an
;; option table, so a new module needs a new C factory and nothing here, and an
;; option the C side has never heard of is simply ignored instead of becoming a
;; configuration error.
;;
;; Defaults live on the C side, next to the module that reads them (the same
;; contract as lambdock's bridge), because the shell has to run with no Scheme
;; at all: a missing option and a settings file that failed to load are the
;; same thing to a module.
;;
;; Every constructor takes the same base keys:
;;
;;   #:icon            a glyph, drawn from the bar's icon font
;;   #:format          how the reading is rendered; {icon} is the glyph
;;   #:interval        seconds between refreshes (0 = render once)
;;   #:tooltip         hover text; {text} is the current reading
;;   #:on-click        #:on-click-right  #:on-click-middle
;;   #:on-scroll-up    #:on-scroll-down
;;   #:css-class       extra CSS class on the module's own button
;;
;; @example
;; (cpu-module #:icon "\uf2db" #:format "{icon} {usage}%" #:interval 2)
;; @end example

;;; Code:

(define-module (whelk modules)
  #:use-module (srfi srfi-1)
  #:use-module (srfi srfi-9)
  #:use-module (whelk logger)
  #:export (<shell-module>
            shell-module?
            shell-module-kind
            shell-module-options
            module
            module-option
            module-options-ref
            module->alist
            clock-module
            cpu-module
            memory-module
            battery-module
            backlight-module
            audio-module
            power-profile-module
            workspaces-module
            window-module
            language-module
            keyboard-module
            custom-module
            wifi-module
            bluetooth-module
            notifications-module
            tray-module
            start-module
            menu-module))

;; =============================================================================
;; The record
;; =============================================================================

(define-record-type <shell-module>
  (make-shell-module kind options)
  shell-module?
  (kind shell-module-kind)
  (options shell-module-options))

(define (option-pairs kv)
  "Return KEYWORD VALUE ... as an alist of (symbol . value), dropping #f values.

A #f is how a configuration says \"leave this one to the module\", which is not
the same as \"set it to false\": dropping it here means the C side sees a missing
option and applies its own default."
  (let loop ((rest kv) (acc '()))
    (cond
      ((null? rest) (reverse acc))
      ((null? (cdr rest))
       (log-warn "Modules" "Odd number of keyword arguments: ~s" rest)
       (reverse acc))
      (else
       (let ((key (car rest)) (value (cadr rest)))
         (loop (cddr rest)
               (if (eq? value #f)
                   acc
                   (cons (cons (if (keyword? key)
                                   (keyword->symbol key)
                                   key)
                               value)
                         acc))))))))

(define* (module kind #:rest kv)
  "Build a <shell-module> of KIND from KEYWORD VALUE ... pairs.

This is the open constructor: every named constructor below is this call with
its kind spelled out, so a module can be named without waiting for a Scheme
wrapper."
  (when (not (symbol? kind))
    (log-warn "Modules" "Module kind should be a symbol, got: ~s" kind))
  (make-shell-module kind (option-pairs kv)))

(define (module-option mod key)
  "Return the value of KEY in MOD, or #f when it has none."
  (let ((pair (assq key (shell-module-options mod))))
    (if pair (cdr pair) #f)))

(define (module->alist mod)
  "Return MOD as a plain alist for the C runtime.

SRFI-9's accessors are macros, and a macro is not something the C bridge can
call, so the bridge asks for this shape instead: a kind and an option alist, read
with `assq' on either side and with no record syntax crossing the boundary."
  (list (cons 'kind (shell-module-kind mod))
        (cons 'options (shell-module-options mod))))

(define (module-options-ref mod key default)
  "Return the value of KEY in MOD, or DEFAULT when it has none."
  (let ((pair (assq key (shell-module-options mod))))
    (if pair (cdr pair) default)))

;; =============================================================================
;; The modules
;; =============================================================================
;;
;; One line each, so the set below reads as the bar's alphabet.  The docstrings
;; name the options that are not part of the base set in the header.

(define* (clock-module #:rest kv)
  "The clock.  #:format is a strftime(3) format (default \"%a %d %b  %H:%M\");
#:format-alt is the reading a click alternates to; #:tooltip may ask for
{calendar} to get the current month laid out under the reading."
  (apply module 'clock kv))

(define* (cpu-module #:rest kv)
  "CPU usage, sampled from /proc/stat.  #:format uses {usage} (percent)."
  (apply module 'cpu kv))

(define* (memory-module #:rest kv)
  "Memory usage, read from /proc/meminfo.  #:format uses {percentage}, {used},
{total}, {avail}, {swap-used} and {swap-total}."
  (apply module 'memory kv))

(define* (battery-module #:rest kv)
  "A battery from /sys/class/power_supply.  #:device names the pack (BAT0 by
default); #:format uses {capacity} and {state}; #:warning and #:critical are the
percentages that switch the readout's colour."
  (apply module 'battery kv))

(define* (backlight-module #:rest kv)
  "Screen brightness from /sys/class/backlight.  #:device names the panel;
#:format uses {percent}."
  (apply module 'backlight kv))

(define* (audio-module #:rest kv)
  "The default sink's volume, read with wpctl.  #:format uses {volume} and
{icon}; #:muted-format replaces it while the sink is muted."
  (apply module 'audio kv))

(define* (power-profile-module #:rest kv)
  "power-profiles-daemon's active profile.  #:format uses {profile}."
  (apply module 'power-profile kv))

(define* (workspaces-module #:rest kv)
  "niri's workspaces as buttons.  #:hide-empty? leaves out the workspaces that
have no window; #:format is the label ({index} or {name})."
  (apply module 'workspaces kv))

(define* (window-module #:rest kv)
  "The focused window's title.  #:format uses {title} and {app-id};
#:max-length truncates the title (0 = no limit)."
  (apply module 'window kv))

(define* (start-module #:rest kv)
  "The shell's own menu: the launcher, the panels, the settings and quitting,
behind one mark.  The menu is a GTK popover the shell builds itself -- no rofi,
no subprocess.  It wears the whelk, drawn in the bar's ink; #:label replaces it
with a glyph from the bar's icon font, #:icon-size sets the mark's pixels (18 by
default), and #:tooltip is its hover text."
  (apply module 'start kv))

(define* (menu-module #:rest kv)
  "The global menu: the focused application's File/Edit/View menus.  Read from
its D-Bus menubar where it exports one; otherwise from the dictionary's menus
for the applications whose shortcuts are documented; otherwise from the
shortcuts every application shares.  The base keys apply; #:shortcuts? (default
on) offers the shortcut menus at all, and #:shortcut-tool names the program
that types a chord (wtype on Wayland, xdotool on X11).  #:accelerator-style
chooses how every chord is printed -- 'cua for Ctrl+Shift+P, 'emacs for
C-S-p -- and #:app-name? (default on) puts the application's name before its
menus, the way a menu bar names the application first."
  (apply module 'menu kv))

(define* (language-module #:rest kv)
  "The active keyboard layout.  #:codes is the comma-separated XKB code list
(\"us,nl\"), in niri's order, so the label is two letters rather than a name."
  (apply module 'language kv))

(define* (keyboard-module #:rest kv)
  "Caps Lock, read from the input LED.  #:format uses {state}."
  (apply module 'keyboard kv))

(define* (custom-module #:rest kv)
  "A reading produced by a shell command.  #:exec is the command; its first line
of standard output is the reading; #:interval 0 renders it once."
  (apply module 'custom kv))

(define* (wifi-module #:rest kv)
  "Wi-Fi, read from NetworkManager with nmcli.  #:show-ssid? (default off)
chooses whether the connected reading and its tooltip name the network; with
it off they show the icon and the signal only.  #:format uses {ssid}, {signal}
and {state}, and an explicit #:format or #:tooltip still wins.
#:disabled-format and #:disconnected-format are the readings when the radio is
off or nothing is associated.  A left click opens the network panel, where a network is chosen
and a passphrase typed."
  (apply module 'wifi kv))

(define* (bluetooth-module #:rest kv)
  "The Bluetooth adapter and its connected devices, read with bluetoothctl.
#:format uses {count} and {state}; #:disabled-format is the reading when the
adapter is off.  A left click opens the panel, where devices connect."
  (apply module 'bluetooth kv))

(define* (notifications-module #:rest kv)
  "The notification bell: how many notifications are waiting.  #:format uses
{count} and {state}.  A left click opens the notification centre, a right click
toggles do-not-disturb."
  (apply module 'notifications kv))

(define* (tray-module #:rest kv)
  "The StatusNotifierItem tray: other programs' status icons.  #:spacing is the
pixels between icons; #:hide is a comma-separated list of id/title fragments to
leave out.  Without #:hide, applets for what whelk reads natively (a Wi-Fi or
Bluetooth applet, when the bar has those modules) are hidden."
  (apply module 'tray kv))

;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; The shipped configuration as one preset: the heks-linux Waybar bar (global
;; menu, status items, overflow shelf) and the Golden Gate desktop widgets.
;; It is the same bundle as `default-configuration!' and the bar described by
;; settings/settings.scm; the defaults test is what keeps the three together.

;;; Code:

(define-module (whelk presets current)
  #:use-module (whelk defaults)
  #:use-module (whelk presets)
  #:export (current-preset))

(define* (current-preset #:key (name 'current))
  "Return the preset for whelk's current settings.

It is `(default-bar)' -- the top bar with the shell menu, the application's
menus and the status items -- and `(golden-gate-widgets)' on the desktop."
  (preset #:name name
          #:description
          "The heks-linux Waybar bar and the Golden Gate desktop widgets."
          #:bars (list (default-bar))
          #:widgets (golden-gate-widgets)))

(register-preset! (current-preset))

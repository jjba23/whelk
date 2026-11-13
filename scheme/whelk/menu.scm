;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; A menu, in four forms.  The shell's own menu is configuration -- the popover
;; behind the mark at the far left of the bar is what a settings file says it
;; is, not a table compiled into the binary -- so the shape it is written in
;; lives here, next to the module constructors it sits beside.
;;
;;   (menu-item "About This System" "shell:about")
;;   (menu-separator)
;;   (menu-submenu "Recent Items" "shell:recent-items")
;;   (menu-submenu "Open Recent"
;;                 (menu-item "notes.org" "xdg-open ~/notes.org"))
;;
;; An action is one of two things, and the spelling is the whole difference: a
;; string beginning with `shell:' is something the shell itself does -- show the
;; launcher, open the network panel, offer the open windows to close -- and
;; anything else is a command line, run the way every other click on the bar
;; runs one.  A submenu given a string instead of children is a slot the shell
;; fills when it opens, which is how a menu holds something that is only known
;; at the moment somebody looks at it.
;;
;; The units are plain lists rather than records on purpose: they cross into C
;; as data, and a list a person can read in a settings file is a list they can
;; also generate with `map' and a `lambda'.

;;; Code:

(define-module (whelk menu)
  #:export (menu-item menu-separator menu-submenu))

(define (menu-item label action)
  "One row: LABEL, and what choosing it does.

ACTION is `shell:NAME' for something the shell does itself, or a command line.
A label with no action is shown disabled, because a row that cannot say what it
does should not pretend to."
  (list 'item label (if action action "")))

(define (menu-separator)
  "A rule between two groups of rows."
  (list 'separator))

(define* (menu-submenu label . units)
  "A row that opens more rows: LABEL, then UNITS.

With a single string instead of units, it is a slot the shell fills when the
submenu opens: `(menu-submenu \"Recent Items\" \"shell:recent-items\")'."
  (cons* 'submenu label units))

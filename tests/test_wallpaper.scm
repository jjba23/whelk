;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The wallpaper is configuration before it is a surface: a path list, a
;; rotation interval, a transition and a picker size.  These checks pin the
;; constructor's normalisation, the alist the bridge reads, and the fact that
;; saving an unrelated GUI preference does not drop the wallpaper policy.

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
  (string-append root "/whelk-wallpaper-test-"
                 (number->string (current-time))))

(mkdir state-dir)
(setenv "XDG_STATE_HOME" state-dir)
(setenv "HOME" state-dir)

(define default (wallpaper-settings))

(check "the wallpaper is off until it is configured"
       (and (eq? (assq-ref default 'enabled?) #f)
            (eq? (assq-ref default 'configured?) #t)))

(check "the default is a fifteen-minute crossfade"
       (and (= (assq-ref default 'interval) 900)
            (eq? (assq-ref default 'animation) 'crossfade)
            (= (assq-ref default 'animation-duration) 1200)
            (eq? (assq-ref default 'fit) 'cover)))

(define custom
  (wallpaper-settings #:enabled? #t
                      #:paths (list "~/Walls" "/srv/walls" "")
                      #:interval 300
                      #:shuffle? #f
                      #:animation 'slide-left
                      #:animation-duration 800
                      #:fit 'contain
                      #:ken-burns? #t
                      #:video? #f
                      #:current "~/Walls/start.avif"
                      #:picker-width 1200
                      #:picker-height 700
                      #:picker-columns 5))

(check "a home-relative path is expanded"
       (equal? (assq-ref custom 'paths)
               (list (string-append state-dir "/Walls") "/srv/walls")))
(check "the rotation and transition are kept"
       (and (= (assq-ref custom 'interval) 300)
            (eq? (assq-ref custom 'shuffle?) #f)
            (eq? (assq-ref custom 'animation) 'slide-left)
            (= (assq-ref custom 'animation-duration) 800)
            (eq? (assq-ref custom 'fit) 'contain)
            (eq? (assq-ref custom 'ken-burns?) #t)
            (eq? (assq-ref custom 'video?) #f)))
(check "the starting picture is expanded too"
       (equal? (assq-ref custom 'current)
               (string-append state-dir "/Walls/start.avif")))
(check "the picker geometry is kept"
       (and (= (assq-ref custom 'picker-width) 1200)
            (= (assq-ref custom 'picker-height) 700)
            (= (assq-ref custom 'picker-columns) 5)))

;; A typo is a default, not a broken shell.
(define typo
  (wallpaper-settings #:animation 'explode #:fit 'stretch
                      #:interval -4 #:picker-columns 40))

(check "an unknown animation falls back to crossfade"
       (eq? (assq-ref typo 'animation) 'crossfade))
(check "an unknown fit falls back to cover"
       (eq? (assq-ref typo 'fit) 'cover))
(check "a negative interval stops the clock"
       (= (assq-ref typo 'interval) 0))
(check "the column count is clamped"
       (= (assq-ref typo 'picker-columns) 12))

;; The shape the C bridge reads.
(define alist (shell-wallpaper->alist))
(check "the bridge sees an empty policy before one is registered"
       (null? alist))

(define config-path (string-append state-dir "/settings.scm"))

(define (write-config!)
  (call-with-output-file config-path
    (lambda (port)
      (display "(configure-shell!\n" port)
      (display " #:bars (list (bar #:name 'main #:right (list (clock-module))))\n" port)
      (display " #:wallpaper (wallpaper-settings #:enabled? #t\n" port)
      (display "                                   #:paths (list \"/walls\")\n" port)
      (display "                                   #:interval 60\n" port)
      (display "                                   #:animation 'zoom\n" port)
      (display "                                   #:fit 'fill))\n" port))))

(write-config!)
(load-shell-config! config-path)

(check "configure-shell! stores the wallpaper"
       (let ((wallpaper (get-shell-wallpaper config-path)))
         (and (eq? (assq-ref wallpaper 'enabled?) #t)
              (equal? (assq-ref wallpaper 'paths) (list "/walls"))
              (= (assq-ref wallpaper 'interval) 60)
              (eq? (assq-ref wallpaper 'animation) 'zoom)
              (eq? (assq-ref wallpaper 'fit) 'fill))))

(check "the bridge view carries the policy"
       (let ((view (shell-wallpaper->alist config-path)))
         (and (eq? (assq-ref view 'enabled?) #t)
              (equal? (assq-ref view 'paths) (list "/walls")))))

;; A GUI save rebuilds the shell record.  It must not forget the background.
(check "a GUI preference keeps the wallpaper"
       (and (save-preference! config-path 'bar-height 40)
            (begin
              (load-shell-config! config-path)
              (let ((wallpaper (get-shell-wallpaper config-path)))
                (and (eq? (assq-ref wallpaper 'enabled?) #t)
                     (= (assq-ref wallpaper 'interval) 60))))))

(if (> failures 0)
    (begin
      (format (current-error-port) "~a wallpaper test(s) failed~%" failures)
      (exit 1))
    (begin
      (display "wallpaper: all checks passed\n")
      (exit 0)))

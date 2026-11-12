;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The menu dictionary is data, and data goes wrong in ways the C tests cannot
;; see: an id two applications both claim, an entry whose chord was never
;; written, a menu with a label and nothing behind it.  The runtime test loads
;; the same table and asks it what a compositor would; this one reads it.

(use-modules (whelk menudict)
             (srfi srfi-1))

(define failures 0)

(define (check name condition)
  (unless condition
    (set! failures (+ failures 1))
    (format (current-error-port) "not ok ~a~%" name)))

(define (non-empty-string? value)
  (and (string? value) (> (string-length value) 0)))

(define apps (menudict->alists))

;; The dictionary has to be worth having: the applications a desktop actually
;; runs, not a handful of examples.
(check "the dictionary knows many applications" (>= (length apps) 35))

;; Names are what the menu's log line says, so two entries cannot share one.
(let ((names (map (lambda (app) (assq-ref app 'name)) apps)))
  (check "every application is named"
         (every non-empty-string? (map (lambda (app) (assq-ref app 'name)) apps)))
  (check "names are unique" (= (length names) (length (delete-duplicates names)))))

;; An id belongs to one application: a lookup returns the first match, so a
;; second claim on the same id would silently shadow the other entry.
(define ids (append-map (lambda (app) (assq-ref app 'ids)) apps))

(check "every application has ids" (every (lambda (app) (pair? (assq-ref app 'ids))) apps))
(check "ids are non-empty strings" (every non-empty-string? ids))
(check "ids are unique" (= (length ids) (length (delete-duplicates ids))))
(check "ids carry no whitespace"
       (every (lambda (id) (not (string-any char-set:whitespace id))) ids))
(check "ids differ even ignoring case"
       (= (length ids) (length (delete-duplicates (map string-downcase ids)))))

;; A menu needs a label and at least one thing in it; an item needs the four
;; fields the runtime reads, and a chord it can actually type.
(define (check-menu app label menu)
  (let ((name (assq-ref app 'name))
        (items (cdr menu)))
    (check (format #f "~a: menu label" name) (non-empty-string? label))
    (check (format #f "~a > ~a has items" name label) (pair? items))

    (for-each
     (lambda (item)
       (let ((where (format #f "~a > ~a > ~a" name label (car item))))
         (check (format #f "~a has four fields" where) (= (length item) 4))
         (check (format #f "~a is labelled" where) (non-empty-string? (list-ref item 0)))
         (check (format #f "~a shows a chord" where) (non-empty-string? (list-ref item 1)))
         (check (format #f "~a has modifiers or #f" where)
                (or (eq? (list-ref item 2) #f)
                    (non-empty-string? (list-ref item 2))))
         (check (format #f "~a has a key" where) (non-empty-string? (list-ref item 3)))))
     items)

    ;; Two entries that type the same chord in one menu: one of them is a typo,
    ;; and the other would be unreachable.
    (let ((chords (map (lambda (item) (cons (list-ref item 2) (list-ref item 3))) items)))
      (check (format #f "~a > ~a has no duplicate chords" name label)
             (= (length chords) (length (delete-duplicates chords)))))))

(for-each
 (lambda (app)
   (let ((name (assq-ref app 'name))
         (menus (assq-ref app 'menus)))
     (check (format #f "~a has menus" name) (pair? menus))
     (for-each (lambda (menu) (check-menu app (car menu) menu)) menus)))
 apps)

(if (> failures 0)
    (begin
      (format (current-error-port) "~a dictionary checks failed~%" failures)
      (exit 1)))

(format #t "the dictionary: ~a applications, ~a ids, ~a menus~%"
        (length apps)
        (length ids)
        (apply + (map (lambda (app) (length (assq-ref app 'menus))) apps)))

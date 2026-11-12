;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The menu dictionary is data, and data goes wrong in ways the C tests cannot
;; see: an id two applications both claim, an entry whose chord was never
;; written, a menu with a label and nothing behind it.  The runtime test loads
;; the same table and asks it what a compositor would; this one reads it.

(use-modules (whelk menudict)
             (srfi srfi-1)
             (srfi srfi-13))

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


;; ============================================================================
;; What a settings file may add or replace
;; ============================================================================

;; Every settings file starts from the built-in table.
(reset-menu-edits!)

(check "with no edits the table is the built-in one"
       (= (length (menudict->alists)) (length menudict)))

;; --- an addition ------------------------------------------------------------
;; An installed application the table does not know is appended, and found by
;; the id a compositor would report for it.

(define steam (add-menu-app "Steam" '("steam" "com.valvesoftware.Steam")
                            '(("File" ("Settings" "Ctrl+," "ctrl" "comma"))
                              ("View" ("Full Screen" "F11" #f "F11")))))

(check "a settings file can add an application" (not (eq? steam #f)))
(check "the addition answers to its id" (eq? (menudict-for-id "steam") steam))
(check "the addition answers to its flatpak id as well"
       (eq? (menudict-for-id "com.valvesoftware.Steam") steam))
(check "the addition is appended after the built-in table"
       (= (length (menudict->alists)) (+ (length menudict) 1)))
(check "the addition is the last entry"
       (eq? (last (menudict->alists)) steam))

;; --- an override ------------------------------------------------------------
;; A chord the table gets wrong is corrected by replacing the entry: what comes
;; back for the id is the settings file's, the built-in one is gone, and the
;; table does not grow a second Firefox.

(define built-in-firefox (menudict-for-id "org.mozilla.firefox"))

(define mine (replace-menu-app "firefox" "Firefox (mine)"
                               '("firefox" "org.mozilla.firefox")
                               '(("File" ("Open" "Ctrl+O" "ctrl" "o"))
                                 ("Edit" ("Undo" "Ctrl+Z" "ctrl" "z")))))

(check "the replacement answers to the id it replaced"
       (eq? (menudict-for-id "firefox") mine))
(check "the replacement answers to the entry's other ids too"
       (eq? (menudict-for-id "org.mozilla.firefox") mine))
(check "the replacement is not the built-in entry"
       (not (eq? mine built-in-firefox)))
(check "the built-in entry is gone, not shadowed"
       (not (any (lambda (app) (eq? app built-in-firefox))
                 (menudict->alists))))
(check "an override does not lengthen the table"
       (= (length (menudict->alists)) (+ (length menudict) 1)))
(check "the override's menus are the ones the settings file wrote"
       (equal? (map car (assq-ref (menudict-for-id "firefox") 'menus))
               '("File" "Edit")))
(check "the override's items are the ones the settings file wrote"
       (equal? (assq-ref (menudict-for-id "firefox") 'menus)
               '(("File" ("Open" "Ctrl+O" "ctrl" "o"))
                 ("Edit" ("Undo" "Ctrl+Z" "ctrl" "z")))))

;; Replacing again for the same id replaces the first replacement: the settings
;; file's last word wins, as it does for the rest of the configuration.
(define second (replace-menu-app "firefox" "Firefox (twice)"
                                 '("firefox") '(("File" ("Quit" "Ctrl+Q" "ctrl" "q")))))
(check "the last replacement for an id wins"
       (eq? (menudict-for-id "firefox") second))
(check "replacing twice still does not lengthen the table"
       (= (length (menudict->alists)) (+ (length menudict) 1)))

;; --- a bad entry ------------------------------------------------------------
;; "An entry that types the wrong chord is worse than no entry": one that types
;; nothing is refused, and the refusal says why instead of vanishing.

(define (reported thunk)
  "What THUNK logs while it runs, and what it returned."
  (let ((port (open-output-string)))
    (let ((value (parameterize ((current-error-port port)) (thunk))))
      (cons value (get-output-string port)))))

(define bad-entry
  (reported (lambda ()
              (add-menu-app "Broken" '("broken") '(("File" ("No chord")))))))
(define bad-entry-log (cdr bad-entry))

(check "an entry whose item has no chord is refused" (eq? (car bad-entry) #f))
(check "the refusal names the application" (string-contains bad-entry-log "Broken"))
(check "the refusal says what is wrong"
       (string-contains bad-entry-log "an item needs a label, the chord shown"))
(check "the refused entry is not in the table"
       (eq? (menudict-for-id "broken") #f))
(check "the refused entry leaves the table its size"
       (= (length (menudict->alists)) (+ (length menudict) 1)))

;; A menu whose two items type the same chord is refused for the same reason.
(define duplicate-chord
  (reported (lambda ()
              (add-menu-app "Duplicate" '("duplicate")
                            '(("Edit" ("Copy" "Ctrl+C" "ctrl" "c")
                               ("Paste" "Ctrl+V" "ctrl" "v")
                               ("Copy again" "Ctrl+C" "ctrl" "c")))))))
(check "two items typing one chord are refused" (eq? (car duplicate-chord) #f))
(check "the duplicate is reported"
       (string-contains (cdr duplicate-chord) "two entries typing Ctrl+C"))

;; An entry with no ids cannot ever be looked up, so it is refused too.
(define no-ids
  (reported (lambda () (add-menu-app "Nameless ids" '() '(("File" ("Quit" "Ctrl+Q" "ctrl" "q")))))))
(check "an entry with no ids is refused" (eq? (car no-ids) #f))
(check "the missing ids are reported"
       (string-contains (cdr no-ids) "it answers to no ids"))

;; --- a replacement for something the table does not know ---------------------
;; That is an addition wearing the wrong form: report it, and add it rather
;; than lose what the settings file wrote.

(define orphan
  (reported (lambda ()
              (replace-menu-app "nosuchapp" "No Such App" '("nosuchapp")
                                '(("File" ("Quit" "Ctrl+Q" "ctrl" "q")))))))

(check "replacing an unknown id is reported"
       (string-contains (cdr orphan) "nothing in the built-in table answers to nosuchapp"))
(check "replacing an unknown id still adds it"
       (not (eq? (menudict-for-id "nosuchapp") #f)))

;; --- reloading --------------------------------------------------------------
;; A reload must start from the built-in table again, or the entries would
;; stack up every time the settings file is saved.

(reset-menu-edits!)
(check "resetting forgets the additions" (eq? (menudict-for-id "steam") #f))
(check "resetting restores the built-in entries"
       (and (not (eq? (menudict-for-id "firefox") #f))
            (string=? (assq-ref (menudict-for-id "firefox") 'name) "Firefox")))
(check "resetting leaves the table its built-in size"
       (= (length (menudict->alists)) (length menudict)))

(if (> failures 0)
    (begin
      (format (current-error-port) "~a dictionary checks failed~%" failures)
      (exit 1)))

(format #t "the dictionary: ~a applications, ~a ids, ~a menus~%"
        (length apps)
        (length ids)
        (apply + (map (lambda (app) (length (assq-ref app 'menus))) apps)))

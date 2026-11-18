;; whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;; The menu dictionary is data, and data goes wrong in ways the C tests cannot
;; see: an id two applications both claim, an entry whose chord was never
;; written, a chord the runtime's parser cannot read and will therefore never
;; type, a menu with a label and nothing behind it, a label repeated in one
;; menu.  The runtime test loads the same table and asks it what a compositor
;; would; this one reads it.

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
(check "the dictionary knows many applications" (>= (length apps) 113))

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

;; ---------------------------------------------------------------------------
;; The chords the runtime can read
;; ---------------------------------------------------------------------------
;; The runtime passes every chord through `rust/src/chord.rs': Emacs notation
;; (C-c, M-F4, s-e), CUA notation (Ctrl+C, Alt+F4) or a bare key (F11, Space,
;; Delete), and a short item may hold several chords separated by spaces
;; (C-x C-s).  A chord it cannot read is a row the menu shows greyed and types
;; nothing for, so a chord the parser rejects is a dead entry rather than a
;; shortcut.  What follows is that grammar written out again, so the dictionary
;; can be checked without Cargo.

(define (key-shaped? text)
  (and (string? text)
       (not (string-null? (string-trim-both text)))
       (not (string-any char-set:whitespace (string-trim-both text)))))

(define modifier-names
  '("ctrl" "control" "primary" "alt" "meta" "option" "shift"
    "super" "win" "cmd" "command"))

(define (unknown-modifier head)
  (find (lambda (part) (not (member (string-downcase part) modifier-names)))
        (remove string-null? (map string-trim-both (string-split head #\+)))))

;; One C-, M-, S- or s- prefix per modifier, in any order, is Emacs notation;
;; what is left is the key.
(define (strip-emacs-prefixes text)
  "TEXT without its Emacs prefixes, and how many were taken off."
  (let loop ((rest text) (count 0))
    (if (and (>= (string-length rest) 2)
             (char=? (string-ref rest 1) #\-)
             (memv (string-ref rest 0) (list #\C #\M #\S #\s)))
        (loop (substring rest 2) (+ count 1))
        (values count rest))))

(define (cua-parts-problem text head key)
  "Why HEAD+KEY is not a CUA chord such as Ctrl+Shift+P, or #f when it is."
  (let ((unknown (unknown-modifier head)))
    (cond
      ((not (key-shaped? key))
       (format #f "~a has no key after its modifiers" text))
      (unknown (format #f "~a names ~a, which is not a modifier" text unknown))
      (else #f))))

;; Ctrl+Shift+P: the last plus separates the key.  Ctrl++ is Ctrl and the plus
;; key, and a plus with nothing before it is not a chord.
(define (cua-chord-problem text)
  (if (string-suffix? "+" text)
      (let ((head (substring text 0 (- (string-length text) 1))))
        (cua-parts-problem text
                           (if (string-suffix? "+" head)
                               (substring head 0 (- (string-length head) 1))
                               head)
                           "+"))
      (let ((last (string-rindex text #\+)))
        (if (and last (> last 0))
            (cua-parts-problem text (substring text 0 last) (substring text (+ last 1)))
            (format #f "~a has nothing before its plus" text)))))

(define (chord-problem chord)
  "Why CHORD is not one chord the runtime can read, or #f when it is."
  (let ((text (string-trim-both chord)))
    (call-with-values (lambda () (strip-emacs-prefixes text))
      (lambda (prefixes rest)
        (cond
          ((> prefixes 0)
           (and (not (key-shaped? rest))
                (format #f "~a has no key behind its modifiers" text)))
          ((string-contains text "+") (cua-chord-problem text))
          ((key-shaped? text) #f)
          (else (format #f "~a is not a key" text)))))))

(define (chords-problem written)
  "Why WRITTEN is not one or more chords the runtime can read, or #f when it is."
  (let ((chords (string-tokenize written)))
    (or (and (null? chords) (format #f "~a has no chord in it" written))
        (any (lambda (chord)
               (let ((problem (chord-problem chord)))
                 (and problem (format #f "~a: ~a" written problem))))
             chords))))

(define (item-chord-problem item)
  "Why ITEM's chord would not be typed, or #f when it would."
  (if (= (length item) 2)
      (chords-problem (list-ref item 1))
      ;; The long form: the runtime joins the modifiers with + and reads that,
      ;; so #f and an empty string mean the same thing.
      (let* ((modifiers (list-ref item 2))
             (key (string-trim-both (list-ref item 3)))
             (words (if (eq? modifiers #f)
                        '()
                        (string-tokenize modifiers))))
        (chords-problem (if (null? words)
                            key
                            (string-append (string-join words "+") "+" key))))))

;; A menu needs a label and at least one thing in it; an item needs a label
;; and a chord, either as (label chord) or as the long four-field form.  A
;; chord may be an Emacs sequence, so a space is not a mistake.
(define (item-signature item)
  (if (= (length item) 2)
      (string-downcase (list-ref item 1))
      (cons (list-ref item 2) (string-downcase (list-ref item 3)))))

(define (check-menu app label menu)
  (let ((name (assq-ref app 'name))
        (items (cdr menu)))
    (check (format #f "~a: menu label" name) (non-empty-string? label))
    (check (format #f "~a > ~a has items" name label) (pair? items))

    (for-each
     (lambda (item)
       (let ((where (format #f "~a > ~a > ~a" name label (car item))))
         (check (format #f "~a has a label and a chord" where)
                (memv (length item) '(2 4)))
         (check (format #f "~a is labelled" where) (non-empty-string? (list-ref item 0)))
         (check (format #f "~a shows a chord" where) (non-empty-string? (list-ref item 1)))
         (when (= (length item) 4)
           (check (format #f "~a has modifiers or #f" where)
                  (or (eq? (list-ref item 2) #f)
                      (non-empty-string? (list-ref item 2))))
           (check (format #f "~a has a key" where) (non-empty-string? (list-ref item 3))))
         ;; A chord the runtime's parser refuses is drawn greyed and types
         ;; nothing: the entry exists but the shortcut does not.
         (check (format #f "~a types a chord the parser reads" where)
                (not (item-chord-problem item)))))
     items)

    ;; Two entries that type the same chord in one menu: one of them is a typo,
    ;; and the other would be unreachable.
    (let ((chords (map item-signature items)))
      (check (format #f "~a > ~a has no duplicate chords" name label)
             (= (length chords) (length (delete-duplicates chords)))))

    ;; A menu is a list a person reads: two rows with one label make the second
    ;; one meaningless, and the copy-and-paste that produced it usually left the
    ;; chord wrong as well.
    (let ((labels (map car items)))
      (check (format #f "~a > ~a does not repeat a label" name label)
             (= (length labels) (length (delete-duplicates labels)))))))

(for-each
 (lambda (app)
   (let ((name (assq-ref app 'name))
         (menus (assq-ref app 'menus)))
     (check (format #f "~a has menus" name) (pair? menus))
     ;; Appending a shared menu to an application that already wrote one of that
     ;; label leaves two Edit menus, and only the first is ever opened.
     (let ((labels (map car (if (pair? menus) menus '()))))
       (check (format #f "~a does not repeat a menu" name)
              (= (length labels) (length (delete-duplicates labels)))))
     ;; An application with an id but nothing behind it can be focused and
     ;; still show an empty menubar, which is worse than a dictionary that does
     ;; not know it.
     (check (format #f "~a offers a row for its ids" name)
            (and (pair? (assq-ref app 'ids))
                 (any (lambda (menu) (and (pair? menu) (pair? (cdr menu)))) menus)))
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
       (string-contains bad-entry-log "an item needs a label and a chord"))
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

;; The suite's other scripts say "ok <name>" when they pass; this one says what
;; the table holds and then the same word.
(display "ok menudict\n")
(exit 0)

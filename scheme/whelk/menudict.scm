;; whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;; Copyright © Josep Bigorra <jjbigorra@gmail.com>

;;; Commentary:
;;
;; The menu dictionary: what the global menu shows for an application that
;; publishes no menubar of its own.
;;
;; A Wayland shell cannot ask a GTK application for its menus -- that is the
;; compositor's `gtk-shell' protocol, which niri and sway do not implement -- so
;; the shell needs a second source: menus written from each application's
;; documented keyboard shortcuts, typed into the focused window when an entry is
;; chosen.  This module is that source.  It is data, not logic: an application
;; is the names it answers to and the menus it has, and adding one is one form.
;;
;; Nothing here is a guess about what an application's own menu says.  Every
;; chord is the one its documentation gives; where a project offers two
;; bindings, the one printed in its own menus wins.  An application with no
;; entry gets the shared Edit and View instead of an invented File menu, so a
;; missing entry degrades rather than lies.
;;
;; The entries follow the machine this shell is developed on -- the desktop
;; entries actually installed there -- and the applications most people run:
;; browsers, terminals, editors, file managers, office suites, mail, graphics,
;; media players, PDF readers, chat clients, note-taking applications, CAD,
;; audio and video production, and the system's own panels.
;;
;; A chord may be written the way Emacs writes it: ("Copy" "C-c"), ("Quit"
;; "M-F4"), ("Find" "C-S-f").  The runtime reads C- as Control, M- as Alt,
;; S- as Shift and s- as Super, and translates either notation into the keys
;; it types -- so a settings file can be written in the notation its author
;; thinks in, and the menu can still show Ctrl+C or C-c, whichever the bar
;; asked for.

;;; Code:

(define-module (whelk menudict)
  #:use-module (whelk logger)
  #:use-module (srfi srfi-1)
  #:export (menudict
            menudict->alists
            menudict-for-id
            add-menu-app
            replace-menu-app
            reset-menu-edits!))

(define (dict-app name ids menus)
  "One application: the name for the log, the ids it answers to, its menus.

A menu is a list whose first element is its label and whose rest are items.
An item is (label chord): what the entry says, and the chord to show and
type -- C-v in Emacs notation, Ctrl+V in CUA notation, or a bare key such
as F11.  The long form (label accel modifiers key) is still read for a
chord the parser cannot spell, with #f for the modifiers of a bare key."
  (list (cons 'name name)
        (cons 'ids ids)
        (cons 'menus menus)))

;;; --- browsers --------------------------------------------------------------

(define firefox
  (dict-app
   "Firefox"
        '("firefox" "firefox-esr" "firefox-developer-edition" "firefox-nightly"
     "org.mozilla.firefox" "org.mozilla.firefox-esr" "librewolf"
     "io.gitlab.librewolf-community" "waterfox" "zen" "zen-browser"
     "app.zen_browser.zen" "mullvad-browser" "org.mullvad.MullvadBrowser")
   '(("File"
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Private Window" "Ctrl+Shift+P" "ctrl shift" "p")
      ("Open Location" "Ctrl+L" "ctrl" "l")
      ("Open File" "Ctrl+O" "ctrl" "o")
      ("Save Page As" "Ctrl+S" "ctrl" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find in Page" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Reload" "F5" #f "F5")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")
      ("Full Screen" "F11" #f "F11")
      ("Bookmarks" "Ctrl+Shift+O" "ctrl shift" "o")
      ("History" "Ctrl+H" "ctrl" "h")))))

(define chromium
  (dict-app
   "Chromium"
        '("chromium" "chromium-browser" "chromium-freeworld" "google-chrome"
     "google-chrome-stable" "google-chrome-beta" "chrome"
     "brave-browser" "brave" "com.brave.Browser" "microsoft-edge"
     "microsoft-edge-stable" "vivaldi" "vivaldi-stable" "opera"
     "opera-stable" "com.opera.Opera" "yandex-browser" "thorium"
     "ungoogled-chromium")
   '(("File"
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Incognito Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Open Location" "Ctrl+L" "ctrl" "l")
      ("Save Page As" "Ctrl+S" "ctrl" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Close Tab" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Shift+Q" "ctrl shift" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Reload" "F5" #f "F5")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")
      ("Full Screen" "F11" #f "F11")
      ("Developer Tools" "Ctrl+Shift+J" "ctrl shift" "j")))))

(define epiphany
  (dict-app
   "Web"
   '("epiphany" "org.gnome.Epiphany")
   '(("File"
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("Open Location" "Ctrl+L" "ctrl" "l")
      ("Save As" "Ctrl+S" "ctrl" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Close Tab" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Reload" "Ctrl+R" "ctrl" "r")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

;;; --- terminals -------------------------------------------------------------
;;; A terminal's Edit menu is its most useful one in a bar: copy and paste are
;;; the chords a compositor's clipboard works through, and every terminal that
;;; has tabs spells them the same way.

(define terminals
  (dict-app
   "Terminal"
        '("gnome-terminal" "org.gnome.Terminal" "tilix" "com.gexperts.Tilix"
     "org.gnome.Console" "kgx" "mate-terminal" "org.mate.terminal"
     "xfce4-terminal" "org.xfce.terminal" "terminator" "sakura"
     "lxterminal")
   '(("File"
      ("New Tab" "Ctrl+Shift+T" "ctrl shift" "t")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Close Tab" "Ctrl+Shift+W" "ctrl shift" "w")
      ("Quit" "Ctrl+Shift+Q" "ctrl shift" "q"))
     ("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")
      ("Select All" "Ctrl+Shift+A" "ctrl shift" "a")
      ("Find" "Ctrl+Shift+F" "ctrl shift" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Normal Size" "Ctrl+0" "ctrl" "0")
      ("Full Screen" "F11" #f "F11")))))

(define wezterm
  (dict-app
   "WezTerm"
   '("wezterm" "org.wezfurlong.wezterm")
   '(("File"
      ("New Tab" "Ctrl+Shift+T" "ctrl shift" "t")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Close Tab" "Ctrl+Shift+W" "ctrl shift" "w"))
     ("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")
      ("Search" "Ctrl+Shift+F" "ctrl shift" "f"))
     ("View"
      ("Zoom In" "Ctrl+=" "ctrl" "equal")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")))))

(define kitty
  (dict-app
   "kitty"
   '("kitty")
   '(("File"
      ("New Tab" "Ctrl+Shift+T" "ctrl shift" "t")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Close Tab" "Ctrl+Shift+W" "ctrl shift" "w"))
     ("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")))))

(define alacritty
  (dict-app
   "Alacritty"
   '("alacritty" "org.alacritty.Alacritty")
   '(("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")
      ("Search" "Ctrl+Shift+F" "ctrl shift" "f"))
     ("View"
      ("Zoom In" "Ctrl+=" "ctrl" "equal")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

;;; --- editors ---------------------------------------------------------------

(define text_editor
  (dict-app
   "Text Editor"
   '("org.gnome.TextEditor" "gnome-text-editor")
   '(("File"
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

(define gedit
  (dict-app
   "gedit"
   '("gedit" "org.gnome.gedit" "org.gnome.TextEditor.legacy")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Side Panel" "F9" #f "F9")
      ("Bottom Panel" "Ctrl+F9" "ctrl" "F9")
      ("Full Screen" "F11" #f "F11")))))

;; Emacs is where multi-key sequences matter: C-x C-s is one menu entry, not
;; two shortcuts.  The runtime types each chord in order.
(define emacs
  (dict-app
   "Emacs"
   '("emacs" "org.gnu.emacs" "emacs-gtk")
   '(("File"
      ("Open File" "C-x C-f")
      ("Save" "C-x C-s")
      ("Save As" "C-x C-w")
      ("Quit" "C-x C-c"))
     ("Edit"
      ("Undo" "C-x u")
      ("Cut" "C-w")
      ("Copy" "M-w")
      ("Paste" "C-y")
      ("Select All" "C-x h")
      ("Search" "C-s"))
     ("Buffers"
      ("List Buffers" "C-x C-b")
      ("Switch Buffer" "C-x b")
      ("Kill Buffer" "C-x k")))))

(define kate
  (dict-app
   "Kate"
        '("kate" "org.kde.kate" "kwrite" "org.kde.kwrite")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "Ctrl+Shift+F" "ctrl shift" "f")))))

(define msedit
  (dict-app
   "Edit"
   '("com.microsoft.edit" "msedit")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")))))

(define vscode
  (dict-app
   "Visual Studio Code"
   '("code" "code-oss" "codium" "vscodium" "cursor"
     "com.visualstudio.code")
   '(("File"
      ("New File" "Ctrl+N" "ctrl" "n")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Open File" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Close Editor" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Command Palette" "Ctrl+Shift+P" "ctrl shift" "p")
      ("Explorer" "Ctrl+Shift+E" "ctrl shift" "e")
      ("Terminal" "Ctrl+`" "ctrl" "grave")
      ("Toggle Sidebar" "Ctrl+B" "ctrl" "b")
      ("Zoom In" "Ctrl+=" "ctrl" "equal")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

(define jetbrains
  (dict-app
   "JetBrains IDE"
   '("jetbrains-idea" "jetbrains-webstorm" "jetbrains-datagrip"
     "jetbrains-studio" "jetbrains-rider" "jetbrains-pycharm"
     "jetbrains-phpstorm" "jetbrains-clion" "jetbrains-goland"
     "android-studio")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save All" "Ctrl+S" "ctrl" "s")
      ("Settings" "Ctrl+Alt+S" "ctrl alt" "s"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+R" "ctrl" "r"))
     ("View"
      ("Project" "Alt+1" "alt" "1")
      ("Terminal" "Alt+F12" "alt" "F12")))))

;;; --- file managers ---------------------------------------------------------

(define nautilus
  (dict-app
   "Files"
        '("nautilus" "org.gnome.Nautilus" "caja" "org.mate.caja")
   '(("File"
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Rename" "F2" #f "F2")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("New Folder" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Show Hidden Files" "Ctrl+H" "ctrl" "h")
      ("Reload" "Ctrl+R" "ctrl" "r")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

(define thunar
  (dict-app
   "Thunar"
   '("thunar" "org.xfce.Thunar")
   '(("File"
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Rename" "F2" #f "F2")
      ("Delete" "Delete" #f "Delete"))
     ("View"
      ("New Folder" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Show Hidden Files" "Ctrl+H" "ctrl" "h")
      ("Reload" "Ctrl+R" "ctrl" "r")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Back" "Alt+Left" "alt" "Left")
      ("Forward" "Alt+Right" "alt" "Right")))))

(define dolphin
  (dict-app
   "Dolphin"
   '("dolphin" "org.kde.dolphin")
   '(("File"
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("Close Tab" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Rename" "F2" #f "F2")
      ("Move to Trash" "Delete" #f "Delete"))
     ("View"
      ("New Folder" "F10" #f "F10")
      ("Show Hidden Files" "Ctrl+H" "ctrl" "h")
      ("Reload" "F5" #f "F5")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Split" "F3" #f "F3")
      ("Search" "Ctrl+F" "ctrl" "f")))))

(define nemo
  (dict-app
   "Nemo"
   '("nemo" "org.gnome.Nemo")
   '(("File"
      ("New Window" "Ctrl+N" "ctrl" "n")
      ("New Tab" "Ctrl+T" "ctrl" "t")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Rename" "F2" #f "F2")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("New Folder" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Show Hidden Files" "Ctrl+H" "ctrl" "h")
      ("Reload" "Ctrl+R" "ctrl" "r")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

;;; --- office ----------------------------------------------------------------

(define libreoffice_writer
  (dict-app
   "LibreOffice Writer"
   '("libreoffice-writer" "org.libreoffice.LibreOffice.writer"
     "libreoffice")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Find and Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Full Screen" "Ctrl+Shift+J" "ctrl shift" "j")
      ("Sidebar" "Ctrl+F5" "ctrl" "F5")))))

(define libreoffice_calc
  (dict-app
   "LibreOffice Calc"
   '("libreoffice-calc" "org.libreoffice.LibreOffice.calc")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Find and Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Full Screen" "Ctrl+Shift+J" "ctrl shift" "j")
      ("Sidebar" "Ctrl+F5" "ctrl" "F5")))))

(define libreoffice_impress
  (dict-app
   "LibreOffice Impress"
   '("libreoffice-impress" "org.libreoffice.LibreOffice.impress")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find and Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Start from First Slide" "F5" #f "F5")
      ("Full Screen" "Ctrl+Shift+J" "ctrl shift" "j")))))

(define libreoffice_draw
  (dict-app
   "LibreOffice Draw"
   '("libreoffice-draw" "org.libreoffice.LibreOffice.draw")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find and Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Full Screen" "Ctrl+Shift+J" "ctrl shift" "j")
      ("Sidebar" "Ctrl+F5" "ctrl" "F5")))))

;;; --- mail ------------------------------------------------------------------

(define thunderbird
  (dict-app
   "Thunderbird"
   '("thunderbird" "org.mozilla.Thunderbird")
   '(("File"
      ("New Message" "Ctrl+N" "ctrl" "n")
      ("Save As" "Ctrl+S" "ctrl" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Find Again" "F3" #f "F3"))
     ("Message"
      ("Reply" "Ctrl+R" "ctrl" "r")
      ("Reply All" "Ctrl+Shift+R" "ctrl shift" "r")
      ("Forward" "Ctrl+L" "ctrl" "l")
      ("Delete" "Delete" #f "Delete")
      ("Mark as Read" "M" #f "m"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

(define evolution
  (dict-app
   "Evolution"
   '("evolution" "org.gnome.Evolution")
   '(("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

;;; --- graphics -------------------------------------------------------------

(define gimp
  (dict-app
   "GIMP"
   '("gimp" "gimp-3.0" "gimp-3.2" "org.gimp.GIMP")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Export As" "Ctrl+Shift+E" "ctrl shift" "e")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("View"
      ("Zoom In" "+" #f "plus")
      ("Zoom Out" "-" #f "minus")
      ("Fit Image in Window" "Ctrl+Shift+J" "ctrl shift" "j")
      ("Full Screen" "F11" #f "F11")))))

(define inkscape
  (dict-app
   "Inkscape"
   '("inkscape" "org.inkscape.Inkscape")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Export" "Ctrl+Shift+E" "ctrl shift" "e")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "+" #f "plus")
      ("Zoom Out" "-" #f "minus")
      ("Drawing" "Ctrl+1" "ctrl" "1")
      ("Full Screen" "F11" #f "F11")))))

(define krita
  (dict-app
   "Krita"
   '("krita" "org.kde.krita")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("View"
      ("Zoom In" "+" #f "plus")
      ("Zoom Out" "-" #f "minus")
      ("Full Screen" "Ctrl+Shift+F" "ctrl shift" "f")))))

(define blender
  (dict-app
   "Blender"
   '("blender" "blender-5.2" "org.blender.Blender")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Duplicate" "Shift+D" "shift" "d")
      ("Select All" "A" #f "a")
      ("Deselect All" "Alt+A" "alt" "a"))
     ("View"
      ("Front" "Numpad1" #f "KP_1")
      ("Top" "Numpad7" #f "KP_7")
      ("Right" "Numpad3" #f "KP_3")))))

;;; --- media -----------------------------------------------------------------

(define vlc
  (dict-app
   "VLC"
   '("vlc")
   '(("File"
      ("Open File" "Ctrl+O" "ctrl" "o")
      ("Open Network" "Ctrl+N" "ctrl" "n")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Stop" "S" #f "s")
      ("Next" "N" #f "n")
      ("Previous" "P" #f "p")
      ("Fullscreen" "F" #f "f"))
     ("Audio"
      ("Mute" "M" #f "m")
      ("Volume Up" "Ctrl+Up" "ctrl" "Up")
      ("Volume Down" "Ctrl+Down" "ctrl" "Down")))))

(define mpv
  (dict-app
   "mpv"
   '("mpv")
   '(("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Mute" "M" #f "m")
      ("Fullscreen" "F" #f "f")
      ("Quit" "Q" #f "q"))
     ("View"
      ("Statistics" "I" #f "i")
      ("Screenshot" "S" #f "s")
      ("Subtitles" "J" #f "j")))))

(define totem
  (dict-app
   "Videos"
   '("totem" "org.gnome.Totem" "org.gnome.Showtime")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Fullscreen" "F11" #f "F11")))))

(define spotify
  (dict-app
   "Spotify"
   '("spotify" "com.spotify.Client")
   '(("File"
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Next" "Ctrl+Right" "ctrl" "Right")
      ("Previous" "Ctrl+Left" "ctrl" "Left"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

;;; --- documents -------------------------------------------------------------

(define evince
  (dict-app
   "Document Viewer"
   '("evince" "org.gnome.Evince" "papers" "org.gnome.Papers")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save a Copy" "Ctrl+S" "ctrl" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Fullscreen" "F11" #f "F11")
      ("Presentation" "F5" #f "F5")))))

(define okular
  (dict-app
   "Okular"
   '("okular" "org.kde.okular")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Fit Page" "Ctrl+0" "ctrl" "0")
      ("Fullscreen" "Ctrl+Shift+F" "ctrl shift" "f")))))

;;; --- chat ------------------------------------------------------------------

(define signal
  (dict-app
   "Signal"
   '("signal" "org.signal.Signal")
   '(("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

(define telegram
  (dict-app
   "Telegram"
   '("telegram" "org.telegram.desktop" "TelegramDesktop")
   '(("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

(define discord
  (dict-app
   "Discord"
   '("discord" "com.discordapp.Discord")
   '(("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")
      ("Toggle Mute" "Ctrl+Shift+M" "ctrl shift" "m")
      ("Toggle Deafen" "Ctrl+Shift+D" "ctrl shift" "d")))))

(define slack
  (dict-app
   "Slack"
   '("slack" "com.slack.Slack")
   '(("Edit"
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("View"
      ("Search" "Ctrl+F" "ctrl" "f")
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Reset Zoom" "Ctrl+0" "ctrl" "0")))))

;;; --- development tools -----------------------------------------------------

(define wireshark
  (dict-app
   "Wireshark"
   '("wireshark" "org.wireshark.Wireshark")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Find Packet" "Ctrl+F" "ctrl" "f")
      ("Find Next" "Ctrl+N" "ctrl" "n")
      ("Copy" "Ctrl+C" "ctrl" "c"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")))))

(define keepassxc
  (dict-app
   "KeePassXC"
   '("keepassxc" "org.keepassxc.KeePassXC")
   '(("File"
      ("New Database" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Open Database" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Copy Username" "Ctrl+B" "ctrl" "b")
      ("Copy Password" "Ctrl+C" "ctrl" "c")
      ("Find" "Ctrl+F" "ctrl" "f")))))

(define audacity
  (dict-app
   "Audacity"
   '("audacity")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save Project" "Ctrl+S" "ctrl" "s")
      ("Export Audio" "Ctrl+Shift+E" "ctrl shift" "e")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Y" "ctrl" "y")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("View"
      ("Zoom In" "Ctrl+1" "ctrl" "1")
      ("Zoom Out" "Ctrl+3" "ctrl" "3")
      ("Fit Selection" "Ctrl+E" "ctrl" "e")
      ("Fit Project" "Ctrl+F" "ctrl" "f")))))


;;; --- more terminals --------------------------------------------------------

(define konsole
  (dict-app
   "Konsole"
   '("konsole" "org.kde.konsole")
   '(("File"
      ("New Tab" "Ctrl+Shift+T" "ctrl shift" "t")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Close Tab" "Ctrl+Shift+W" "ctrl shift" "w")
      ("Quit" "Ctrl+Shift+Q" "ctrl shift" "q"))
     ("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")
      ("Select All" "Ctrl+Shift+A" "ctrl shift" "a")
      ("Find" "Ctrl+Shift+F" "ctrl shift" "f"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Normal Size" "Ctrl+0" "ctrl" "0")
      ("Full Screen" "Ctrl+Shift+F11" "ctrl shift" "F11")))))

(define ghostty
  (dict-app
   "Ghostty"
   '("ghostty" "com.mitchellh.ghostty")
   '(("File"
      ("New Tab" "Ctrl+Shift+T" "ctrl shift" "t")
      ("New Window" "Ctrl+Shift+N" "ctrl shift" "n")
      ("Close Tab" "Ctrl+Shift+W" "ctrl shift" "w")
      ("Quit" "Ctrl+Shift+Q" "ctrl shift" "q"))
     ("Edit"
      ("Copy" "Ctrl+Shift+C" "ctrl shift" "c")
      ("Paste" "Ctrl+Shift+V" "ctrl shift" "v")
      ("Search" "Ctrl+Shift+F" "ctrl shift" "f"))
     ("View"
      ("Zoom In" "Ctrl+Shift+=" "ctrl shift" "equal")
      ("Zoom Out" "Ctrl+Shift+-" "ctrl shift" "minus")
      ("Reset Size" "Ctrl+Shift+0" "ctrl shift" "0")))))

;;; --- image viewers ---------------------------------------------------------

(define image_viewer
  (dict-app
   "Image Viewer"
        '("org.gnome.Loupe" "loupe" "org.gnome.eog" "eog" "org.kde.gwenview"
     "gwenview" "ristretto" "org.xfce.ristretto")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Print" "Ctrl+P" "ctrl" "p")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("View"
      ("Zoom In" "Ctrl++" "ctrl" "plus")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")
      ("Full Screen" "F11" #f "F11")
      ("Slideshow" "F5" #f "F5")))))

;;; --- music players ---------------------------------------------------------

(define music
  (dict-app
   "Music"
        '("org.gnome.Music" "gnome-music" "io.bassi.Amberol" "amberol"
     "org.kde.elisa" "elisa" "lollypop" "rhythmbox" "clementine"
     "org.clementine_player.Clementine")
   '(("File"
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Next" "Ctrl+Right" "ctrl" "Right")
      ("Previous" "Ctrl+Left" "ctrl" "Left"))
     ("View"
      ("Search" "Ctrl+F" "ctrl" "f")))))

;;; --- video players ---------------------------------------------------------

(define video
  (dict-app
   "Video"
   '("io.github.celluloid_player.Celluloid" "celluloid" "org.kde.haruna"
     "haruna" "com.github.rafostar.Clapper" "clapper")
   '(("File"
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Playback"
      ("Play/Pause" "Space" #f "space")
      ("Fullscreen" "F11" #f "F11")
      ("Mute" "M" #f "m")))))

;;; --- video editing ---------------------------------------------------------

(define kdenlive
  (dict-app
   "Kdenlive"
   '("org.kde.kdenlive" "kdenlive")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Render" "Ctrl+Return" "ctrl" "Return")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("Project"
      ("Add Clip" "Ctrl+I" "ctrl" "i")))))

(define ardour
  (dict-app
   "Ardour"
   '("ardour9" "ardour")
   '(("Session"
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Export" "Ctrl+E" "ctrl" "e")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a"))
     ("Transport"
      ("Play/Stop" "Space" #f "space")))))

;;; --- note-taking -----------------------------------------------------------

(define obsidian
  (dict-app
   "Obsidian"
   '("md.obsidian.Obsidian" "obsidian")
   '(("File"
      ("New Note" "Ctrl+N" "ctrl" "n")
      ("Quick Switcher" "Ctrl+O" "ctrl" "o")
      ("Close" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f"))
     ("View"
      ("Command Palette" "Ctrl+P" "ctrl" "p")
      ("Reading View" "Ctrl+E" "ctrl" "e")))))

;;; --- more editors ----------------------------------------------------------

(define zed
  (dict-app
   "Zed"
   '("dev.zed.Zed" "zed")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Close Editor" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("File Finder" "Ctrl+P" "ctrl" "p")
      ("Project Search" "Ctrl+Shift+F" "ctrl shift" "f")
      ("Command Palette" "Ctrl+Shift+P" "ctrl shift" "p")
      ("Zoom In" "Ctrl+=" "ctrl" "equal")
      ("Zoom Out" "Ctrl+-" "ctrl" "minus")))))

(define sublime_text
  (dict-app
   "Sublime Text"
   '("sublime_text" "subl" "com.sublimetext.4")
   '(("File"
      ("New" "Ctrl+N" "ctrl" "n")
      ("Open" "Ctrl+O" "ctrl" "o")
      ("Save" "Ctrl+S" "ctrl" "s")
      ("Save As" "Ctrl+Shift+S" "ctrl shift" "s")
      ("Close File" "Ctrl+W" "ctrl" "w")
      ("Quit" "Ctrl+Q" "ctrl" "q"))
     ("Edit"
      ("Undo" "Ctrl+Z" "ctrl" "z")
      ("Redo" "Ctrl+Shift+Z" "ctrl shift" "z")
      ("Cut" "Ctrl+X" "ctrl" "x")
      ("Copy" "Ctrl+C" "ctrl" "c")
      ("Paste" "Ctrl+V" "ctrl" "v")
      ("Select All" "Ctrl+A" "ctrl" "a")
      ("Find" "Ctrl+F" "ctrl" "f")
      ("Replace" "Ctrl+H" "ctrl" "h"))
     ("View"
      ("Goto Anything" "Ctrl+P" "ctrl" "p")
      ("Goto Symbol" "Ctrl+R" "ctrl" "r")
      ("Full Screen" "F11" #f "F11")))))

;;; --- menus several applications share --------------------------------------
;;; Still data: a list of menus appended to an application's own.  They keep
;;; the table short without inventing anything -- these are the menus the
;;; applications document, written once because they spell them the same way.

(define standard-document-file
  '(("File"
     ("New" "C-n") ("Open" "C-o") ("Save" "C-s") ("Save As" "C-S-s")
     ("Print" "C-p") ("Quit" "C-q"))))

(define standard-edit
  '(("Edit"
     ("Undo" "C-z") ("Redo" "C-S-z")
     ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
     ("Find" "C-f") ("Replace" "C-h"))))

(define standard-edit-y
  '(("Edit"
     ("Undo" "C-z") ("Redo" "C-y")
     ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
     ("Find" "C-f") ("Replace" "C-h"))))

(define standard-view
  '(("View"
     ("Zoom In" "C-+") ("Zoom Out" "C--") ("Reset Zoom" "C-0")
     ("Full Screen" "F11"))))

(define terminal-edit
  '(("Edit"
     ("Copy" "C-S-c") ("Paste" "C-S-v") ("Select All" "C-S-a")
     ("Find" "C-S-f"))))

;;; --- terminals, continued --------------------------------------------------

(define foot
  (dict-app
   "foot"
   '("foot" "org.codeberg.dnkl.foot")
   (append terminal-edit
           '(("View" ("Zoom In" "C-+") ("Zoom Out" "C--")
                      ("Reset Zoom" "C-0") ("Full Screen" "F11"))))))

;;; --- browsers, continued ---------------------------------------------------

(define qutebrowser
  (dict-app
   "qutebrowser"
   '("qutebrowser" "org.qutebrowser.qutebrowser")
   '(("File"
      ("Open URL" "o") ("Open in New Tab" "S-o") ("Close Tab" "d")
      ("Reopen Tab" "u") ("Next Tab" "S-j") ("Previous Tab" "S-k"))
     ("Edit"
      ("Search" "/") ("Find Next" "n") ("Find Previous" "S-n")
      ("Copy URL" "y") ("Paste URL" "p"))
     ("View"
      ("Reload" "r") ("Hints" "f") ("Zoom In" "+") ("Zoom Out" "-")
      ("Reset Zoom" "=") ("Full Screen" "F11")))))

(define falkon
  (dict-app
   "Falkon"
   '("falkon" "org.kde.falkon")
   (append
    '(("File"
       ("New Tab" "C-t") ("New Window" "C-n")
       ("New Private Window" "C-S-n") ("Open Location" "C-l")
       ("Open File" "C-o") ("Save Page As" "C-s") ("Print" "C-p")
       ("Close Tab" "C-w") ("Quit" "C-q")))
    standard-edit
    '(("View"
       ("Reload" "F5") ("Zoom In" "C-+") ("Zoom Out" "C--")
       ("Reset Zoom" "C-0") ("Full Screen" "F11"))))))

;;; --- editors, continued ----------------------------------------------------

(define geany
  (dict-app
   "Geany"
   '("geany" "org.geany.Geany")
   (append standard-document-file standard-edit-y standard-view)))

;; The gedit-shaped editors of Xfce, MATE and Xed spell the same menus.
(define lightweight_editor
  (dict-app
   "Editor"
   '("mousepad" "org.xfce.mousepad" "pluma" "org.mate.pluma"
     "xed" "org.x.editor")
   (append standard-document-file standard-edit-y standard-view)))

;;; --- file managers, continued ----------------------------------------------

(define pcmanfm
  (dict-app
   "PCManFM"
   '("pcmanfm" "pcmanfm-qt" "org.pcmanfm.PCManFM")
   '(("File"
      ("New Tab" "C-t") ("New Window" "C-n") ("Close Tab" "C-w")
      ("Quit" "C-q"))
     ("Edit"
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Rename" "F2") ("Delete" "Delete") ("Find" "C-f"))
     ("View"
      ("New Folder" "C-S-n") ("Show Hidden Files" "C-h")
      ("Reload" "C-r") ("Zoom In" "C-+") ("Zoom Out" "C--")
      ("Back" "M-Left") ("Forward" "M-Right")))))

(define krusader
  (dict-app
   "Krusader"
   '("krusader" "org.kde.krusader")
   '(("File"
      ("New Tab" "C-t") ("Close Tab" "C-w") ("Quit" "C-q"))
     ("Edit"
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Rename" "F2") ("Move to Trash" "F8"))
     ("View"
      ("Show Hidden Files" "C-h") ("Reload" "C-r")
      ("Zoom In" "C-+") ("Zoom Out" "C--")
      ("Back" "M-Left") ("Forward" "M-Right")))))

(define doublecmd
  (dict-app
   "Double Commander"
   '("doublecmd" "doublecmd-qt" "doublecmd-gtk")
   '(("File"
      ("Quit" "C-q"))
     ("Edit"
      ("Copy" "F5") ("Move" "F6") ("New Folder" "F7") ("Delete" "F8"))
     ("View"
      ("Quick View" "F3") ("Edit File" "F4") ("Refresh" "C-r")
      ("Show Hidden Files" "C-h")))))

;;; --- office, continued -----------------------------------------------------

(define onlyoffice
  (dict-app
   "ONLYOFFICE"
   '("onlyoffice-desktopeditors" "org.onlyoffice.desktopeditors")
   (append standard-document-file standard-edit-y standard-view)))

(define abiword
  (dict-app
   "AbiWord"
   '("abiword" "com.abisource.AbiWord")
   (append standard-document-file standard-edit-y standard-view)))

(define gnumeric
  (dict-app
   "Gnumeric"
   '("gnumeric" "org.gnumeric.Gnumeric")
   (append standard-document-file standard-edit-y standard-view)))

;;; --- mail, continued -------------------------------------------------------

(define geary
  (dict-app
   "Geary"
   '("geary" "org.gnome.Geary")
   '(("File"
      ("New Message" "C-n") ("Print" "C-p") ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Find" "C-f"))
     ("Message"
      ("Reply" "C-r") ("Reply All" "C-S-r"))
     ("View"
      ("Zoom In" "C-+") ("Zoom Out" "C--") ("Full Screen" "F11")))))

;;; --- graphics, continued ---------------------------------------------------

(define darktable
  (dict-app
   "darktable"
   '("darktable" "org.darktable.Darktable")
   '(("File"
      ("Open Image" "C-o") ("Open Folder" "C-S-o") ("Export" "C-S-e")
      ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z") ("Copy" "C-c") ("Paste" "C-v"))
     ("View"
      ("Full Screen" "F11")))))

(define scribus
  (dict-app
   "Scribus"
   '("scribus" "net.scribus.Scribus")
   (append
    standard-document-file
    standard-edit-y
    '(("View"
       ("Zoom In" "C-+") ("Zoom Out" "C--") ("Full Screen" "F11"))))))

;;; --- video editing, continued ----------------------------------------------

(define shotcut
  (dict-app
   "Shotcut"
   '("shotcut" "org.shotcut.Shotcut")
   (append
    '(("File"
       ("New" "C-n") ("Open" "C-o") ("Save" "C-s") ("Save As" "C-S-s")
       ("Quit" "C-q")))
    standard-edit
    '(("Playback" ("Play/Pause" "Space"))
      ("View" ("Full Screen" "F11"))))))

(define pitivi
  (dict-app
   "Pitivi"
   '("pitivi" "org.pitivi.Pitivi")
   (append
    '(("File"
       ("New" "C-n") ("Open" "C-o") ("Save" "C-s") ("Save As" "C-S-s")
       ("Quit" "C-q")))
    standard-edit
    '(("Playback" ("Play/Pause" "Space"))
      ("View" ("Full Screen" "F11"))))))

(define openshot
  (dict-app
   "OpenShot"
   '("openshot" "org.openshot.OpenShot")
   (append
    '(("File"
       ("New" "C-n") ("Open" "C-o") ("Save" "C-s") ("Save As" "C-S-s")
       ("Quit" "C-q")))
    standard-edit-y
    '(("Playback" ("Play/Pause" "Space"))
      ("View" ("Full Screen" "F11"))))))

;;; --- media, continued ------------------------------------------------------

(define strawberry
  (dict-app
   "Strawberry"
   '("strawberry" "org.strawberrymusicplayer.strawberry")
   '(("File"
      ("Open" "C-o") ("Quit" "C-q"))
     ("Playback"
      ("Play/Pause" "Space") ("Next" "C-Right") ("Previous" "C-Left"))
     ("View"
      ("Full Screen" "F11")))))

(define parole
  (dict-app
   "Parole"
   '("parole" "org.xfce.parole")
   '(("File"
      ("Open" "C-o") ("Quit" "C-q"))
     ("Playback"
      ("Play/Pause" "Space") ("Full Screen" "F11"))
     ("View"
      ("Zoom In" "C-+") ("Zoom Out" "C--")))))

(define smplayer
  (dict-app
   "SMPlayer"
   '("smplayer" "org.smplayer.SMPlayer")
   '(("File"
      ("Open File" "C-o") ("Quit" "C-q"))
     ("Playback"
      ("Play/Pause" "Space") ("Forward" "Right") ("Back" "Left")
      ("Full Screen" "F"))
     ("Audio"
      ("Mute" "M") ("Volume Up" "Up") ("Volume Down" "Down")))))

;;; --- documents, continued --------------------------------------------------

(define zathura
  (dict-app
   "Zathura"
   '("zathura" "org.pwmt.zathura")
   '(("File"
      ("Open" "C-o") ("Close" "C-w") ("Quit" "C-q"))
     ("Edit"
      ("Find" "C-f"))
     ("View"
      ("Zoom In" "C-+") ("Zoom Out" "C--") ("Reset Zoom" "C-0")
      ("Full Screen" "F11")))))

(define mupdf
  (dict-app
   "MuPDF"
   '("mupdf" "com.artifex.mupdf")
   '(("File"
      ("Open" "C-o") ("Save" "C-s") ("Quit" "C-q"))
     ("Edit"
      ("Find" "C-f"))
     ("View"
      ("Zoom In" "C-+") ("Zoom Out" "C--") ("Reset Zoom" "C-0")
      ("Full Screen" "F11")))))

;;; --- chat, continued -------------------------------------------------------

(define element
  (dict-app
   "Element"
   '("element-desktop" "im.riot.Riot" "io.element.Element")
   '(("File"
      ("Settings" "C-,") ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a"))
     ("View"
      ("Search" "C-f") ("Zoom In" "C-+") ("Zoom Out" "C--")
      ("Reset Zoom" "C-0")))))

;;; --- note-taking, continued ------------------------------------------------

(define joplin
  (dict-app
   "Joplin"
   '("joplin" "net.cozic.joplin_desktop")
   '(("File"
      ("New Note" "C-n") ("New Todo" "C-S-n") ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Find" "C-f")))))

(define typora
  (dict-app
   "Typora"
   '("typora" "io.typora.Typora")
   (append
    standard-document-file
    '(("Edit"
       ("Undo" "C-z") ("Redo" "C-S-z")
       ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
       ("Find" "C-f") ("Replace" "C-h")))
    standard-view)))

;;; --- development tools, continued ------------------------------------------

(define postman
  (dict-app
   "Postman"
   '("postman" "com.getpostman.Postman")
   '(("File"
      ("New Request" "C-n") ("Save" "C-s") ("Save As" "C-S-s")
      ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Find" "C-f"))
     ("View"
      ("Send Request" "C-Return")))))

(define dbeaver
  (dict-app
   "DBeaver"
   '("dbeaver" "io.dbeaver.DBeaverCommunity")
   '(("File"
      ("New Connection" "C-n") ("Open SQL File" "C-o") ("Save" "C-s")
      ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Find" "C-f"))
     ("View"
      ("Refresh" "F5")))))

(define meld
  (dict-app
   "Meld"
   '("meld" "org.gnome.Meld")
   '(("File"
      ("New Comparison" "C-n") ("Open" "C-o") ("Save" "C-s")
      ("Close" "C-w") ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v")
      ("Select All" "C-a"))
     ("View"
      ("Find" "C-f")))))

(define bitwarden
  (dict-app
   "Bitwarden"
   '("bitwarden" "com.bitwarden.desktop")
   '(("File"
      ("New Item" "C-n") ("Lock" "C-l") ("Quit" "C-q"))
     ("Edit"
      ("Copy" "C-c") ("Select All" "C-a") ("Find" "C-f")))))

;;; --- desktop utilities -----------------------------------------------------

(define flameshot
  (dict-app
   "Flameshot"
   '("flameshot" "org.flameshot.Flameshot")
   '(("File"
      ("Save" "C-s") ("Quit" "C-q"))
     ("Edit"
      ("Copy" "C-c") ("Undo" "C-z")))))

(define file_roller
  (dict-app
   "Archive Manager"
   '("file-roller" "org.gnome.FileRoller")
   '(("File"
      ("New Archive" "C-n") ("Open" "C-o") ("Quit" "C-q"))
     ("View"
      ("Refresh" "F5")))))

(define ark
  (dict-app
   "Ark"
   '("ark" "org.kde.ark")
   '(("File"
      ("New Archive" "C-n") ("Open" "C-o") ("Quit" "C-q"))
     ("Edit"
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")))))

;;; --- audio production ------------------------------------------------------

(define lmms
  (dict-app
   "LMMS"
   '("lmms" "org.lmms.LMMS")
   (append
    '(("File"
       ("New Project" "C-n") ("Open" "C-o") ("Save" "C-s")
       ("Save As" "C-S-s") ("Quit" "C-q")))
    standard-edit
    '(("Transport" ("Play/Pause" "Space"))))))

(define musescore
  (dict-app
   "MuseScore"
   '("musescore" "mscore" "org.musescore.MuseScore")
   (append
    standard-document-file
    '(("Edit"
       ("Undo" "C-z") ("Redo" "C-S-z")
       ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")))
    '(("Playback" ("Play/Pause" "Space"))))))

;;; --- CAD and 3D ------------------------------------------------------------

(define freecad
  (dict-app
   "FreeCAD"
   '("freecad" "org.freecad.FreeCAD")
   (append
    standard-document-file
    standard-edit-y
    '(("View" ("Fit All" "C-S-f") ("Full Screen" "F11"))))))

(define openscad
  (dict-app
   "OpenSCAD"
   '("openscad" "org.openscad.OpenSCAD")
   (append
    '(("File"
       ("New" "C-n") ("Open" "C-o") ("Save" "C-s") ("Save As" "C-S-s")
       ("Quit" "C-q")))
    '(("Edit"
       ("Undo" "C-z") ("Redo" "C-S-z")
       ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")))
    '(("Design" ("Preview" "F5") ("Render" "F6"))))))

;;; --- study and libraries ---------------------------------------------------

(define anki
  (dict-app
   "Anki"
   '("anki" "net.ankiweb.Anki")
   '(("File"
      ("Sync" "y") ("Quit" "C-q"))
     ("Edit"
      ("Add Note" "a") ("Browse" "b") ("Decks" "d"))
     ("View"
      ("Statistics" "t") ("Preferences" "C-p")))))

(define calibre
  (dict-app
   "Calibre"
   '("calibre" "calibre-gui")
   '(("File"
      ("Open" "C-o") ("Quit" "C-q"))
     ("View"
      ("Full Screen" "F11")))))

(define foliate
  (dict-app
   "Foliate"
   '("foliate" "com.github.johnfactotum.Foliate")
   '(("File"
      ("Open" "C-o") ("Close" "C-w") ("Quit" "C-q"))
     ("Edit"
      ("Find" "C-f"))
     ("View"
      ("Zoom In" "C-+") ("Zoom Out" "C--") ("Reset Zoom" "C-0")
      ("Full Screen" "F11")))))

(define zotero
  (dict-app
   "Zotero"
   '("zotero" "org.zotero.Zotero")
   '(("File"
      ("New Item" "C-n") ("New Note" "C-S-n") ("Quit" "C-q"))
     ("Edit"
      ("Undo" "C-z") ("Redo" "C-S-z")
      ("Cut" "C-x") ("Copy" "C-c") ("Paste" "C-v") ("Select All" "C-a")
      ("Find" "C-f")))))

;;; --- the system's own panels -----------------------------------------------

(define gnome_settings
  (dict-app
   "GNOME Settings"
   '("gnome-control-center" "org.gnome.Settings")
   '(("View"
      ("Search" "C-f") ("Close" "C-w")))))

(define gnome_tweaks
  (dict-app
   "GNOME Tweaks"
   '("gnome-tweaks" "org.gnome.tweaks")
   '(("View"
      ("Close" "C-w")))))

(define virt_manager
  (dict-app
   "Virtual Machine Manager"
   '("virt-manager" "org.virt_manager.virt-manager")
   '(("File"
      ("New Virtual Machine" "C-n") ("Quit" "C-q"))
     ("View"
      ("Refresh" "F5")))))

(define filezilla
  (dict-app
   "FileZilla"
   '("filezilla" "org.filezillaproject.Filezilla")
   '(("File"
      ("Site Manager" "C-s") ("Quit" "C-q"))
     ("View"
      ("Refresh" "F5")))))

;;; --- the table -------------------------------------------------------------

;; Every application the dictionary knows, in the order it is written.
(define menudict
  (list firefox
        chromium
        epiphany
        terminals
        wezterm
        kitty
        alacritty
        text_editor
        gedit
        emacs
        kate
        msedit
        vscode
        jetbrains
        nautilus
        thunar
        dolphin
        nemo
        libreoffice_writer
        libreoffice_calc
        libreoffice_impress
        libreoffice_draw
        thunderbird
        evolution
        gimp
        inkscape
        krita
        blender
        vlc
        mpv
        totem
        spotify
        evince
        okular
        signal
        telegram
        discord
        slack
        wireshark
        keepassxc
        audacity
        konsole
        ghostty
        image_viewer
        music
        video
        kdenlive
        ardour
        obsidian
        zed
        sublime_text
        foot
        qutebrowser
        falkon
        geany
        lightweight_editor
        pcmanfm
        krusader
        doublecmd
        onlyoffice
        abiword
        gnumeric
        geary
        darktable
        scribus
        shotcut
        pitivi
        openshot
        strawberry
        parole
        smplayer
        zathura
        mupdf
        element
        joplin
        typora
        postman
        dbeaver
        meld
        bitwarden
        flameshot
        file_roller
        ark
        lmms
        musescore
        freecad
        openscad
        anki
        calibre
        foliate
        zotero
        gnome_settings
        gnome_tweaks
        virt_manager
        filezilla))

;;; =========================================================================
;;; WHAT A SETTINGS FILE MAY ADD
;;; =========================================================================
;; The built-in table is a starting point, not a law.  An application it does
;; not know can be added, and an entry it gets wrong can be replaced, from any
;; settings file: both are ordinary forms, and the runtime reads the result the
;; same way it reads the table.
;;
;; Neither trusts its argument.  An entry that would show an entry it cannot
;; type is refused and logged, because a dictionary that silently drops what
;; you wrote is worse than one that says why it did.

;; Replacements, as (selector . entry): the selector is the id the settings
;; file named, and the entry takes the place of whatever claims that id.
(define *replacements* '())

;; Validated entries, in the order the settings file wrote them.
(define *additions* '())

(define (string-non-empty? value)
  (and (string? value) (not (string-null? value))))

(define (strip-desktop-suffix id)
  (if (string-suffix? ".desktop" id)
      (substring id 0 (- (string-length id) (string-length ".desktop")))
      id))

(define (normalize-id id)
  (string-downcase (strip-desktop-suffix id)))

;; The same rule the C lookup applies: equal after normalising, or one a dotted
;; suffix of the other ("firefox" and "org.mozilla.firefox" are one application).
(define (id-answers-to? id candidate)
  (let ((question (normalize-id id))
        (name (normalize-id candidate)))
    (cond
      ((string=? question name) #t)
      ((> (string-length question) (string-length name))
       (string-suffix? (string-append "." name) question))
      ((> (string-length name) (string-length question))
       (string-suffix? (string-append "." question) name))
      (else #f))))

(define (entry-answers-to? app id)
  (any (lambda (app-id) (id-answers-to? id app-id))
       (or (assq-ref app 'ids) '())))

;; --- what makes an entry bad ----------------------------------------------

(define (item-problem item)
  (cond
    ((not (list? item)) "an item is not a list")
    ((not (memv (length item) '(2 4)))
     "an item needs a label and a chord, or the long label/chord/modifiers/key form")
    ((not (string-non-empty? (list-ref item 0))) "an item has no label")
    ((not (string-non-empty? (list-ref item 1)))
     (simple-format #f "~a shows no chord" (list-ref item 0)))
    ;; (label chord): the runtime reads Emacs or CUA notation from the one
    ;; string, so there is nothing else here to check.  It may be a sequence:
    ;; C-x C-s has a space between its chords.
    ((= (length item) 2) #f)
    ((not (or (eq? (list-ref item 2) #f) (string-non-empty? (list-ref item 2))))
     (simple-format #f "~a has neither modifiers nor #f for none"
                    (list-ref item 0)))
    ((not (string-non-empty? (list-ref item 3)))
     (simple-format #f "~a types no key" (list-ref item 0)))
    (else #f)))

;; Two items in one menu that would type the same keys.  The short form's one
;; chord is compared as it is written; the runtime's C test compares the parsed
;; modifiers and keys, which catches C-c against Ctrl+C as well.
(define (item-chord a)
  (if (= (length a) 2)
      (string-downcase (list-ref a 1))
      (cons (list-ref a 2) (string-downcase (list-ref a 3)))))

(define (same-chord? a b)
  (equal? (item-chord a) (item-chord b)))

(define (duplicate-chord rest)
  "The first item in REST that types what another item in REST types."
  (cond
    ((null? rest) #f)
    ((any (lambda (other) (same-chord? (car rest) other)) (cdr rest)) (car rest))
    (else (duplicate-chord (cdr rest)))))

(define (menu-problem menu)
  (cond
    ((not (list? menu)) "a menu is not a list")
    ((not (pair? menu)) "a menu has no label")
    ((not (string-non-empty? (car menu))) "a menu has no label")
    ((not (pair? (cdr menu)))
     (simple-format #f "menu ~a has no items" (car menu)))
    (else
     (or (any item-problem (cdr menu))
         (let ((twice (duplicate-chord (cdr menu))))
           (and twice
                (simple-format #f "~a has two entries typing ~a"
                               (car menu) (list-ref twice 1))))))))

(define (entry-problem app)
  "Why APP cannot be a menu application, or #f when it can."
  (let ((name (assq-ref app 'name))
        (ids (assq-ref app 'ids))
        (menus (assq-ref app 'menus)))
    (cond
      ((not (string-non-empty? name)) "it has no name")
      ((not (and (list? ids) (pair? ids))) "it answers to no ids")
      ((any (lambda (id) (not (string-non-empty? id))) ids)
       "one of its ids is not a string")
      ((not (and (list? menus) (pair? menus))) "it has no menus")
      (else (any menu-problem menus)))))

(define (refuse! name problem)
  (log-warn "Menus" "~a is not a menu application: ~a" name problem)
  #f)

;; --- the two forms a settings file calls ----------------------------------

(define (add-menu-app name ids menus)
  "Add an application to the dictionary, after the built-in entries.

NAME is what the log calls it, IDS are the ids a compositor or a desktop entry
reports, and MENUS are its menus, in the shape of the table above.  Returns the
entry, or #f when it could not be used -- in which case the reason is logged."

  (let ((app (dict-app name ids menus))
        (problem (entry-problem (dict-app name ids menus))))
    (if problem
        (refuse! name problem)
        (begin
          ;; An id already answered cannot be reached through the built-in
          ;; entry in front of it: say so, rather than pretend otherwise.
          (for-each
           (lambda (id)
             (let ((clash (find (lambda (other) (entry-answers-to? other id))
                                (menudict->alists))))
               (when clash
                 (log-warn "Menus"
                           "~a answers to ~a, which ~a already answers: use (replace-menu-app '~a' ...) to override it"
                           name id (assq-ref clash 'name) id))))
           ids)
          (set! *additions* (append *additions* (list app)))
          app))))

(define (replace-menu-app id name ids menus)
  "Put an application of your own in the place of the one that answers to ID.

The built-in entry claiming ID is dropped; the entry built from NAME, IDS and
MENUS takes its place in the table, so a settings file can correct a chord or a
whole menu without giving up the runtime's.  An ID the built-in table does not
know is reported and added instead.  Returns the entry, or #f when it could not
be used -- in which case the reason is logged."

  (let ((app (dict-app name ids menus))
        (problem (entry-problem (dict-app name ids menus))))
    (cond
      (problem (refuse! name problem))
      (else
       (unless (find (lambda (other) (entry-answers-to? other id)) menudict)
         (log-warn "Menus"
                   "nothing in the built-in table answers to ~a: adding ~a instead"
                   id name))
       (set! *replacements*
             (cons (cons id app)
                   (remove (lambda (other) (id-answers-to? id (car other)))
                           *replacements*)))
       app))))

(define (reset-menu-edits!)
  "Forget everything a previous settings file added or replaced.

Called before each load, so editing the file replaces the additions instead of
appending to them again."

  (set! *additions* '())
  (set! *replacements* '()))

;; --- the table as the runtime reads it ------------------------------------

(define (menudict->alists)
  "The dictionary as the C runtime reads it: the built-in table with each
replacement in the place of the entry it names, then the settings file's
additions, oldest first.

An alist per application, as (name . string), (ids . list of strings) and
(menus . list of (label items ...)), each item a list of label, accel,
modifiers and key.  The runtime walks it once, at startup and after a reload."

  (append
   ;; In place: the entry claiming the id is dropped, the replacement stands
   ;; where it stood.
   (map (lambda (app)
          (let ((replacement (find (lambda (pair)
                                     (entry-answers-to? app (car pair)))
                                   *replacements*)))
            (if replacement (cdr replacement) app)))
        menudict)
   ;; A replacement for an id nothing claims was reported when the file asked
   ;; for it; here it is simply an addition.
   (map cdr
        (remove (lambda (pair)
                  (any (lambda (app) (entry-answers-to? app (car pair)))
                       menudict))
                *replacements*))
   *additions*))

(define (menudict-for-id id)
  "The entry that answers to ID, as the runtime would find it, or #f."
  (find (lambda (app) (entry-answers-to? app id)) (menudict->alists)))

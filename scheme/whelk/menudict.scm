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
;; media players, PDF readers and chat clients.

;;; Code:

(define-module (whelk menudict)
  #:export (menudict
            menudict->alists))

(define (dict-app name ids menus)
  "One application: the name for the log, the ids it answers to, its menus.

A menu is a list whose first element is its label and whose rest are items.
An item is (label accel modifiers key): what the entry says, the chord printed
dim on its right, the modifiers to hold, and the key to press.  A #f for the
modifiers means a bare key, like F11."
  (list (cons 'name name)
        (cons 'ids ids)
        (cons 'menus menus)))

;;; --- browsers --------------------------------------------------------------

(define firefox
  (dict-app
   "Firefox"
   '("firefox" "firefox-esr" "firefox-developer-edition" "firefox-nightly"
     "org.mozilla.firefox" "librewolf" "waterfox" "zen" "zen-browser"
     "app.zen_browser.zen")
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
   '("chromium" "chromium-browser" "google-chrome" "google-chrome-stable"
     "brave-browser" "brave" "com.brave.Browser" "microsoft-edge"
     "microsoft-edge-stable" "vivaldi" "vivaldi-stable" "thorium"
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
     "org.gnome.Console" "kgx")
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

(define kate
  (dict-app
   "Kate"
   '("kate" "org.kde.kate")
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
   '("nautilus" "org.gnome.Nautilus")
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
        audacity))

(define (menudict->alists)
  "The dictionary as the C runtime reads it.

An alist per application, as (name . string), (ids . list of strings) and
(menus . list of (label items ...)), each item a list of label, accel,
modifiers and key.  The runtime walks it once, at startup."
  menudict)

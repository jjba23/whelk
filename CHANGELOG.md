# Changelog

All notable changes to whelk. The format is [Keep a Changelog][kac] and the
project follows [Semantic Versioning][semver].

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

_No changes yet._

## [0.7.0] - 2026-11-13

### Added

- **The global menu reads both chord notations.** An item may be written as
  `(label chord)` with the chord in Emacs notation (`C-v`, `M-F4`, `C-S-p`,
  `s-p`) or CUA notation (`Ctrl+V`, `Alt+F4`), and the runtime translates it
  into the modifiers and keys `wtype` or `xdotool` type. The long
  `(label chord modifiers key)` form is still read. `#:accelerator-style`
  chooses how the menu prints every chord: `'cua` by default, `'emacs` for
  `C-v` rather than `Ctrl+V`.
- **The menu bar shows shortcuts wherever it can.** A menubar exported over
  D-Bus now has its `accel` attributes read and printed beside the labels, so
  an application that publishes its menus also shows the keys those menus
  answer to. The dictionary's entries keep printing the chord they type.
- **Forty-four more applications in the dictionary**, written in Emacs
  notation: browsers (qutebrowser, Falkon), terminals (foot), editors (Geany,
  mousepad, pluma, xed), file managers (PCManFM, Krusader, Double Commander),
  office suites (ONLYOFFICE, AbiWord, Gnumeric), mail (Geary), graphics
  (darktable, Scribus), video editors (Shotcut, Pitivi, OpenShot), media
  players (Strawberry, Parole, SMPlayer), PDF readers (Zathura, MuPDF), chat
  (Element), note-taking (Joplin, Typora), developer tools (Postman, DBeaver,
  Meld, Bitwarden), desktop utilities (Flameshot, Archive Manager, Ark), audio
  production (LMMS, MuseScore), CAD (FreeCAD, OpenSCAD), study (Anki, Calibre,
  Foliate, Zotero), and the system's own panels. Ninety-four applications
  answer to two hundred and seventy-seven ids, and more existing entries accept
  extra ids: Opera and Chrome variants, mate-terminal, xfce4-terminal,
  terminator, caja, kwrite, Ristretto, Clementine and Mullvad Browser.
- **A menu bar that can be used like a menu bar.** The focused application's
  name comes first, submenus wear a right chevron, and the keyboard walks the
  menus: Down and Up through the items, Home and End to the ends, Left and
  Right between top-level menus, Right into a submenu, Escape out.
  `#:app-name?` turns the name label off.
- `tests/test_chord.c` covers the parser, formatter and both typing tools; the
  menutree and dictionary suites cover Emacs-stored items, the GTK `accel`
  attribute, both display styles, and duplicate chords after translation.

### Changed

- The dictionary may store either notation; the four-field form remains for a
  chord the parser cannot spell.
- A submenu is a menu button with a right chevron rather than a second label,
  and its entries are reachable from the keyboard like the rest.

### Fixed

- A bare `C-` -- a modifier prefix with no key behind it -- is refused rather
  than read as a key name.

## [0.6.0] - 2026-11-12

### Added

- The menu dictionary answers to a settings file: `add-menu-app` appends an
  application the built-in table does not know, `replace-menu-app` puts an entry
  of your own where the one answering to an id stood, and the runtime reads the
  result at startup and after every reload. A bad entry -- an item with no
  chord, a menu with nothing in it, two items typing one chord -- is refused and
  logged with the reason rather than dropped in silence, and a replacement for
  an id nothing answers to is reported and added.
- Ten more applications in the dictionary: Konsole, Ghostty, image viewers,
  music players, video players, Kdenlive, Ardour, Obsidian, Zed and Sublime
  Text -- fifty applications, a hundred and fifty-five ids.

## [0.5.0] - 2026-11-12

### Added

- **A dictionary of shortcut menus for the global menu, in Scheme.**
  `scheme/whelk/menudict.scm` holds the documented menus of forty
  applications -- a hundred and twenty ids: the browsers, terminals, editors,
  file managers, office suites, mail clients, graphics tools, media players,
  PDF readers, chat clients and developer tools installed on a real desktop --
  each entry a label, the chord it shows and the chord it types.  An
  application that publishes no menubar now gets File, Edit and the rest that
  are really its own, and one that is not in the table gets the shared Edit and
  View instead of an invented File menu.  Adding an application is one form; it
  is read once at startup, so nothing is compiled in and nothing needs a
  rebuild.
- `tests/test_menudict.scm` reads the table: every application named and given
  ids, no id claimed twice, no menu without items, no item without its chord,
  and no two entries in one menu typing the same keys.  `tests/test_menudict.c`
  and four cases in `tests/test_menutree.c` load the real dictionary through
  the bridge: lookup by id, by bare name, by `WM_CLASS` and by `.desktop` id,
  every entry complete, the de-duplication against the desktop entry's actions,
  the xdotool chords, and the case where nothing can type.

### Changed

- **The start menu wears a whelk.** The mark at the left of the default bar is
  no longer Font Awesome's empty star: it is the shell the shell is named after,
  one ribbon spiralling from a thin spire out to a wide lip.  It is drawn from a
  formula in the bar's own foreground, so it needs no icon theme, no icon file
  and no icon font, and it stays sharp at any `#:icon-size` (18 by default) and
  any bar height.  `#:label` still swaps it for a glyph.
- **The start menu is a native GTK menu.** The default bar wears
  `(start-module)` rather than a `custom-module` that called
  `rofi -show heks-system-menu`: one mark, and a popover with the launcher, the
  network panel, the notification centre, a reload and Quit, with no program
  spawned to draw it.
- The global menu asks D-Bus first, then the dictionary, then the shared
  shortcuts -- and asks GTK's anonymous application for a menubar too, which is
  where a non-unique application publishes one.
- A shortcut entry shows its chord on the right, dim, the way a menu does.

### Fixed

- The start menu's mark was drawn in the theme's text colour rather than the
  bar's, so on a dark bar it was nearly invisible; the glyph beside it had the
  same fault.  Both now take the bar's configured foreground.
- The start menu's tooltip covered its first entry; hover text is switched off
  while the popover is up.
- A File menu with nothing in it is not offered any more, and a separator is
  emitted only between two groups -- never twice, never at the end.

## [0.4.0] - 2026-11-12

### Added

- **The bar whelk starts with is the bar it documents.** `(default-bar)` and
  `(default-configuration!)` now live in `scheme/whelk/defaults.scm`: the
  heks-linux Waybar bar, one module for one, with the global menu after the
  focused window. `default-shell!` falls back to them, so a machine with no
  settings file gets the documented bar rather than a stand-in for it, and
  `tests/test_defaults.scm` pins the module layout and asserts that
  `settings/settings.scm` and the built-in bar are the same bar.
- `menu-module` is part of `settings/settings.scm`, so the copyable example
  carries the menu bar too.
- `README.org` is rewritten: what whelk is at a glance, a screenshot of the
  built-in bar, installation per ecosystem, a five-command quick start, the
  configuration reference grouped by what you are trying to do, and the session
  and Waybar-parity tables kept intact.

### Fixed

- **A crash while building the global menu.** `build_for_app` handed
  `g_desktop_app_info_list_actions()`'s result to `g_auto(GStrv)`, which frees
  strings the app info still owns. A build with the 26-module default bar
  crashed on 2 of 3 starts under AddressSanitizer and is clean on 6 of 6 after
  the fix; the `probe_menus` bus is an autoptr as well.
- **The machine-menu glyph is drawn in the bar's icon font.** The custom module
  carried U+F005 as `#:exec` output, which the text face has no glyph for; it is
  now `#:icon` plus `#:format "{icon}"`, like every other icon on the bar.
- **The suite passes where `/bin/sh` and `/bin/true` do not exist.** The session
  probe resolves `sh` through `g_find_program_in_path` instead of assuming the
  path, and the desktop-entry fixture runs a `true` found on `PATH` instead of
  `/bin/true` -- which is what a Guix build sandbox has.

### Verified

- `guix build -f guix.scm`, sandboxed check phase included, passes.
- `meson test` 12/12 and `maak run` stays up: `1 bar(s) up`, 26 modules.
- The README screenshot is the built-in bar itself, captured from a 1920x1080 X
  server.

## [0.3.3] - 2026-11-12

### Fixed

- **The X11 fallback, now exercised on a real X server.** Under X11
  `gtk_layer_is_supported()` provoked a GLib assertion (`init_and_get_layer_shell_global:
  GDK_IS_WAYLAND_DISPLAY(gdk_display)`) because it asks the display for a
  Wayland one; the layer question is now asked only of Wayland, and X11 goes
  straight to the dock path.
- **Struts describe the window that exists.** A top bar reserved 34 px while
  painting 36: the strut used the configured thickness, which GTK can exceed
  when it lays the bar out. It now uses the window's actual size and its
  position translated to the root, so a bar with a margin reserves only its own
  strip, and a resize refreshes it.
- `_NET_WM_STRUT` (the four-value property older panels read) is written
  alongside `_NET_WM_STRUT_PARTIAL`, and only bars reserve an edge: the
  launcher and the toasts stay docks without struts.

### Verified

- On Xvfb with a 1920x1080 screen: `_NET_WM_WINDOW_TYPE_DOCK`, the four
  above/sticky/skip states, `_NET_WM_DESKTOP = 0xFFFFFFFF`, and struts
  `top 30@0..1919`, `bottom 24@0..1919`, `left 40@0..1079`, `right 138@0..1079`
  for bars on each edge, each equal to the window it describes.
- `window-module` on X11 reads `WM_NAME`/`WM_CLASS` through `xprop` and shows
  the focused application's title.

## [0.3.2] - 2026-11-12

### Fixed

- **Session detection no longer depends on an exported variable.** `NIRI_SOCKET`
  and `SWAYSOCK` are inherited only by the processes a compositor spawned, and
  `niri msg` works regardless because niri resolves its default socket by path —
  so a bar started by a service unit, a login shell or a nested session went
  blank where it used to work. Detection now probes the compositor
  (`niri msg -j version`, then `swaymsg -t get_version`) with a 400 ms timeout
  when no variable names a session, and takes the first one that answers.
- sway's focused-window and close commands discover `$XDG_RUNTIME_DIR/sway-ipc.*.sock`
  when `SWAYSOCK` is absent, so they work in those sessions too.

### Added

- The detected session is logged once, with its evidence: `niri, from
  NIRI_SOCKET`, `niri, from niri msg -j version answered`, or a warning that no
  compositor answered and the window and menu modules stay empty.

## [0.3.1] - 2026-11-12

### Fixed

- The Nix expression lists `guile` in `nativeBuildInputs`, so Meson finds it at
  configure time and registers the Guile theme test: the Nix build runs all ten
  suites rather than nine.
- Its source tree filters `build/`, `result*` and `.direnv`, so packaging a tree
  that has already been built does not unpack a `build/` directory configured by
  another Meson version.

## [0.3.0] - 2026-11-12

### Added

- **A session layer, so the shell knows where it runs.** `src/util/session.c`
  detects niri, sway, an X11 desktop or a Wayland session with no
  shell-facing IPC, and answers the two questions the modules actually ask: what
  is in front, and how to close it. `NIRI_SOCKET` and `SWAYSOCK` identify their
  compositors; EWMH (`xprop`) supplies the same two strings on X11.
- **The focused window and the global menu work on sway and on X11.**
  `window-module` and `menu-module` parse niri's focused-window JSON, sway's
  window tree, or `WM_NAME`/`WM_CLASS` from `xprop`, instead of calling
  `niri msg`. The menu's Close entry uses the session's own command
  (`niri msg action close-window`, `swaymsg kill`, `wmctrl`), and a session that
  cannot close windows gets no entry rather than a dead one.
- **Shortcuts are typed with the session's tool**: `wtype` on Wayland,
  `xdotool key ctrl+shift+z` on X11.
- **An X11 dock fallback.** With `libx11` present at build time, a bar on an
  X11 desktop is a real panel: `_NET_WM_WINDOW_TYPE_DOCK`,
  `_NET_WM_STRUT_PARTIAL` and `_NET_WM_STRUT` for its edge, above/sticky/skip-
  taskbar/skip-pager states, and `_NET_WM_DESKTOP` on every workspace.
- **Graceful degradation without layer-shell.** GNOME's Wayland session (and
  KDE's) has no `wlr-layer-shell`: whelk now presents its surfaces as ordinary
  windows, logs one clear line explaining why, and keeps the launcher,
  notifications and panels usable instead of failing.
- `tests/test_session.c`: the three parsers, the command table and the
  no-command sessions.
- A "Compositors and desktops" table in the README, and a packaging
  `default.nix`.

### Changed

- `whelk_menubar_synthesize()` takes the session's close command, so the menu
  model no longer names a compositor.

## [0.2.0] - 2026-11-12

### Added

- **The shell's own menu.** `start-module` puts one glyph at the head of the
  bar, and behind it the launcher, the network panel, the notification centre,
  a settings reload and quitting. The glyph is the configuration's.
- **A global menu.** `menu-module` shows the focused application's menubar,
  read from D-Bus (`org.gtk.Menus`, the interface GTK publishes a menubar on)
  when the application exports one. Applications that export nothing get a
  menubar the shell synthesizes from their desktop entry's actions, from
  closing their window, and — when `wtype` is installed — from the Edit and View
  shortcuts every application shares. Nothing is offered that cannot run.
- **Bar themes.** Twelve brand-free palettes, from the pinstriped plastic of
  `glass-2001` to the tinted glass of `tinted-2024`, plus `frost`, `ember`,
  `mocha`, `midnight`, `sandstone` and `void`. `(bar #:theme 'name ...)` applies
  one; any colour the bar sets itself wins.
- `themes`, `theme-palette` and `theme-exists?` in the configuration language.
- The launcher's file mode searches every file **and folder** under `$HOME`,
  six levels deep, with caches, repositories and package trees skipped
  (`.cache`, `.git`, `node_modules`, `.venv`, …). Matches are ranked in C — a
  name match above a path match, a shallower path above a deeper one, files
  above folders of the same name — and the walk is reused for thirty seconds
  so typing does not cost one walk per key.

### Fixed

- A vertical bar's last group was laid out below its surface and painted
  nowhere when another bar's exclusive zone shrank the surface: layer surfaces
  now follow the compositor's size.
- A vertical bar reserved its configured thickness while painting its widest
  module, so windows slid under it: the reserved zone is the painted width.
- The launcher's file mode re-ranked an already ranked list by basename, which
  dropped every result that matched further up its path.

### Changed

- Panels and toasts wear the bar's palette through one shared popover skin.

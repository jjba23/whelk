# Changelog

All notable changes to whelk. The format is [Keep a Changelog][kac] and the
project follows [Semantic Versioning][semver].

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

### Added

- **Menu search, the Help menu's field.** `--launcher-mode=menuitem` (prefix
  `:`) flattens the focused application's live menubar -- whatever producer
  made it -- into rows carrying the entry's path and chord, ranks them with the
  launcher's own fuzzy matcher, and activates the chosen one through the same
  code the bar's menu click uses. It reads the same tree the bar is showing, so
  a GTK, DBusMenu, scripted or dictionary menu is searchable identically.

## [0.9.0] - 2026-11-13

### Added

- **A clipboard manager.** `wl-paste --watch` feeds a de-duplicated history
  (newest first, fifty entries, memory only), and `whelk --clipboard` or the
  launcher's `;` prefix lists it: choosing a row puts the copy back. The
  default bar's clipboard button opens that picker instead of `clipman`.
- **About This System is a window now.** The shell menu's first item opens a
  card painted in the bar's palette: host name and system at the top, then the
  processor, memory, disk, kernel, session and uptime, with a button that copies
  the summary. It replaces the generic `GtkAboutDialog`.
- **A shell menu that is configuration.** The popover behind the mark at the far
  left of the bar is now built from `(whelk menu)` units written in the settings
  file -- `(menu-item "Sleep" "systemctl suspend")`, `(menu-separator)`,
  `(menu-submenu "Recent Items" "shell:recent-items")` -- with
  `default-shell-menu` as the menu a file that writes none gets. The default is
  the Apple menu's shape in this desktop's words: About This System (the machine
  described in its own window), System
  Settings, Software, a Recent Items submenu filled from GTK's recent list when
  it opens, Force Quit, Sleep/Restart/Shut Down, Lock Screen and Log Out,
  followed by the shell's own panels. Actions spelled `shell:name` are the
  shell's (`about`, `launcher`, `force-quit`, `network`, `bluetooth`,
  `notifications`, `dnd`, `reload`); anything else is a command line.
- **The left half in macOS' order.** The default bar is now the shell menu, the
  window in front's name in bold, and the application's File/Edit/View -- where
  the reference bar puts its mark, its window title and its system menu.
  `menu-module` keeps its application-name variant behind `#:app-name?` for a
  bar that would rather have two items than three.
- **DBusMenu, the other menu protocol.** `com.canonical.dbusmenu` is read as
  well as `org.gtk.Menus`, so Qt and KDE applications' own menus appear in the
  bar: `GetLayout` for the tree, the item's `shortcut` for the chord, and
  `Event(id, "clicked")` when an entry is chosen. The window-scoped
  `.../window/0/menus/menubar` path is probed too.
- **A menu the configuration writes.** `menu-module`'s `#:menu-command` runs a
  program that prints `Menu<TAB>Item<TAB>Chord<TAB>Command` lines, and those
  become the focused application's menus -- the escape hatch for an application
  no bus describes, with no rebuild.
- **rofi's script protocol in the launcher.** A menu generator that prints
  `label\0icon\x1f…\0nonselectable\x1ftrue` rows is read as bytes rather than as
  text, so its icons appear, its rules are not selectable, and choosing a row
  runs the generator again with `ROFI_RETV=1` and the label, the way rofi's own
  script mode does.
- **`ssh` and `quit` launcher modes.** `--launcher-mode=ssh` (prefix `%`) lists
  the hosts `~/.ssh/config` names and opens one in a terminal, which is rofi's
  `ssh` mode; `--launcher-mode=quit` (prefix `!`) lists the open windows and
  closes the chosen one, which is what the shell menu's Force Quit opens.

### Changed

- **mako's numbers.** A toast stays ten seconds when the application does not
  say (mako's `default-timeout` in the reference configuration), five toasts are
  visible at once (`max-visible`), and a critical one still never expires on
  its own.
- `whelk --launcher-mode` documents the full mode list: app, window, run, calc,
  file, web, menu, ssh, quit, clipboard.
- **The notification centre and its toasts.** The centre's header now carries
  the count (it was written to a label that was never built), each group wears
  its application's icon and a count pill, cards gained padding, hover and
  action-button styling, and the empty state is an icon with a sentence rather
  than one grey line. Toasts carry up to two action buttons, so a notification
  can be answered without opening the centre.

## [0.8.0] - 2026-11-13

### Added

- **One application-name menu on the left.** `menu-module` now begins with the
  focused application's short name, and that name is a menu button: the
  launcher, the network and Bluetooth panels, the notification centre, a
  reload and quitting are behind it, and the application's own
  File/Edit/View follow. The default bar no longer puts the whelk mark and the
  focused window title beside it, so there is one menu instead of two;
  `(start-module)` is still available for a bar that wants the mark as well.
  The short name is the dictionary's name, else the desktop entry's name, else
  the last component of the application id (`com.jetbrains.toolbox` becomes
  `Toolbox`).
- **A Window menu for the applications the shell serves.** Full Screen and
  Close Window use the compositor's own actions, so a menu has the two window
  commands a menu bar is expected to have even when the application publishes
  no menubar. Desktop actions still come first in File.
- **Emacs multi-key chords.** A chord may be a sequence: `C-x C-s` is parsed,
  printed in either notation, and typed as `C-x` then `C-s`, rather than being
  refused because it contains a space. Emacs itself is in the dictionary with
  sequences such as `C-x C-s`, `C-x C-f`, `C-x b` and `C-x k`; the dictionary
  now covers ninety-five applications and two hundred and eighty ids.
- **A working, overflowing tray.** The tray reads the standard
  `RegisteredStatusNotifierItems` property, registers the bar as a
  StatusNotifierHost, and follows items registered after it starts, so a
  running JetBrains Toolbox appears. `#:max-items` (six by default) keeps the
  first icons on the bar and puts the rest behind a chevron.
- **Right-side bar overflow.** When the right row no longer fits the monitor,
  its trailing modules move behind a chevron in the row. The same widgets keep
  their timers, tooltips and clicks, so a clipped reading is not lost.

### Changed

- **The sound icon is the familiar speaker.** The default volume glyph is
  `\uf028`, and muting swaps it for `\uf026`, instead of the unfamiliar
  broadcast mark.
- **Bluetooth and notification counts are hover text.** Their default formats
  are the glyph alone; the connected-device and waiting counts remain in the
  tooltips and are still available to an explicit `#:format`.

### Fixed

- **The tray was reading a property almost no watcher publishes.** It asked
  for `RegisteredStatusItems`; the protocol's name is
  `RegisteredStatusNotifierItems`, which is what Waybar and other modern
  watchers expose. It also registers as a host now, so an item that was already
  running is listed rather than waiting for a registration signal that has
  already happened.
- **A whitespace chord was rejected by the Scheme validator** before the C
  parser could read it as a sequence. A sequence is now a valid dictionary
  item, and an empty chord is still refused with the reason.
- **The application-name menu disappeared when the application had no
  menus.** The shell's own launcher/panel/reload/quit menu was gated on the
  application menus being non-empty, so a session with no shortcut tool (or no
  focused window) lost the whole left item. The name is now always present;
  the row only hides when `#:app-name?` is off and there are no menus either.
- **The tray chevron stayed behind after the watcher vanished.** Clearing the
  item list now reflows the chevron and the overflow popover too.
- **`add-menu-app` logged with unescaped quotes.** The message's example
  `(replace-menu-app "id" ...)` ended the Scheme string early, so Guile warned
  `possibly unbound variable \`~a'` and the diagnostic was garbled. The example
  now uses a quoted symbol and the file compiles without that warning.

### Tests

- `tests/test_chord.c` covers sequence parsing, both display styles, both
  typing tools, and a sequence with a bare final chord.
- `tests/test_menutree.c` renders Emacs's `C-x C-s` from the dictionary and
  checks both the printed chord and the two typed commands; the Scheme suite
  accepts a multi-key item and still rejects one with no chord.

## [0.7.1] - 2026-11-13

### Changed

- **The Wi-Fi reading hides the network's name by default.** The bar shows the
  icon and the hover text shows the signal, not the SSID; `#:show-ssid? #t`
  names it again. An explicit `#:format` or `#:tooltip` still wins, because it
  was asked for.

### Fixed

- **The menu reads the documented `#:shortcuts?` and `#:app-name?` keys.** It
  had looked for `shortcuts` and `app-name`, so the question-mark spellings
  shown in the README were ignored. The older spellings still work.

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

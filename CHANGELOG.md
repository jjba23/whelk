# Changelog

All notable changes to whelk. The format is [Keep a Changelog][kac] and the
project follows [Semantic Versioning][semver].

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

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

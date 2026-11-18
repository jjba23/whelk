# Vision

whelk is a Wayland shell written in Rust, configured by a Scheme program that is
evaluated rather than parsed. The bar on screen today is a value your settings
file returns; the point of the project is that every surface a desktop needs --
bar, panels, launcher, notifications, toasts, menus -- is the same kind of
object, built by the same code, in the same language.

This document is not a changelog and it is not a promise list. It is the
direction the code is already leaning in, written down so that a contributor can
tell which pull request is on the path and which one is a detour.  This revision
is current at whelk 0.40.0 (November 2026).

## Where whelk is today

The pieces that exist and work: twenty-two module kinds behind one
keyword-driven shape; hot reload and a socket REPL; a notification daemon with
toasts, a grouped centre, actions and do-not-disturb; a Wi-Fi and Bluetooth
panel over `nmcli` and `bluetoothctl`; a launcher with eleven modes and a menu
protocol of its own; the focused application's `File`/`Edit`/`View` from six
producers (`org.gtk.Menus`, `com.canonical.dbusmenu`, AT-SPI, a configured
command, a dictionary of known applications, and the compositor's own window
verbs); a StatusNotifierItem tray with real `dbusmenu` popovers; a clipboard
history with pinning, expiry, a password-manager hint and lock-time clearing; a
volume and brightness HUD; a right row that folds behind a `+N` count with an
optional shelf; an About window that reports the machine; presets as Scheme
modules, the shipped bar and an XP-shaped taskbar included; twelve themes; and a
supervisor that restarts a crashed UI.

The implementation is now Rust 2024 from the command line to the last panel.
The C shell and the Meson tree are gone, the crate graph is vendored under
`rust/vendor` for offline Guix and Nix builds, and the 0.39 line removed the
remaining avoidable panics from configuration, PAM, menus and page
construction.  A release is not only a set of features; it is also a smaller
surface for the next contributor to audit.

Three things in the code are load-bearing for everything that comes next:

- **A module is a value, not a subclass.** A kind, a root widget, an `update`
  function and a destroy function, all in one uniform struct. Adding the
  twenty-third module kind is a small local change.
- **A surface is a `WhelkSurface`, not a window.** Edge, anchors, margins,
  exclusive zone, monitor and layer are properties of the surface, so the next
  thing whelk grows does not re-learn layer-shell.
- **The session is asked, not assumed.** niri, sway, Hyprland, river and X11
  are detected and probed for capabilities separately; what a session cannot do
  goes quiet with one line in the log.
- **A decision is a value.** Restart policy, lock policy, idle policy and
  workspace selection are pure transitions over small types. The process,
  timer or compositor performs the decision; it does not contain the decision.
- **Unsafe is a named boundary.** POSIX calls, libguile, PAM, the session-lock
  ABI and gamma-table ioctls live in audited modules with documented invariants.
  A widget or parser never adds an `unsafe` block to avoid a borrow check.
- **Malformed input is an error, not a panic.** Configuration, files, PAM
  responses, D-Bus replies and menu paths have an error path. A missing Guile
  handle, an unexpected widget child or a malformed menu is handled and logged,
  not turned into a crash that the supervisor has to restart.

**Floating desktop widgets** are now one of those surfaces: a `WhelkSurface`
with `#:exclusive? #f`, margins and a fixed card.  The Golden Gate collection
uses it; cards drag by a grip, snap to an eight-pixel grid, stay inside their
monitor and remember a position per monitor.  The remaining work is refinement,
not a missing foundation: better keyboard placement, multi-monitor handoff and
a visual picker for the widget catalogue.

## Engineering principles

The user-facing vision has an implementation vision underneath it.  These are
the rules that decide whether a change makes the shell easier or harder to
trust.

1. **Safe by construction before safe by review.** Prefer a type that cannot
   hold an invalid value over a runtime check, and prefer an exhaustive enum
   over a boolean with a comment.  A constructor should make the impossible
   state impossible, not document it.
2. **A functional core with imperative edges.** Parsing, scoring, geometry,
   policy, scheduling and menu selection are plain functions over owned data.
   GTK, Guile, D-Bus and process I/O are thin edges that translate a decision
   into an effect.  This is why the restart policy, fuzzy search and playlist
   selection can be tested without a display.
3. **One audited FFI boundary per C library.** The `unsafe` block is where the
   C contract is written down, not where a shortcut hides.  Static names are C
   string literals; dynamic input is copied once; no `SCM` or borrowed C pointer
   escapes into a widget.
4. **Errors are user-facing facts.** A failed load, a refused lock, a missing
   compositor interface or an unreadable file should produce a specific message
   at the boundary and a safe degraded state.  A panic is a bug in the shell,
   not an error report.
5. **Test the decision, not the timer.** A policy takes data and time as input
   and returns a decision; the timer only sleeps and calls it.  A parser takes
   text and returns a typed reading; the widget only paints it.
6. **Documentation is part of the contract.** A documented keyword, command,
   default or failure message is a testable promise.  When the code and a page
   disagree, the page is a bug report with instructions.

## The bar after the overflow shelf

0.42.0 shipped the first five entries of the previous version of this list, and
0.43.0 shipped the two that followed -- auto-hide and a bar a screen reader can
read and a keyboard can drive -- so the bar now has a task list, a fold that
knows what must stay, presets that can be overridden and switched from the
command line, a battery that answers "how long" and sums two packs, a native
MPRIS tile in place of the `playerctl` one, an edge that gives its pixels back,
and accessible names, roles and a keyboard path.  What is left is ranked the same way: by the value it adds to a desktop
that uses whelk every day, and by how much of the foundation is already there.
The order is an argument, not a promise.

1. **A screen-reader pass in CI.** The names, roles and keyboard path landed;
   what is missing is the proof that a real reader hears them.  A CI job that
   runs AT-SPI against a test bar -- asserting every reading's name, its role,
   and the order Tab walks -- is the difference between "we set the properties"
   and "it is usable with Orca".  Focus that does not jump when a popover opens
   belongs here too.  *Why:* assistive technology is exactly the consumer that
   never gets tested by hand.  Effort: medium.

2. **A quick-settings popover.** One bar module opening volume, brightness,
   Wi-Fi, Bluetooth, battery and power profile in a single card.  *Why:* the
   data and panels exist separately; a single obvious place is what people
   expect from a modern bar, and it gives the network and power readings a home
   that is not a chevron.  Effort: high.

3. **A per-monitor window tile.** The window module shows the focused window on
   the bar's own monitor, with the application icon, click-to-focus and
   scroll-to-cycle.  *Why:* with two monitors the global focus title is wrong on
   every bar but one; the task list answers the multi-window half of that, and
   this answers the single-window half.  Effort: low-medium.

4. **A bar and preset preview.** `whelk --preview` (or a button in the settings
   window) renders a chosen preset, bar or theme in a floating surface or a
   PNG.  *Why:* trying the XP preset no longer means editing the live
   configuration -- `whelk --preset xp` switches it -- but seeing one without
   taking over the session is still a separate thing worth having, and a preview
   is how a screenshot in the README stops going stale.  Effort: low-medium.

5. **A bar inspector and icon fallback.** `whelk --inspect-bar` prints module
   widths, the fold boundary and what is hidden, and a startup check falls
   back from a missing icon font to symbolic icons.  *Why:* `+3` is hard to
   debug from the outside, the fold now answers to priorities as well as
   position, and a fresh machine with no Font Awesome shows boxes instead of
   glyphs.  Both are small and protect first impressions.  Effort: low.

6. **A sparkline module.** A small history of CPU, memory or network
   throughput, drawn as a graph rather than a number.  *Why:* a bar is for
   glancing, and a number without a trend is half the story; the module
   protocol already supplies the timer and the widget tree.  Effort: medium.

7. **MPRIS polish left on the table.** The tile speaks to the player directly,
   has play/pause, next, previous, a position and a local cover, but it has no
   popover, no seek bar and no remote art.  *Why:* a card with the track, a
   scrubber and the queue is what a native player tile is for; the proxy work
   is already in `mpris.rs`.  Effort: medium.

## The preferences window: next

The window now exposes the launcher geometry, zoom and mode keys, every
module's options and visibility, and the rest of the GUI override layer.  What
remains is not another page; it is making that override layer trustworthy.
Ranked by value.

1. **Overrides you can see and undo.** Mark every row whose value comes from
   the GUI, show the value `settings.scm` would supply, and put a per-row
   **Revert** beside it. A page-level **Reset** clears only that page's
   overrides, **Undo** puts back the last change, and **Reset all** asks before
   discarding everything. *Why:* the window says "GUI overrides" but does not
   say which values those are, so a user cannot tell a tuned value from the
   configuration and one click can discard all of them. Every control already
   carries its preference key; the missing piece is remembering the inherited
   value and the change history, not inventing another writer. Tests should
   cover modified marks, row revert, page reset, undo and the confirmation
   path, and the documentation should show one override being reverted. Effort:
   low-medium.

2. **One metadata schema for module options.** Describe each option once --
   label, help text, type, default, unit, range, step and choices -- and let
   the window, the docs and the tests read that description. The module editor
   then chooses a color picker, font chooser, path chooser, enum dropdown or
   precise numeric control instead of a raw Scheme name and a generic
   `0..1000000` spin. *Why:* the editor is type-safe now, but not value-safe:
   editing a `0.3333` option through a three-digit spin can round it to
   `0.333`, and bounds and defaults are already repeated between the window and
   the launcher. One schema removes that drift, makes validation possible and
   gives module authors a place to explain their options. Tests should prove
   that each type selects the right control, that ranges and choices are
   enforced, and that a value round-trips without changing shape; the module
   reference should be generated from the same table. Effort: high.

3. **Atomic saves with visible failures.** Write preferences through a
   temporary file and a rename, validate a value before it is queued, show the
   error on the row and keep the previous accepted value. A failed reload
   should not leave the window claiming success. *Why:* a preference write
   that fails currently only reaches the log, while the control still shows
   the new value; a read-only state directory or a malformed font size then
   looks saved and is not. Silent loss undermines every page and every type
   fix above. Tests should inject a failed write and an invalid value, assert
   that the old value survives and the error is visible, and cover the atomic
   replacement itself. The documentation should say where overrides are stored
   and what the window does when it cannot write there. Effort: medium.

4. **A bar selector, not an assumption.** Let the window choose which bar it
   edits when the configuration returns several, keep overrides distinct per
   bar and output, and offer copying settings from one bar to another.
   *Why:* the window currently loads the first bar only, so a multi-monitor
   configuration silently exposes one bar and gives no way to reproduce a
   tuned layout on another output. A selector makes per-output differences
   visible and turns a repeated setup into one action. Tests should cover
   enumerating bars and outputs, editing a bar that is not first, keeping
   overrides isolated per output, and copying without aliasing the source;
   the documentation should show a two-monitor example. Effort: medium-high.

## The vision

A desktop where the shell is a single programmable process that knows its own
state, instead of five programs that talk past each other. Concretely, five
statements that should be true in a few years and are not true yet:

1. **Any surface is a widget.** Bars, panels, the launcher, toasts, the HUD and
   free-floating desktop widgets are all built from the same surface and module
   primitives, and a user can put a live module anywhere on the desktop by
   writing three lines.
2. **The configuration is a library.** `settings.scm` is not a config file with
   Lisp syntax: it can `(use-modules ...)`, ship its own procedures, and import
   a shared module from another repository. A bar becomes something you can
   publish.  Presets are the first proof: `(use-preset! 'xp)` is a library a
   settings file calls, not a copy of one.
3. **The menu bar is honest.** Wherever an application can be made to tell the
   truth about its menus, whelk reads that truth; the dictionary of known
   shortcuts is the fallback it falls back to, and it shrinks over time rather
   than growing.
4. **The shell degrades, never breaks.** On a compositor without layer-shell,
   on a session without focus IPC, whelk still starts, still runs its launcher
   and notifications, and says in one log line what it could not do.
5. **The implementation is boring to audit.** The FFI surface is small enough
   to read in an afternoon, every decision worth testing is a pure function,
   and no malformed file, menu or authentication reply can turn into a panic.
   Safety is a property of the architecture, not a promise in a README.

## Milestones since the first draft

The first draft was written while the shell was still a C program with a Scheme
front end.  The direction survived; the implementation changed underneath it.
The milestones that mattered:

- **0.19-0.20** made a deliberate bar per monitor with hotplug, read menus from
  AT-SPI, allowed a Scheme module whose update is a Guile procedure returning a
  widget, added golden-image tests, and landed a fail-closed PAM locker.  On a
  compositor with `ext-session-lock-v1` the lock is compositor-enforced; the
  layer-shell overlay remains only as a fallback, and the lock fails closed
  when neither interface can provide a real input grab.
- **0.24-0.29** turned the right row's overflow into a configurable shelf and a
  `+N` fold, added the About window and presets as Scheme modules, completed the
  preferences window's reversible overrides, and gave the idle/inhibit manager
  one timer for logind's idle lock and do-not-disturb.
- **0.30-0.38** moved the whole implementation to Rust, retired the C shell and
  Meson tree, added the Golden Gate desktop widgets with drag and per-monitor
  placement, and made the crate graph vendored so Guix and Nix build offline.
- **0.39.1-0.39.3** made the POSIX calls one audited module, turned the
  supervisor restart policy into a pure decision, replaced optional Wi-Fi page
  fields and their `expect`s with concrete types, passed static Guile names as
  C string literals, and removed the last avoidable panics from the config,
  menu and authentication paths.
- **0.40.0** closed the CodeQL review: Scheme modules come only from explicit
  install paths, logind properties use their real interface, suspend waits for
  lock readiness, the lock fails closed, notification markup and Bluetooth
  input are validated, menu expansion is bounded, and clipboard saves use
  private atomic files.

## Reserved work that is still open

These are the gaps the code already anticipates.

- **Configured surfaces beyond bars and widgets.** A launcher or panel is still
  built from Rust defaults and a CLI request.  The bar and widget records show
  the pattern: a Scheme record, a typed reader, a surface factory, a reload
  boundary.
- **Per-monitor panels.** Bars follow `#:monitor`; the launcher, toasts and
  panels still choose the first output.  A shared placement decision would fix
  the whole family rather than one surface at a time.
- **Keyboard placement for desktop widgets.** Dragging is complete and the
  position is remembered; moving, resizing and choosing a widget without a
  pointer is the accessibility-shaped remainder.
- **A stable module API.** The module protocol is documented and typed, but a
  third-party module still means changing Rust.  A versioned Scheme-side module
  API is the long-run condition for an ecosystem.

## New features that would be worth having

Not reserved for by the code, but plausible and in character.

- **A Waybar converter.** `waybar-to-whelk config.jsonc style.css` printing a
  `settings.scm`, so a migration is a command rather than an afternoon. whelk
  already documents module-by-module parity with the heks-linux Waybar bar;
  the mapping exists, it is just written in prose right now.
- **A reference generator.** Modes, bar keys, module options and settings
  docstrings are already in the source; a `whelk --docs` that emits the
  reference would keep the documentation from drifting.

## Contributions that would move the needle

In rough order of value per hour of work:

- **Menu dictionary entries.** Each one is a small, reviewable, testable unit:
  the shortcuts of an application you use, with their chords, in the format the
  existing entries use. Ninety-five applications is a start; the useful number
  is "whatever is on your dock". This is the single best first contribution.
- **Themes.** A palette is a value and a theme is a file. Themes named after
  materials and years are the house style, not products.
- **Packaging and distribution.** Nix, Guix, Debian, Arch and openSUSE recipes
  exist; keeping them building, adding a Fedora/CI lane and getting whelk into a
  distribution archive are the next real steps.
- **Compositor integrations.** The session interface wants more implementations,
  and each one comes with tests that are easy to write: the parser for
  `hyprctl -j monitors`, the mapping from river's tags to workspaces.
- **A module cookbook.** Ten worked examples -- a media player, a git branch, a
  Pomodoro timer, a weather reading, an MPRIS queue -- in the documentation's style.
  Nothing demonstrates "it is a program" better than examples that are short.
- **Accessibility.** A bar read by a screen reader needs the right GTK roles
  announced, a keyboard path through every panel, and the HUD not to steal
  focus. This is unglamorous, genuinely missing, and would make whelk usable by
  more people.

## Compared to related projects

| Project | What it is | Where whelk differs |
| --- | --- | --- |
| Waybar | C++ bar, JSON config plus CSS, modules compiled in | whelk's bar is data returned by Scheme, and the shell owns the panels, launcher and notifications that Waybar leaves to other programs |
| eww / ags (Aylur's GTK shell) | widget systems configured in `yuck` or TypeScript, drawn as GTK layers | closer in spirit than anything else; whelk's modules are native Rust with a Scheme front end, so a bar costs no interpreter at runtime, at the price of a smaller widget vocabulary |
| Quickshell | QtQuick shell framework in QML | Quickshell has the richer widget toolkit and a real declarative language; whelk has the notification daemon, global menu and tray as first-class native parts rather than plugins |
| sfwbar, nwg-panel | pinned, focused panel programs | comparable on the bar; whelk is broader by design -- one process for the session's surfaces |
| polybar | the X11 answer to all of the above | whelk keeps an X11 dock path with EWMH struts so the same configuration works there |
| cliphist, clipman | clipboard history | whelk's history is a module with a picker, pinning, expiry and lock-time clearing |
| GNOME Shell, KDE Plasma | the reference integrated shells | the comparison that matters: whelk is not a desktop environment, it is the shell layer of one you already chose |

What the others do that whelk could still add: Quickshell's depth of widget
types (a bar with a chart, a slider, a carousel without writing Rust); AGS's
project structure for shipping a whole desktop as a package; Waybar's pure
JSON-plus-CSS ease for people who do not want to learn Scheme; Plasma's
configuration UI, which no shell in this list approaches; and `swaync`'s
notification centre filtering rules, which are more expressive than DND.

Where whelk is ahead and should stay ahead: the application menu (nothing else
in the table reads both `org.gtk.Menus` and `com.canonical.dbusmenu` and falls
back to a documented dictionary), the one-process session model, and the
willingness to say in the log exactly which capability is missing.

## In the long run

- **A stable module protocol.** If packaging a module means patching Rust, the
  ecosystem stays small. A documented, versioned Scheme-side module API -- a kind,
  an `update`, a widget tree, an options alist -- plus a way to load modules
  from a directory lets people share a bar the way they share an Emacs package.
- **Shells as libraries.** `WhelkSurface` is already the seam. Publishing it as
  a small library other shells can link means whelk's layer-shell, panel and
  menu code outlives whelk's bar.
- **A push for a real menu protocol on Wayland.** `com.canonical.dbusmenu` is a
  decade-old de facto standard and `org.gtk.Menus` is GTK-only. A global menu
  needs a protocol; if one is proposed, the project with five producers behind
  it is in a good position to argue for it.
- **A regression suite you can trust with a release.** Golden images, a headless
  niri, a stub notification client, and a Waybar-configured desktop replayed
  against whelk's output. The tests are what would let the project grow past
  twenty-two module kinds without fear.
- **A shell someone installs by default.** The honest long-term measure: a
  distribution image or a desktop spin that ships whelk as its shell, because
  the bar, the launcher and the notifications all came from one place and
  nothing had to be glued together.

## How to help without writing a shell

Read the source and file issues with a log line and a `settings.scm` that
reproduces it. Send a menu dictionary entry. Send a theme. Send a paragraph
explaining what you expected the shell to do -- the documentation is the specification,
and a wrong page is a bug report with instructions.

---

_This document describes intent, not commitments; `docs/` and `NEWS.org`
describe what exists._

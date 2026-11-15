# Vision

whelk is a Wayland shell written in C, configured by a Scheme program that is
evaluated rather than parsed. The bar on screen today is a value your settings
file returns; the point of the project is that every surface a desktop needs --
bar, panels, launcher, notifications, toasts, menus -- is the same kind of
object, built by the same code, in the same language.

This document is not a changelog and it is not a promise list. It is the
direction the code is already leaning in, written down so that a contributor can
tell which pull request is on the path and which one is a detour.

## Where whelk is today

The pieces that exist and work: twenty-two module kinds behind one
keyword-driven shape; hot reload and a socket REPL; a notification daemon with
toasts, a grouped centre, actions and do-not-disturb; a Wi-Fi and Bluetooth
panel over `nmcli` and `bluetoothctl`; a launcher with nine modes and a menu
protocol of its own; the focused application's `File`/`Edit`/`View` from
five producers (`org.gtk.Menus`, `com.canonical.dbusmenu`, a configured command,
a dictionary of ninety-five applications, and the compositor's own window
verbs); a StatusNotifierItem tray with real `dbusmenu` popovers; a clipboard
history with pinning, expiry, a password-manager hint and lock-time clearing; a
volume and brightness HUD; a right row that folds behind a `+N` count with an
optional shelf; an About window that reports the machine; presets as Scheme
modules, the shipped bar and an XP-shaped taskbar included; twelve themes; and a
supervisor that restarts a crashed UI.

Three things in the code are load-bearing for everything that comes next:

- **A module is a value, not a subclass.** A kind, a root widget, an `update`
  function and a destroy function, all in one uniform struct. Adding the
  twenty-third module kind is a small local change.
- **A surface is a `WhelkSurface`, not a window.** Edge, anchors, margins,
  exclusive zone, monitor and layer are properties of the surface, so the next
  thing whelk grows does not re-learn layer-shell.
- **The session is asked, not assumed.** niri, sway and X11 are detected and
  probed for capabilities separately; what a session cannot do goes quiet with
  one line in the log.

The architecture's reserved surface is now **floating desktop widgets**: a
`WhelkSurface` with `#:exclusive? #f`, margins and a fixed card.  The Golden
Gate collection uses it; dragging a card and remembering its position are the
next step, not a missing foundation.

## The bar after the overflow shelf

The shelf and the automatic fold gave the right row a way to lose width without
losing modules; the battery now knows that only a pack that is filling is quiet;
and a preset is a name a settings file can apply.  The next bar work, ranked by
the value it adds to a desktop that uses whelk every day and by how much of the
foundation is already in place.  The order is an argument, not a promise.

1. **A task-list module.** Every open window as a button, grouped by workspace,
   click to focus, middle-click to close.  *Why:* `window-module` names only the
   focused window, so no whelk bar can be a taskbar; this is the missing half of
   the XP preset and of every bottom-bar layout.  The session layer already has
   focus, close and workspace commands for niri, sway and X11; the work is a
   list command and a parser per backend.  Effort: high.

2. **Fold priorities, reflow and shelf memory.** Let a module declare that it
   must stay (`#:keep?`, or a priority), recompute the fold when the bar's
   width, font, scale or monitor changes instead of once after first
   allocation, and remember whether the shelf was open across reloads.  *Why:*
   the fold is positional today, so a clock can disappear while a CPU meter
   stays; a bar that loses its time is worse than one that loses a load average.
   It also finishes the feature we just shipped.  Effort: low.

3. **Preset overrides and switching without editing.** `use-preset!` with
   `#:bars`/`#:widgets` overrides, plus a way to select a preset outside
   `settings.scm` (a CLI flag or an environment variable) and remember the
   choice.  *Why:* presets just landed; without overrides, changing one colour
   means forking the bundle, and trying the XP bar means editing the live
   configuration by hand.  This is what makes `current` and `xp` feel like
   themes rather than templates.  Effort: low-medium.

4. **Battery time remaining and aggregate packs.** Report `{time}` from
   `energy_now`/`power_now` (or charge/current), and `#:device "all"` to sum two
   packs.  *Why:* a percentage does not answer "how long"; two-pack laptops and
   charge thresholds are common.  The parsers are pure sysfs, so they are
   testable without a display.  Effort: low-medium.

5. **A native MPRIS module.** Title, artist, album, art, play state,
   previous/play/next and progress; choose the active player.  *Why:* the
   default now-playing tile shells out to `playerctl` every two seconds; a
   native module is live, has controls, and removes a subprocess from a bar
   that is supposed to be one process.  Effort: medium.

6. **Accessibility and keyboard navigation.** AT-SPI roles and names for every
   module and popover, a tab/arrow path through the bar, focus that does not
   jump when a popover opens, and a screen-reader pass in CI.  *Why:* the bar is
   mouse-first; a shell that ignores assistive technology excludes people from
   the part of the desktop they look at most.  This is the most valuable
   unglamorous work on the list.  Effort: medium-high.

7. **A quick-settings popover.** One bar module opening volume, brightness,
   Wi-Fi, Bluetooth, battery and power profile in a single card.  *Why:* the
   data and panels exist separately; a single obvious place is what people
   expect from a modern bar, and it gives the network and power readings a home
   that is not a chevron.  Effort: high.

8. **Auto-hide and reveal.** Hide the bar except for a hot edge or keybinding,
   toggling the exclusive zone so maximised windows use the space.  *Why:*
   vertical space matters on laptops and for fullscreen media; auto-hide is the
   one common bar behaviour whelk lacks.  Effort: medium.

9. **A per-monitor window tile.** The window module shows the focused window on
   the bar's own monitor, with the application icon, click-to-focus and
   scroll-to-cycle.  *Why:* with two monitors the global focus title is wrong on
   every bar but one; this is a small step toward the task list and makes the XP
   centre honest on a multi-monitor desk.  Effort: low-medium.

10. **A bar and preset preview.** `whelk --preview` (or a button in the settings
    window) renders a chosen preset, bar or theme in a floating surface or a
    PNG, reusing `bar_preview` and the golden harness.  *Why:* trying the XP
    preset or a new theme should not mean taking over the session and
    restarting; a preview is also how a screenshot in the README stops going
    stale.  Effort: low-medium.

11. **A bar inspector and icon fallback.** `whelk --inspect-bar` prints module
    widths, the fold boundary and what is hidden, and a startup check falls
    back from a missing icon font to symbolic icons.  *Why:* `+3` is hard to
    debug from the outside, and a fresh machine with no Font Awesome shows
    boxes instead of glyphs.  Both are small and protect first impressions.
    Effort: low.

12. **A sparkline module.** A small history of CPU, memory or network
    throughput, drawn as a graph rather than a number.  *Why:* a bar is for
    glancing, and a number without a trend is half the story; the module
    protocol already supplies the timer and the widget tree.  Effort: medium.

## The vision

A desktop where the shell is a single programmable process that knows its own
state, instead of five programs that talk past each other. Concretely, four
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

## Built since this file was written

0.19 closed five gaps this file used to list: a deliberate bar per monitor with
hotplug, menus read from AT-SPI, a Scheme module whose update is a Guile
procedure returning a widget, golden-image tests on a headless compositor, and
a fail-closed PAM screen locker. They are documented in the README and covered
by the test suite. 0.20 closes the locker's remaining gap: on a compositor that
implements `ext-session-lock-v1` the lock is compositor-enforced, so another
client cannot cover it and a dead shell cannot unlock it. The layer-shell
overlay remains only as the documented fallback for compositors without the
protocol.

0.24 made the right row's overflow a feature rather than an accident -- a
configurable shelf, a `+N` fold that counts what it hides -- and taught the
battery the difference between filling and merely plugged in.  The current line
adds an About window that reports the machine and presets as Scheme modules,
with a first XP-shaped taskbar.  0.29 completes the preferences window: every
module's options and visibility, and the launcher's own mode keys and default
mode, are edited there as reversible overrides, with the file escape hatch kept
for what a window cannot express.

## Features that are in scope and not built yet

These are the ones the code already anticipates.

- **Widget dragging and placement memory.** Desktop widgets are placed by
  `settings.scm` today. A drag would need to write back to a preference
  override, remember a per-monitor position and fall back sanely when the
  monitor geometry changes.
- **Hyprland and river.** The session layer is a small interface with three
  implementations; `hyprctl` and river's IPC are two more. The workspaces and
  window modules would light up on two more desktops.
- **An idle and inhibit daemon.** One process that owns idle timers, inhibitor
  locks and the do-not-disturb flag, because "why did my screen not blank" is
  exactly the kind of question a single-process shell should be able to answer.

## New features that would be worth having

Not reserved for by the code, but plausible and in character.

- **A Waybar converter.** `waybar-to-whelk config.jsonc style.css` printing a
  `settings.scm`, so a migration is a command rather than an afternoon. whelk
  already documents module-by-module parity with the heks-linux Waybar bar;
  the mapping exists, it is just written in prose right now.
- **A reference generator.** Modes, bar keys, module options and settings
  docstrings are already in the source; a `whelk --docs` that emits the
  reference would keep the README from drifting.

## Contributions that would move the needle

In rough order of value per hour of work:

- **Menu dictionary entries.** Each one is a small, reviewable, testable unit:
  the shortcuts of an application you use, with their chords, in the format the
  existing entries use. Ninety-five applications is a start; the useful number
  is "whatever is on your dock". This is the single best first contribution.
- **Themes.** A palette is a value and a theme is a file. Themes named after
  materials and years are the house style, not products.
- **Packaging.** Nix and Guix exist; an Arch `PKGBUILD`, a Fedora spec or a
  Debian source package would each be a real distribution.
- **Compositor integrations.** The session interface wants more implementations,
  and each one comes with tests that are easy to write: the parser for
  `hyprctl -j monitors`, the mapping from river's tags to workspaces.
- **A module cookbook.** Ten worked examples -- a media player, a git branch, a
  Pomodoro timer, a weather reading, an MPRIS queue -- in the README's style.
  Nothing demonstrates "it is a program" better than examples that are short.
- **Accessibility.** A bar read by a screen reader needs the right GTK roles
  announced, a keyboard path through every panel, and the HUD not to steal
  focus. This is unglamorous, genuinely missing, and would make whelk usable by
  more people.

## Compared to related projects

| Project | What it is | Where whelk differs |
| --- | --- | --- |
| Waybar | C++ bar, JSON config plus CSS, modules compiled in | whelk's bar is data returned by Scheme, and the shell owns the panels, launcher and notifications that Waybar leaves to other programs |
| eww / ags (Aylur's GTK shell) | widget systems configured in `yuck` or TypeScript, drawn as GTK layers | closer in spirit than anything else; whelk's modules are native C with a Scheme front end, so a bar costs no interpreter at runtime, at the price of a smaller widget vocabulary |
| Quickshell | QtQuick shell framework in QML | Quickshell has the richer widget toolkit and a real declarative language; whelk has the notification daemon, global menu and tray as first-class native parts rather than plugins |
| sfwbar, nwg-panel | pinned, focused panel programs | comparable on the bar; whelk is broader by design -- one process for the session's surfaces |
| polybar | the X11 answer to all of the above | whelk keeps an X11 dock path with EWMH struts so the same configuration works there |
| cliphist, clipman | clipboard history | whelk's history is a module with a picker, pinning, expiry and lock-time clearing |
| GNOME Shell, KDE Plasma | the reference integrated shells | the comparison that matters: whelk is not a desktop environment, it is the shell layer of one you already chose |

What the others do that whelk could still add: Quickshell's depth of widget
types (a bar with a chart, a slider, a carousel without writing C); AGS's
project structure for shipping a whole desktop as a package; Waybar's pure
JSON-plus-CSS ease for people who do not want to learn Scheme; Plasma's
configuration UI, which no shell in this list approaches; and `swaync`'s
notification centre filtering rules, which are more expressive than DND.

Where whelk is ahead and should stay ahead: the application menu (nothing else
in the table reads both `org.gtk.Menus` and `com.canonical.dbusmenu` and falls
back to a documented dictionary), the one-process session model, and the
willingness to say in the log exactly which capability is missing.

## In the long run

- **A stable module protocol.** If packaging a module means patching C, the
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

## How to help without writing C

Read the source and file issues with a log line and a `settings.scm` that
reproduces it. Send a menu dictionary entry. Send a theme. Send a paragraph
explaining what you expected the shell to do -- the README is the specification,
and a wrong README is a bug report with instructions.

---

_This document describes intent, not commitments; the README and `NEWS.org`
describe what exists._

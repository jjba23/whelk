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

The pieces that exist and work: twenty module kinds behind one keyword-driven
shape; hot reload and a socket REPL; a notification daemon with toasts, a
grouped centre, actions and do-not-disturb; a Wi-Fi and Bluetooth panel over
`nmcli` and `bluetoothctl`; a launcher with nine modes including rofi's own
script protocol; the focused application's `File`/`Edit`/`View` from five
producers (`org.gtk.Menus`, `com.canonical.dbusmenu`, a configured command, a
dictionary of ninety-five applications, and the compositor's own window verbs);
a StatusNotifierItem tray with real `dbusmenu` popovers; a clipboard history
with pinning, expiry, a password-manager hint and lock-time clearing; a volume
and brightness HUD; twelve themes; and a supervisor that restarts a crashed UI.

Three things in the code are load-bearing for everything that comes next:

- **A module is a value, not a subclass.** A kind, a root widget, an `update`
  function and a destroy function, all in one uniform struct. Adding the
  twenty-first module kind is a small local change.
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
   publish.
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

## Features that are in scope and not built yet

These are the ones the code already anticipates.

- **Widget dragging and placement memory.** Desktop widgets are placed by
  `settings.scm` today. A drag would need to write back to a preference
  override, remember a per-monitor position and fall back sanely when the
  monitor geometry changes.
- **A complete preferences window.** `prefs.scm` and the GTK settings window
  cover the bar, theme, notifications, Wi-Fi, global menu, tray and clock. The
  remaining modules, keybindings and launcher modes should be reachable there,
  with the same reversible-overrides model, and with an "open the file at this
  line" escape hatch for everything the window cannot express.
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
| mako, dunst, swaync | notification daemons | whelk folds the daemon in, with a grouped centre and DND that the bar modules can see |
| rofi, wofi | launchers | whelk runs the same overlay with the focused window's live menus, and speaks rofi's script protocol, so existing scripts keep working |
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
  twenty module kinds without fear.
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

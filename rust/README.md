# whelk-rs

The first slice of whelk's move from C + GTK4 + Guile Scheme to **Rust + GTK4 +
Guile Scheme**.

The C shell is still the default `whelk` binary and still owns the real session.
`whelk-rs` is a separate Cargo package with a separate application id
(`org.jointhefreeworld.whelk.rust`), so both can be built, tested and started
without a flag day. The migration is incremental on purpose: pure policy and a
small audited Scheme boundary are proven here before any C surface is deleted.

## What is implemented

- `src/wallpaper.rs` — the C wallpaper engine's policy, ported without GTK:
  extension classification, animation names and arithmetic, fit geometry, easing
  and the sequential/shuffled rotation.
- `src/settings.rs` — the typed view of `(shell-wallpaper->alist)` and
  `(shell-clipboard->alist)`: defaults, numeric clamps, `~/` expansion, the XDG
  config path, and the clipboard persistence-path rule.
- `src/collection.rs` — the C collection walk: recursive roots, hidden-entry
  skipping, file/directory symlink rules, video filtering, dedupe, sorting and
  the explicit current file.
- `src/rotation.rs` — the C engine's current-selection resolution and rotation
  clock as a pure `Playlist`: resolved fallbacks, sequential wrapping,
  reproducible shuffled rotation and the "can rotate" guard.
- `src/wallpaper_view.rs` — the C view's GTK snapshot widget: two paintables,
  eased transitions, fade/slide/zoom, fit and the 60-second Ken Burns breath.
  Its timing helpers are pure and tested without a display.
- `src/wallpaper_surface.rs` — one Rust background surface per output. It loads
  still images through `GdkTexture`, videos through `GtkMediaFile`, drives
  animated GIF/WebP frames through `GdkPixbufAnimation`, starts a layer-shell
  background on Wayland and a normal window on Broadway/X11, and drives rotation
  from the collection/playlist.  A repaint replaces the surfaces it presented
  last (windows, rotation timer and any paused media), so a chosen picture or a
  settings save never stacks a second background on one output.  The set
  follows the monitor model: the outputs are signed as `CONNECTOR:WxH;` in
  model order, and one idle rebuilds every surface when that signature or the
  surface count changes, giving a new output the current item while an
  unchanged set leaves a running movie alone.  A rebuild clears each surface's
  media first -- `view.clear()` and a paused `MediaStream` -- so a removed
  output cannot leave a decoder running behind a destroyed window.
- `src/wallpaperpicker.rs` — the wallpaper picker, ported from
  `src/wallpaper_picker.c`: an overlay surface with a centred card, the
  collection as a flow-box grid of thumbnails decoded one per idle (so opening
  it never waits on a directory of 4K files), the count and the
  not-drawing warning, F11 or Ctrl+F to expand to the output, Escape or a click
  outside to close, and Enter on a focused tile.  A click saves the picture as
  the `wallpaper-selected` GUI preference `(whelk prefs)` already layers over
  `#:current`, then repaints the background immediately.
- `src/preferences.rs` — the settings window, ported from
  `src/preferences.c`: one stack of nine pages (Bar, Theme, Night light,
  Notifications, Wi-Fi, Global menu, Modules, Launcher, More) over the same
  `(whelk prefs)` overrides the C window writes.  A control's value is written
  back as the Scheme it came from (`#t`, `12`, `0.75`, `"Inter"`, `'cua`), a
  module's key is its kind, occurrence and option (`module-clock-0-format`),
  and only an option one field can represent unchanged is offered.  Typing
  settles for 180 ms before one write and one reload; Reset deletes the
  overrides and redraws from what the shell now reads.  The model (keys, clock
  text and parsing, launcher labels, the module walk) is pure and tested
  without a display.  The window is built on first open -- `--settings` or the
  menu's Control Panel row -- never at startup.
- `src/bar.rs` — the typed view of `(shell-bars->alists)`, ported from
  `bar_from_alist`: edge, layer, monitor rules, geometry, material, launcher
  defaults and ordered module specs with typed options.
- `src/style.rs` — the C surface skin generator: CSS-safe widget ids, colour
  alpha handling, material backgrounds, effects, shadows, accent rules and the
  configuration's free-form CSS.
- `src/chord.rs` — the C chord notation: Emacs (`C-S-p`) and CUA
  (`Ctrl+Shift+P`) parsing, GTK accelerators, the key alias and display tables,
  key sequences, formatting, and the `wtype`/`xdotool` command strings.
- `src/fuzzy.rs` — the launcher's subsequence scorer, with the C weights for
  word starts, consecutive runs and short names, and its best-candidate pick.
- `src/calc.rs` — the `=` mode calculator: the same recursive-descent grammar,
  refusals, and the `%.10g` printer the C answer is read through.
- `src/launcher.rs` — the launcher's display-free geometry: scroll value that
  follows the selected row, click-outside hit test, and the card's zoom bounds.
- `src/launcherkeys.rs` — the launcher's mode prefixes as data: the built-in
  map, overrides that can replace or disable a prefix, and the longest-prefix
  rule.
- `src/launcherdata.rs` — the launcher's searchable databases: XDG desktop
  entries and their shadowing rules, executable PATH commands, SSH hosts,
  niri windows, file rows, both menu-script protocols, the built-in session
  menu, fuzzy ranking and web-query escaping.
- `src/launcherpanel.rs` — the C launcher overlay as a Rust surface: a
  full-output layer surface with a debounced search entry, ranked list,
  keyboard selection/completion, outside-click dismissal, icon/text zoom and a
  resize grip.  Application, window, run, calc, file, web, menu, SSH, quit,
  menu-item and session modes are wired; clipboard history and live
  application menu items are Rust too.
- `src/clipboard.rs` — the clipboard history and service, ported from
  `src/clipboard.c`: stable SHA-1 content ids, duplicate promotion, pinning,
  separate count/byte budgets, TTL expiry, single-removal undo, v1/v2 GVariant
  persistence, secret-manager refusal, MIME preference, the binary-safe
  `WHELK1` helper framing, the `wl-paste --watch` listener, GTK selection
  providers with `wl-copy` fallback, and the logind lock watch.  The service is
  installed once by `ui` and shared by the launcher and bar; its data half has
  no GTK dependency and is tested without a display.
- `src/clipboardbar.rs` — the `clipboard` bar module: the live count, status
  classes, picker click, and the right-click menu for pause/resume, clear
  unpinned, clear-all confirmation, restart and opening the picker.
- `src/startmenu.rs` — the shell's own menu as data: `(whelk menu)` units become
  items, disabled rows, separators, submenus and the `shell:recent-items` slot,
  plus the default/fallback menus and the rule that `whelk --reload` and
  `whelk --quit` stay in the running process instead of spawning a second
  shell.
- `src/startbar.rs` — the `start` bar module: the drawn whelk mark (or an
  `#:label` glyph), the configured menu popover, Recent Items filled from
  GTK's recent manager when it opens, Up/Down/Right/Escape navigation in every
  submenu, and the shell actions (`launcher`, `force-quit`, `network`,
  `bluetooth`, `notifications`, `dnd`, `lock`, `reload`, `quit`, `about`).
  Commands from the menu run through the shared asynchronous runner.  The
  Control Panel row's `shell:settings` asks the application for the Rust
  settings window, which that first request builds and every later one
  redraws from a fresh snapshot.
- `src/about.rs` — the menu's About This System window, using the Rust
  host-info readers: host and OS header, processor/memory/display/disk/kernel/
  session/uptime rows, a copyable report (button or Ctrl+C), Escape to close,
  and the bar's own solid palette.
- `src/filesearch.rs` — the launcher's file mode: case-insensitive ranking that
  puts a basename match before a parent-directory match, shallower paths before
  deeper ones, files before directories, and the `find` command that prunes the
  caches nobody wants searched.
- `src/barmonitors.rs` — a bar's monitor rule: the output-count condition,
  connector lists, `all`/`default`, and the rule that a missing named screen
  draws nothing rather than moving to another panel.
- `src/format.rs` — the text helpers every module renders with: `{key}`
  substitution, Pango escaping, icon markup with its font family, UTF-8-safe
  truncation, first-line extraction and binary sizes.
- `src/level.rs` — the change detector behind a volume or backlight HUD: the
  first reading is not a change, and a mute toggle at the same number is.
- `src/json.rs` — the small JSON reader niri and Hyprland answers are read
  through: objects keep key order, arrays keep document order, escapes and
  surrogate pairs decode to UTF-8, and a malformed document reports its offset
  instead of yielding a partial tree.
- `src/session.rs` — session detection, naming, probes and focused-window
  knowledge: niri, sway, Hyprland and river socket variables first, X11 from
  `XDG_SESSION_TYPE` or a `DISPLAY` with no `WAYLAND_DISPLAY`, then a bounded
  probe of each compositor when the environment names none. Also the
  focused-window commands and per-session parsers, process ids, X11 GTK bus and
  menu paths, the close/fullscreen/maximize/float/move commands, and the
  `wtype`/`xdotool` shortcut lookup.
- `src/riverstatus.rs` — River's status push, ported from `util/riverstatus.c`:
  its own Wayland connection binds `zriver_status_manager_v1` and one
  `zriver_seat_status_v1` per seat, generated from
  `protocol/river-status-unstable-v1.xml` by `wayland-scanner` at cargo build
  time.  `focused_view` carries the title the `window` module shows; the watch
  and the proxies live exactly as long as the widget that started them.
- `src/workspaces.rs` — the two workspace answers through one shape: niri's
  `idx`/`is_active`/`active_window_id` and Hyprland's `id`/`windows` plus the
  active workspace on its monitors, with special workspaces left out and the
  focus commands for both.
- `src/barmodule.rs` — the lifecycle every Rust bar module shares: the C
  interval defaults, one command in flight, a five-second default deadline,
  explicit cancellation, request tokens that reject stale answers, common
  click/scroll options, CSS classes and a root widget that cancels its command
  when GTK destroys it.
- `src/barfactory.rs` — the Rust equivalent of the C factory table: a kind is
  looked up without building a widget, an unknown kind is logged and left out,
  and the module receives only the session/bar facts it needs.
- `src/nightlight.rs` — the C night light, ported whole: the normalised
  blackbody curve, 1000–6500 K clamping, overnight and short-window schedules,
  fade strength, next-tick policy, strict `HH:MM` parsing, and typed
  `Rgb`/`GammaRamp` values.  `nightlight/engine` is C's controller (apply only
  when the colour changed, neutral once after the filter touched the screen),
  `nightlight/wayland.rs` is the wlr-gamma-control backend (controls only while
  warm, released on neutral, a refused output retried after 30 s) and
  `nightlight/x11.rs` is the RandR backend (per-CRTC originals restored on
  neutral and shutdown).  `nightlight/runtime.rs` owns the GLib timer and the
  reload/teardown hooks.
- `src/osd.rs` — the C on-screen display as a Rust layer-shell surface: one
  shell-owned HUD, bottom placement relative to the bar, a 1400 ms hide timer,
  clamped progress with the unbounded percentage label, and restyling when the
  configuration reloads.  The `audio` and `backlight` modules now use the
  existing `Level` detector to show it only on real changes.
- `src/notifications.rs` — the notification store and the
  `org.freedesktop.Notifications` session service.  The oldest-first store
  assigns stable ids, replaces a notification in place for `replaces_id`,
  resolves the C default/critical timeout rules, and keeps DND in the existing
  idle manager.  The service exports `Notify`, `CloseNotification`,
  `GetCapabilities` and `GetServerInformation`, emits `NotificationClosed` and
  `ActionInvoked`, and publishes changed/posted/closed events to in-process
  listeners.  It remains alive, with its store usable, when another daemon owns
  the bus name.
- `src/notificationcenter.rs` — the C notification centre and toast stack as
  Rust layer-shell surfaces.  History is grouped by application with the newest
  card on top, the header carries DND/clear/close, malformed markup falls back
  to plain text, and cards invoke their default/action buttons through the
  service.  The corner shows at most five toasts, replaces duplicates, opens
  the panel from a toast click, and expires transient notices on their own
  timers.  It restyles both surfaces when the configuration reloads.
- `src/networkpanel.rs` — the C network panel as Rust layer-shell surfaces:
  the Wi-Fi page reads nmcli's radio, active SSID and terse access-point list,
  dedupes and sorts it, and connects through a saved profile or a passphrase.
  Typed keys travel on stdin through `nmcli --ask`/`connection edit`, never in a
  command line.  The Bluetooth page lists paired devices, marks the connected
  ones from bluetoothctl, and connects or disconnects them.  Both pages have a
  power switch, rescan, status/empty readings and the C refresh delays, and the
  panel takes keyboard focus on demand for the passphrase entry.
- `src/notificationsbar.rs` and `src/dndbar.rs` — the `notifications` and `dnd`
  bar modules.  They read the service's count and DND flag, repaint from a weak
  listener when either changes, and keep their configured format/icon/tooltip
  overrides.  A bare left click on the count toggles the notification centre;
  a bare right click toggles DND.  The `dnd` module's bare left click does the
  same, and `#:show-off?` keeps it visible while DND is off.
- `src/config.rs` — the loaded wallpaper/bar/idle/shell-menu snapshot and its
  reload boundary.  The Guile handle lives with the shell, so a second load reuses it;
  a missing file means the built-in bar, and a failed reload keeps the previous
  windows instead of partially applying a broken configuration.
- `src/barmonitors.rs` also plans concrete windows: every bar configuration is
  evaluated against the current connector list, producing one placement per
  selected output (or the compositor-chosen sentinel when there is no monitor
  model).  The pure plan is tested without a display.
- `src/workspacebar.rs` — the configured `workspaces` module in the Rust bar:
  session detection once at startup, the niri/Hyprland command and JSON shape
  for that session, `{index}`/`{name}` labels, `hide-empty?`, and a click that
  spawns the matching focus command.  It runs on the shared module host, so its
  timer, deadline, cancellation and mouse actions are the same as every later
  module.  A malformed answer leaves the last good row in place; an interval of
  zero polls once instead of starting a timer.
- `src/clockbar.rs` — the `clock` module: a strftime `#:format`, the C default
  `%a %d %b  %H:%M`, and a month-calendar tooltip built with Monday-first
  columns.
- `src/windowbar.rs` — the `window` module: the session's focused-window
  command and parser, `{title}`/`{app-id}`, UTF-8-safe `#:max-length` truncation
  (50 by default) and an end-ellipsized label.  River has no focused-window
  command, so this module subscribes to `riverstatus` instead and presents each
  pushed title.
- `src/custombar.rs` — the `custom` module: `#:exec`, the first line of its
  output, `{text}` and an optional `#:max-length`; `#:interval 0` makes it a
  static, clickable reading.
- `src/audiobar.rs` — the `audio` module: the wpctl default-sink query with the
  pactl fallback, Volume/mute parsing, 0–150 clamping, `{volume}`, and
  `#:muted-format`/`#:muted-icon` overrides.
- `src/batterybar.rs` — the `battery` module: a testable sysfs reader (explicit
  `#:device` or the first `type=Battery` entry), capacity clamping, hiding when
  the pack is absent, charging/warning/critical classes, and the C charging
  format/icon rules.
- `src/wifibar.rs` — the `wifi` module: nmcli's radio state and active terse
  row, `{ssid}`/`{signal}`/`{state}`, the SSID-hiding default, the off and
  disconnected readings, and `off`/`connected` classes.  A bare primary click
  opens the network panel's Wi-Fi page.
- `src/bluetoothbar.rs` — the `bluetooth` module: bluetoothctl's adapter power
  and connected-device count, `{count}`/`{state}`, the off reading, and
  `off`/`connected` classes.  A bare primary click opens the Bluetooth page.
- `src/backlightbar.rs` — the `backlight` module: the first panel under
  `/sys/class/backlight` (or `#:device`), rounded/clamped 0–100 percentages,
  `{percent}`, and hiding when the machine has no panel.
- `src/cpubar.rs` — the `cpu` module: `/proc/stat`'s aggregate line, the idle
  plus iowait subtraction, two-sample deltas, counter-reset recovery and the
  C module's rounded `{usage}` percentage.
- `src/memorybar.rs` — the `memory` module: `/proc/meminfo` totals and
  availability, rounded percentages, swap used/total and every `{used}`,
  `{total}`, `{avail}` and `{swap-*}` value formatted as GiB.
- `src/powerprofilebar.rs` — the `power-profile` module: powerprofilesctl's
  first line as `{profile}`, with the module hidden when the daemon has no
  answer.
- `src/keyboardbar.rs` — the `keyboard` Caps Lock module: the first
  `::capslock` LED in sysfs, hidden while unlocked and `{state}`/the `locked`
  class while lit.
- `src/languagebar.rs` — the `language` module: niri's active-layout marker,
  the active display name, and `#:codes` turned into the uppercase XKB code
  with `{text}`/`{name}`.
- `src/inhibitbar.rs` — the `inhibit` readout on top of the existing
  `inhibitors` parser: count, `show-off?` visibility and the inhibitor summary
  as the tooltip.
- `src/command.rs` — the asynchronous program runner: shell command lines,
  optional stdin for secrets that must not appear in `ps`, exit status, one
  byte-exact reader for NUL-separated menu output, the explicit `WHELK_RETV`
  prefix, fire-and-forget spawns, and timed commands whose handle cancels and
  kills the child.
- `src/hostinfo.rs` — the About window's account of the machine: the
  os-release name, CPU model and thread summary (skipping per-core index
  lines), memory total and availability, root-filesystem usage, kernel,
  uptime sentences, session/protocol summary and the first monitor's display
  summary.
- `src/nmcli.rs` — NetworkManager's escaped terse output: field splitting that
  honours `\:` and `\\`, Wi-Fi profile names, the SSID-hiding reading and
  tooltip, and the literal editor script that carries a Wi-Fi key on stdin.
- `src/applets.rs` — the exact StatusNotifierItem ids of network and Bluetooth
  applets a bar already replaces, with case-insensitive, trimmed matching and
  no substring near-misses.
- `src/menuprobe.rs` — the global menu's ordered fallback chain: exact X11 GTK
  paths before conventional paths, the registrar only when a window id exists,
  and DBusMenu paths with the registrar's answer moved to the front.
- `src/menuwatch.rs` — the live follow-up both protocol producers publish,
  ported from `util/gtkmenu.c` and `util/dbusmenu.c`: which change signal
  belongs to which protocol, the 120 ms debounce a burst is folded into (a GTK
  burst measures from its last signal, a DBusMenu burst from its first), the
  single follow-up read a change during a read owes, and the rule that a fresh
  tree replaces the one on screen only when it differs.  An empty
  `org.gtk.Menus` answer withdraws the application's menus; an empty
  `GetLayout` is a failed read and keeps what is showing.
- `src/menusearch.rs` — the focused application's live menus flattened for the
  launcher: one row per reachable entry with its `View > Appearance > Dark`
  path, chord and an owned action plus the bus context it was published with,
  separators/disabled/unactionable rows left out, a shell-wide snapshot the
  menu bar publishes on every rebuild, and the activation the launcher's
  menu-item mode runs.
- `src/menubar.rs` — the global menu as owned data: the configured
  `Menu<TAB>Item<TAB>Chord<TAB>Command` protocol with `>` submenus, the
  `org.gtk.Menus` `a(uuaa{sv})` reader with sections, accelerators, targets and
  cycle guards, the DBusMenu menubar reader shared with the tray, and the
  shared Edit/View table.  Every actionable row has an owned `MenuAction`; no
  widget holds a `GVariant` pointer.
- `src/menuapps.rs` — the application-specific fallbacks: the curated Scheme
  dictionary, the Orbit corpus, desktop-entry actions and `Exec` commands, the
  case-insensitive id rules, and the File/Edit/View/Window merge.  It also owns
  the data model for a dictionary item's short Emacs form and long
  accelerator/modifier/key form.
- `src/shortcutmaps.rs` — user-installed Orbit JSON maps, ported from
  `util/shortcutmaps.c`: `WHELK_SHORTCUT_MAPS` or the XDG/system
  `global-menu/shortcuts/apps` directories, deterministic path order, the
  identifier index and lazy file reads, nested submenus, separators, chords in
  either notation, GTK action rows, disabled targets and placeholders, and the
  blank-map answer.  A matching map wins over the built-in corpus but loses to
  the curated dictionary.
- `src/atspi.rs` — the accessibility producer: a bounded, display-free tree
  walk (roles, names, children, state and key bindings) plus the live
  accessibility-bus watch.  The tests cover an owned fake tree and a fake
  application on a private D-Bus, including object-path child references and a
  live `ChildrenChanged` refresh.
- `src/globalmenu.rs` — the `menu` bar module: focused-window polling, the
  script/GTK/DBusMenu/AT-SPI producer chain, application-name and top-level
  popovers, keyboard navigation, hover switching, `org.gtk.Actions.Activate`,
  DBusMenu `Event`, desktop-action commands and accessibility activation.
  Asynchronous answers carry a generation so a slow application cannot
  overwrite the application in front.
- `src/tray.rs` — the tray's display-free rules: watcher entries as service and
  path, `#:hide` word fragments, exact network/Bluetooth applet suppression,
  the item limit and overflow decision, and validated ARGB32-to-RGBA pixmap
  conversion.
- `src/traymenu.rs` — the `com.canonical.dbusmenu` layout reader: owned rows
  with labels, enabled state, accelerators, separators, submenus and the ids
  that become `Event` calls, with malformed replies refused rather than
  panicking.
- `src/traybar.rs` — the `tray` bar module: the StatusNotifierWatcher watch and
  host registration, initial and live item lists, per-item D-Bus proxies,
  icon names/theme paths/pixmaps, automatic and configured hiding, the
  overflow chevron, `Activate`/`ContextMenu`, and the applet menu popover.
- `src/trayicon.rs` — a tray item's own icon directory: absolute theme paths
  only, no name that escapes the directory, and precedence for an exact name,
  PNG, SVG and XPM.
- `src/lockpolicy.rs` — the C locker's throttling policy: the doubling
  retry delay with its 30-second cap and the one authoritative
  "may this attempt be submitted" guard.
- `src/idlepolicy.rs` — the idle manager's timer decision, without the bus or
  the wall clock: the zero-second compatible policy, a countdown from the
  first `IdleHint`, an inhibitor suspending it, and a disabled policy.
- `src/inhibitors.rs` — the `systemd-inhibit --list` reader: padded-column
  parsing that keeps spaces inside a reason, and the one-line-per-inhibitor
  tooltip text for "why did my screen not blank".
- `src/idle.rs` — the idle and inhibit manager: logind's `IdleHint` and session
  lookup, the policy timer, a real logind idle inhibitor held as an fd, DND
  ownership with its change callback, and the status report that reads
  `systemd-inhibit`.  The Rust shell starts one and wires its lock action to
  the Rust lock screen.
- `src/lockauth.rs` — the C lock screen's authentication boundary without GTK:
  libpam loaded at run time from the host's library first, with the host
  dependencies (`libaudit`, `libeconf`) resolved by absolute path when a store
  build cannot find them, the per-user `pam_unix` service written with
  `0700`/`0600` and `O_NOFOLLOW`, and the PAM status classification.  The host's
  setuid `unix_chkpwd` is a helper of last resort: it is never asked when a
  `whelk-lock` or `login` service file exists, and `WHELK_LOCK_STRICT=1`
  disables it entirely.  `--check-lock` prints what it would do.
- `src/logind.rs` — logind's lock triggers: the manager and per-session
  `Lock` signals, `PrepareForSleep(true)`, and the session's `Active=false`
  change, with the D-Bus subscriptions and the payload policy kept apart so the
  latter is testable without a bus.
- `src/locker.rs` — the Rust lock screen: a compositor-enforced session lock
  through `ext-session-lock-v1` when the compositor has it, one overlay layer
  surface per output otherwise, one password field on the primary output,
  authentication on a worker thread with the password wiped, the persisted
  retry delay, and no close button, Escape or click-outside.  The windows
  follow the monitor model: the compatibility surfaces rebuild from it on a
  hotplug, and a protocol lock whose primary output disappears is rebuilt so
  the password field lands on an output that remains.  One `RebuildState`
  queues the burst, drops the churn the rebuild itself causes, and refuses
  once the lock has ended.  `watch_logind()` starts the lock once per logind
  trigger, reusing a still-running process.
- `src/guile.rs` and `src/guile/sys.rs` — a small libguile bridge. It initializes
  Guile, prepends the checkout's `scheme/` directory to `%load-path`, registers
  `(whelk-rust-version)` as a Rust primitive, loads the real
  `(whelk core)`/`(whelk state)` modules, reads the wallpaper and bar alists,
  and walks the shell menu's tagged units (falling back to
  `(default-shell-menu)` when the file names none). Nested alists and
  exact/inexact numbers survive the typed conversion.
- `src/guile/widget.rs` — the Scheme widget bridge, ported from the widget half
  of `src/guile_bridge.c`: the nine `whelk-widget-*` primitives (label, markup,
  progress, level, sparkline, image, box, button, add-class) registered into
  `(whelk core)` when Guile is initialized, and the call a `scheme-module`
  makes.  A widget crosses the boundary as an exact-integer handle into a
  thread-local registry, not a smob: the values crossing the ABI are ones
  `sys.rs` declares, and a stale handle is simply `None`.  A primitive can
  return a string, number or symbol instead, which becomes a label, as the C
  bridge accepts.
- `src/schemebar.rs` — the `scheme` bar module, ported from
  `modules/scheme.c`: `#:update`'s token, the one-second refresh, the slot the
  current widget is swapped into, and the rule that a failed or empty reading
  keeps the previous widget.  A module configured without `#:update` says so in
  the bar instead of polling.
- `src/widgets.rs` — the desktop widgets' data, ported from the display-free
  half of `src/widgets.c` and the `(whelk state)` records behind it: the nine
  kinds and their standard sizes, geometry defaults and clamps, the mouse
  vocabulary and per-kind cadence, the reminders model whose done flags are
  runtime state, world-clock city parsing, Open-Meteo and Yahoo answer
  decoding, the calendar's Monday-first month grid, a note's file-or-text
  rule, a custom reading's empty fallback, and the drag/placement arithmetic
  (snap to the 8 px grid, clamp into the monitor, a saved position winning
  over the settings file).
- `src/widgetstate.rs` — the placement and per-widget state file, ported from
  `util/widgetstate.c`: `$XDG_STATE_HOME/whelk/widgets-<stem>.ini` (or
  `~/.local/state`), positions stored per monitor with a monitor-less
  fallback, the `GKeyFile` format the C shell also writes, `0700` directories,
  and the snap/clamp rules both shells share.
- `src/desktopwidgets.rs` — the cards themselves, ported from `src/widgets.c`:
  one bottom-layer surface per configured widget, anchored top-left at its
  saved or configured place, its skin generated from the first bar's palette,
  its grip a primary-button drag that snaps and clamps into the monitor it
  belongs to, and its own mouse vocabulary.  Each kind paints the data
  `src/widgets.rs` decodes: the analog face and digital reading, the month
  calendar with its navigation, Open-Meteo's current conditions, the battery
  level, a note, the reminder rows whose clicks write runtime state, the world
  clock, the delayed quotes and a custom command's output.
- `src/ui.rs` — a GTK4 top bar per selected monitor, whose edge, layer, margins,
  thickness, clock format and generated skin come from its Scheme bar. Every
  configured left, centre, right and overflow module goes through the factory; the
  bootstrap name, wallpaper summary and clock remain only where the
  configuration leaves them empty, and a configured `clock` is not duplicated
  by the fallback.  The configured overflow row is built with the bar and
  opens behind its chevron; the right row folds its trailing readings into a
  `+N` popover after the first allocation, keeping the modules alive so their
  timers and clicks still work.  A right-click on the bar's own gaps opens the
  shell menu: Edit settings and Reload settings; a module's right-click stays
  the module's.  It watches the settings directory with a 250 ms debounce,
  watches the GDK monitor model for hotplug, and rebuilds by destroying the old
  windows (and their module hosts/CSS/clock timers) before building the new
  plan.  It also owns the one clipboard service: starts it before the first
  bar, applies a reloaded policy, and flushes it during teardown.  It uses `gtk4-layer-shell` on Wayland and falls back to normal windows
  on Broadway/X11 test displays.
- `src/main.rs` — `--version`, `--check-config`, `--config PATH`, `--launcher`,
  `--launcher-mode MODE`, `--clipboard`, `--wallpaper-picker`, `--settings`,
  `--help`, plus
  the lock screen's `--lock` and `--check-lock`.  `--clipboard-store` is the
  internal helper entry point `wl-paste --watch` runs against this same binary.

## Bar modules

The Rust bar now builds the daily-use readings from the same module lists as
the C shell.  Put them in whichever half the configuration names:

```scheme
#:left   (list (workspaces-module #:interval 1
                                  #:hide-empty? #f
                                  #:format "{index}")
              (window-module #:max-length 50))
#:center (list (clock-module #:format "%a %d %b  %H:%M"))
#:right  (list (audio-module #:format "{icon} {volume}%")
              (battery-module #:warning 30 #:critical 15)
              (wifi-module #:show-ssid? #f)
              (bluetooth-module))
```

`workspaces` uses the session detected once at startup: on niri it runs
`niri msg -j workspaces`; on Hyprland it runs the workspaces and monitors
queries together.  Labels are built from `{index}` and `{name}`, empty
workspaces follow `#:hide-empty?`, and a primary click runs the session's focus
command.  `#:interval 0` paints the row once and starts no timer.  Other
sessions have no workspace IPC, so the module stays empty rather than guessing.

`window` uses the same session's focused-window command and parser, filling
`{title}` and `{app-id}`.  A session with no focused-window answer leaves the
module empty; on River the title arrives as a push on the `river-status`
protocol instead of a command.  `clock` is a strftime
pattern, and its tooltip is the month calendar.  `custom` runs `#:exec` and
shows its first line.  `audio` prefers wpctl and falls back to pactl; `battery`
reads `/sys/class/power_supply`; `wifi` and `bluetooth` read nmcli and
bluetoothctl.  `backlight`, `cpu` and `memory` read sysfs/proc directly;
`power-profile`, `language` and `inhibit` run powerprofilesctl, niri and
systemd-inhibit; `keyboard` reads the Caps Lock LED.  `notifications` and `dnd`
read the in-process notification service, which shares the idle manager's DND
flag.  `clipboard` reads the shell's own history and opens the same picker as
`whelk-rs --clipboard`; its right-click menu pauses, restarts, clears or opens
that picker.  `start` draws the whelk mark and opens the configured shell menu;
its Recent Items submenu is live, and a shell item it cannot perform yet is
reported on stderr instead of closing silently.  The configured `#:overflow`
list is built too and expands from the chevron at the end of the right group.

Every module built by the host accepts the shared mouse options
(`#:on-click`, `#:on-click-right`, `#:on-click-middle`, `#:on-scroll-up`,
`#:on-scroll-down`) and `#:css-class`.  `#:command-timeout` is a deadline in
seconds for the module's command, defaulting to 5; a timed-out or cancelled
command leaves the last good reading in place.  Set it to 0 only for a command
that is known to finish.

The Wi-Fi and Bluetooth modules open the Rust network panel on a bare primary
click, like their C counterparts; an explicit `#:on-click` still wins.  The
`notifications` module's bare left click toggles the Rust notification centre,
while an explicit `#:on-click` still wins; its bare right click is the C DND
toggle, and `dnd` is a working toggle without the centre.

## The global menu

`menu` is the focused application's menu bar, in the same order a desktop menu
bar uses: the application name (when `#:app-name?` is on), then the application's
File/Edit/View menus.  The first producer that answers wins:

1. `#:menu-command` — a program that prints
   `Menu<TAB>Item<TAB>Chord<TAB>Command` lines.  `File>Export` nests a submenu,
   `-` in the item column is a separator, and an empty command is a disabled
   row.  It runs asynchronously with the module's `#:command-timeout` deadline
   (five seconds by default); its answer replaces the fallback only when it
   produced at least one row.
2. `org.gtk.Menus.Start` on the focused bus name, trying the exact X11 GTK
   paths first and then the conventional application paths.  Sections are
   flattened into the bar, a top-level item without a submenu becomes a
   one-row menu, and `action`, `target`, `accel` and `enabled` are preserved.
3. `com.canonical.dbusmenu.GetLayout`, after the X11
   `com.canonical.AppMenu.Registrar` has had a chance to name the exact bus and
   object path.  Choosing a row sends `Event(id, "clicked", 0, 0)` to the
   publisher; the id is never treated as a command.
4. AT-SPI, when neither protocol answered.  The accessibility worker walks the
   focused application's menu bar and returns the application's own labels,
   accelerators, enabled state and submenus.  It follows
   `org.a11y.atspi.Event.Object` changes after a short debounce, so a menu that
   grows while it is open is read again.  Choosing a row calls
   `org.a11y.atspi.Action.DoAction` on the accessibility bus.
5. The application fallbacks.  A matching curated dictionary entry comes
   first: its File menu is filled with the application's desktop actions, New
   Window from its desktop `Exec`, and Close Window before Quit; its other
   menus follow.  When the dictionary does not know the application, a
   user-installed JSON shortcut map is tried first; otherwise the shipped
   Orbit corpus is used.  Either replaces the shared Edit/View rows it
   describes while keeping Window.  The last resort is the shared Edit/View
   table (only when `#:shortcuts?` is on and a typing tool exists), Close
   Window, and the compositor's maximize, float, move and full-screen verbs.

```scheme
(menu-module #:shortcuts? #t
             #:app-name? #f
             #:accelerator-style 'cua
             #:shortcut-tool "wtype"
             #:menu-command "my-menus --for-focused-window")
```

The module accepts the shared click options and `#:css-class` from the host, but
its own buttons are a menubar: a click opens a menu, so `#:on-click` is not a
second action on the application name.  `#:accelerator-style` accepts `'cua`
(the default) or `'emacs`; `#:shortcut-tool` is used only when shared shortcut
rows are offered, and is resolved on `PATH`.  `#:shortcuts?` and `#:app-name?`
also accept the older spellings without the question mark.

A focus change, or a change to the X11 bus/object paths published by the same
window, rebuilds the tree.  Every asynchronous answer carries a generation
token; an answer for the previous application is ignored, and removing a widget
cancels its popover, command and accessibility watch.  The menu row hides
itself when it has no buttons rather than leaving a blank stripe in the bar.

A dictionary or Orbit row types the chord its author documented; the helper
receives the focused window's keys through `#:shortcut-tool`.  Desktop actions
are launched from their action `Exec`; a `new-window` action is not repeated
when the entry's ordinary `Exec` already provides New Window.  AT-SPI entries
are activated through the accessibility bus instead of being typed, so they
work even when the application publishes no accelerator.

### Current Rust boundaries

The main producers are ported, with these deliberate gaps:

- Production is followed live.  `org.gtk.Menus.Changed` and DBusMenu
  `LayoutUpdated`/`ItemsPropertiesUpdated` cost one debounced `Start` or
  `GetLayout` through `src/menuwatch.rs`, and a tree identical to the one on
  screen keeps the menu that is open.  A tree that really changed is rebuilt,
  which closes the open menu; the C module remembers the open path and puts it
  back.  `LayoutUpdated` does not close the menu by itself: an application
  sends it while it builds the menu somebody just opened.
- The application name comes from the dictionary, then the desktop entry's
  `Name`, then the id's last component.  A dictionary name still wins over the
  desktop entry, matching the C order.

User-installed Orbit JSON maps are read by the Rust producer as well as the C
one.  The Rust index is loaded with each configuration reload; a map that
changes on disk becomes visible on the next reload.

## Monitors, hotplug and reload

Each configured bar is placed independently by its `#:monitor`, `#:monitors`
and `#:when` rule.  `all` creates one window per output, a connector name waits
for that output, `default` leaves placement to the compositor, and an empty
monitor model still gets one compositor-placed surface for `all`/`default`.
The shell holds the GTK application so a bar that is waiting for an external
screen does not exit the process; when the monitor model changes, it schedules
one idle rebuild and evaluates every rule again.

The settings directory is watched, with a 250 ms debounce so one editor save is
not several rebuilds.  A successful reload replaces the complete bar set; a
failed load logs the error and keeps the previous bars.  The shell's `reload`
application action uses the same path, so the start menu's reload row rebuilds
the bars and their menus without starting a second process.  Reload currently
refreshes the bars, their generated CSS, their modules, the idle policy and the
wallpaper summary shown by the fallback centre label.  The wallpaper surface
itself and the long-lived panels are still started once and are not restarted
by a bar reload.

## Requirements

The build needs the same native stack as the C shell:

- Rust 1.92 or newer
- GTK 4.18 or newer (`gtk4.pc`)
- GNU Guile 3.0 development files (`guile-3.0.pc`)
- gtk4-layer-shell 1.0 or newer (`gtk4-layer-shell-0.pc`)

Cargo and the Guile library are both resolved through `pkg-config` (Guile in
`build.rs`); there is no vendored C shim.

## Commands

The repository root is the Cargo workspace, as in lambdock: `rust/` is the
shell, `xtask/` is the packaging task, and both share one `target/`.
`.cargo/config.toml` aliases `cargo xtask` to `cargo run -p xtask --`.  Run from
the workspace root:

```sh
cargo test -p whelk-rs
cargo clippy -p whelk-rs --all-targets -- -D warnings
cargo run -p whelk-rs -- --version
cargo run -p whelk-rs -- --check-config
cargo run -p whelk-rs -- --config /path/to/settings.scm
cargo run -p whelk-rs -- --check-lock
cargo run -p whelk-rs -- --lock
cargo run -p whelk-rs -- --launcher
cargo run -p whelk-rs -- --launcher --launcher-mode=window
cargo run -p whelk-rs -- --clipboard
cargo run -p whelk-rs -- --wallpaper-picker
cargo run -p whelk-rs -- --settings
cargo xtask install --prefix /usr/local --destdir /tmp/whelk-stage
# stages whelk-rs, the Scheme modules, the Rust desktop entry and the shared icons
cargo xtask dist --output /tmp
```

The `xtask` target is the lambdock-style packaging task: `install` builds the
release `whelk-rs` and stages the binary, the shared `scheme/whelk` modules and
this README under the requested prefix and `DESTDIR`; `dist` writes the Git
source archive.  It does not install the C `whelk`, and the Meson/Nix/Guix/
Debian recipes still build the C shell while its modules are ported.  Use
`--no-build` to stage an existing target, and `--debug` to stage the debug
profile.  A custom prefix needs `WHELK_SCHEME_DIR` until the Rust binary becomes
the installed `whelk` and its final data path is settled.

`--check-lock` is the lock screen's diagnosis without a password: the library
candidates, the one that loads, the service stack that would answer, and
whether this machine has a setuid helper for the store-built case.  `--lock`
shows the Rust lock screen itself; like the C one, only a successful
authentication for the session user removes it.

`--check-config` loads the real settings file through Guile, prints the
wallpaper policy, walks the collection and resolves the current file. A missing
file is the C shell's "no configuration" case and uses defaults; a file that
Guile rejects exits non-zero with the reason.

`--launcher` opens the Rust launcher after the shell has started.  A second
mode can be selected with `--launcher-mode`; the built-in mode prefixes still
work while typing (`>`, `=`, `/`, `?`, `@`, `%`, `!`, `;`, `:`).

`--clipboard` is the shortcut for `--launcher --launcher-mode=clipboard`.  The
clipboard service follows the same policy as the C shell: `(clipboard-settings)`
sets the limits, TTL, persistence file and lock clearing; an unconfigured file
still honours `WHELK_CLIPBOARD_PERSIST` (`1`/`auto` for the state-directory
file, or an absolute path).  Pinned copies are never evicted by the count,
byte or TTL rules.  In the picker, `Ctrl+P` pins or unpins the selected copy,
`Ctrl+Delete` removes it, and `Ctrl+Z` restores the last removal.

With a valid configuration, `whelk-rs` presents the Rust bar and a Rust
background surface. On Wayland it uses `zwlr_layer_shell_v1`'s background
layer; on Broadway or X11 it falls back to an ordinary window for development
and smoke tests. The C `whelk` binary remains the installed shell and keeps its
own application id, namespace and background engine.

When running from `rust/`, the binary finds `../scheme` automatically. From
another directory set `WHELK_SCHEME_DIR` to the checkout's `scheme/` directory or
install the modules where Guile can find them. The C app id is not used, but if
another `whelk-rs` is already running, use `dbus-run-session -- cargo run` for an
isolated smoke test.

## Tests

The tests are split by what they can prove without a session:

| Test | What it covers |
| --- | --- |
| `tests/policy.rs` | C parity for extensions, transitions, fit math, easing and rotation |
| `tests/settings.rs` | Defaults, clamps, unknown values, HOME expansion, XDG path rules |
| `tests/collection.rs` | Recursive walk, hidden entries, symlinks, depth limit, video filter, dedupe and sorting |
| `tests/rotation.rs` | Selection fallbacks, picker selection, sequential/shuffled rotation and clock guard |
| `tests/wallpaperpicker.rs` | Tile names, the header count, thumbnail and card sizes (including a small monitor and a headless one), and the Scheme string literal a saved choice is written with |
| `tests/preferences.rs` | Scheme value formatting, module override keys and occurrence numbering across the bar, which options a single field may edit, clock text and parsing, launcher labels, and colour text |
| `tests/locker.rs` | The lock screen's rebuild queue: one rebuild per burst, the churn a rebuild causes dropped, and a stopped lock never building again |
| `tests/wallpaper_view.rs` | Transition progress clamping and the Ken Burns zoom curve |
| `tests/wallpaper_surface.rs` | Animated-frame delay policy (including the 100 ms zero-delay fallback) and the output signature/rebuild rule: connector and resolution changes, a missing connector, and a stale surface count |
| `tests/bar.rs` | C bar defaults, edge/layer/monitor rules, clamps, module lists, typed options and the width-driven fold walk |
| `tests/barmonitors.rs` | Monitor-rule selection, output-count conditions and flattened bar placements |
| `tests/config.rs` | Missing-file defaults (including the built-in shell menu), the configuration label, and reloading a real two-bar config through one Guile handle |
| `tests/style.rs` | CSS ids, colour alpha handling, material/effects/shadows and generated accent rules |
| `tests/chord.rs` | Both notations, named keys and refusals, GTK accelerators, formatting and display, sequences, and the typed commands |
| `tests/launcher.rs` | Search scoring and ordering, calculator evaluation/refusals/formatting, and the card's scroll, hit-test and bounds |
| `tests/launcherdata.rs` | Desktop-entry shadowing, hidden/non-application entries, PATH executable scanning, niri windows, SSH patterns, file rows, menu protocols, ranking and the session menu |
| `tests/clipboard.rs` | MIME and secret policy, binary record framing and malformed input, labels/sizes/ages, and the history's pin, remove, undo and clear operations |
| `tests/guile.rs` | Real `(whelk core)`/`(whelk state)` modules, a real bar and wallpaper, the shell menu and its default, the real menu dictionary and Orbit corpus, repeated loads, caught exceptions |
| `tests/atspi_dbus.rs` | A fake AT-SPI application on a private D-Bus: object-path children, role/state/name/key-binding reads, process-id matching, unknown applications and live `ChildrenChanged` delivery |
| `tests/desktopwidgets.rs` | The monitor a drag may cross (named connector, the surface's monitor, the first one, the headless fallback), the analog clock's hand angles, the battery card's words and classes, and the escaped markup of a reminder row |
| `tests/widgets.rs` | Kind catalogue and sizes, geometry clamps, option/mouse vocabulary, refresh cadence, reminders parsing/toggling/serialisation, world-clock cities, month-grid layout, weather and stock decoding, note/custom fallbacks, drag/placement arithmetic |
| `tests/widgetstate.rs` | State path rules, 8 px snapping, clamping, monitor-aware position round trips and fallback, refused negative/malformed values, per-widget state removal |
| `tests/menuwatch.rs` | Signal classification, the GTK restart and DBusMenu join debounce, the one owed follow-up read, and the replace rules for identical and empty trees |
| `tests/menusearch.rs` | Live menu flattening: nested paths, chords and owned actions, the bus context carried to each row, and separators/disabled/empty/unactionable rows left out |
| `tests/startmenu.rs` | Tagged menu units, disabled rows, submenu slots, the default/fallback menus, reload/quit routing and the mark's box bounds |
| `tests/tray.rs` | Watcher entry splitting, configured and automatic hiding, item limits/overflow and validated pixmap conversion |
| `tests/lockauth.rs` | Candidate order and dedupe, the private PAM service's modes and reuse, refused unsafe runtime directories, the setuid helper's ownership, and that a host library is never reported as missing host policy |
| `tests/cli.rs` | Version, check-config, missing config and rejected config without starting GTK |
| `tests/barmodule.rs` | Module-host defaults, one-request guard, stale tokens, shared actions and the factory table |
| `tests/nightlight.rs` | Blackbody colour and clamping, overnight/daytime/all-day schedules, short-window transitions, mix/next-tick policy, clock parsing, gamma ramps, config normalization and which display server's backend the environment selects |
| `tests/osd.rs` | HUD placement above a bottom bar, progress clamping and the percentage label |
| `tests/notifications.rs` | Stable ids, oldest-first order, timeout resolution, `replaces_id`, removal, icon precedence, service DND, changed/posted/closed events and listener notifications |
| `tests/notificationcenter.rs` | App grouping and newest-first cards, empty summaries, toast queue limit/replacement, toast margins, body-markup validation and local time labels |
| `tests/networkpanel.rs` | Wi-Fi radio/active/access-point parsing, escaped SSIDs, dedupe and signal order; Bluetooth paired/connected sections, malformed rows and both status/empty readings |
| `tests/notifications_dbus.rs` | Opt-in session-bus round trip: name ownership, `GetCapabilities`, `GetServerInformation`, `Notify` and `CloseNotification`; run with `dbus-run-session -- env WHELK_NOTIFICATION_DBUS_TEST=1 cargo test --test notifications_dbus` |
| `tests/barmodules.rs` | Clock calendar, window/custom truncation (including a River pushed title and a blank push), audio parsing, CPU/memory/backlight/battery/Caps Lock fixtures, niri layout parsing, Wi-Fi, Bluetooth and power-profile readings |
| unit tests | Scheme literal escaping, `--config` parsing, text/GTK/DBusMenu menubar parsing, the dictionary/Orbit/desktop/shortcut-map fallbacks and their id rules, the AT-SPI tree walk over a fake accessibility tree, fallback menus and global-menu app paths |

The Guile test deliberately stays in one test function: Guile is process-global
and thread-affine, and Cargo's test harness runs tests on multiple threads.

## Guile ownership rules

The bridge is safe by construction at its boundary, not by convention:

1. The `Guile` handle is `!Send` and `!Sync`; initialize it on the thread that
   will use it.
2. Any `SCM` that outlives one C call is protected with
   `scm_gc_protect_object` via `Root`, and unprotected in `Drop`.
3. Scheme values are converted to Rust data immediately; no `SCM` is stored in
   application state.
4. Exceptions are caught with `scm_internal_catch` and returned as
   `Result<_, GuileError>`. A Scheme error never unwinds through Rust or GTK.
5. FFI declarations are confined to `src/guile/sys.rs`. It contains only the
   functions this slice calls, with the C signatures reviewed against libguile
   3.0.
6. A widget built by a `whelk-widget-*` primitive crosses the boundary as an
   exact integer handle, never as a pointer: the registry lives in a
   thread-local beside Guile, and taking a handle is how ownership moves from
   Scheme to GTK.  The one procedure kept across calls (a `whelk-widget-button`
   callback) is rooted for exactly the life of the click closure.

`(whelk-rust-version)` and the nine `whelk-widget-*` primitives are the
callbacks from Scheme into Rust.  Every primitive body runs under
`catch_unwind` and answers `#f` instead of unwinding into Guile's frames; the
version primitive allocates a Scheme string from a compile-time constant.

## Unsafe surface

The Rust shell keeps `unsafe` at the boundaries where a C library has no safe
Rust API, and keeps it out of the application logic.  Calls into Guile go
through the safe functions in `src/guile/sys.rs`; `src/guile.rs` and
`src/guile/widget.rs` have no unsafe blocks apart from the `extern "C"`
callbacks Guile calls back into.  The crate exports no `#[no_mangle]` symbols:
Scheme reaches Rust through `scm_c_define_gsubr`, not through a second C ABI.
The inventory is small enough to review:

- `src/guile/sys.rs` — the libguile declarations, protected `SCM` roots,
  `scm_internal_catch` trampolines and the safe wrappers that own UTF-8 copies,
  truth values and pointer checks.
- `src/guile.rs` — the `scm_internal_catch` callbacks, whose contexts are kept
  alive by the safe wrapper for the duration of the call.
- `src/lockauth.rs` — `libpam` loaded with `dlopen`/`dlsym`, the PAM
  conversation callback, and the allocation of PAM's response array.  The
  password is zeroed with `write_volatile` before its buffer is released.
- `src/locker.rs` — the GTK session-lock C API that gtk4-rs does not yet bind;
  the wrapper owns the returned `GObject` and releases it at the same point the
  protocol lock is released.
- `src/guile/widget.rs` — the nine `whelk-widget-*` subr trampolines, the single
  `scm_t_subr` cast they are registered through, the integer handle registry,
  the `scm_internal_catch` around a module procedure, and the rooted procedure a
  `whelk-widget-button` keeps for as long as its click closure lives.  Every
  primitive body runs under `catch_unwind`, so a Rust panic cannot unwind
  through Guile's frames.
- `tests/guile.rs` — one call into the same Guile boundary for the integration
  test.

Everything else — the bar and its modules, the desktop widgets, the global menu, tray, launcher,
panels, notifications, wallpaper, clipboard, session parsing and command
runner — is safe Rust.  The idle manager's logind inhibitor used to keep a raw
`i32` fd and call `libc::close`; it now stores `std::os::fd::OwnedFd` and reads
the handle with `gio::UnixFDListExtManual::get`, so replacing or clearing the
field closes it and `src/idle.rs` contains no `unsafe` at all.

## Wallpaper rendering notes

The Rust surface is deliberately a first slice, not a feature-for-feature copy
of the C engine:

- It creates one background surface per monitor, all showing the same current
  item, and follows the monitor model: a hotplug or resolution change rebuilds
  the set once the model settles, pausing each surface's video stream and
  clearing its view before the window goes away.  A configuration save repaints
  only when the wallpaper settings themselves changed, so a bar-only save does
  not restart a movie.  The picker is Rust too, as `--wallpaper-picker`.
- Still images use `GdkTexture`; animated GIF/WebP images use
  `GdkPixbufAnimation` with the C engine's 100 ms zero-delay fallback. Videos
  use `GtkMediaFile`, loop, and respect `mute-video?`.
- A transition uses the same eased progress, offsets, opacity and zoom math as
  the C view. Ken Burns runs at 60 seconds per breath and 6% zoom.
- The normal-window fallback is for development only; it is not a desktop
  background on Broadway/X11.

## Scope and next steps

This is still a bootstrap, not a replacement of the whole shell. The bar model,
its generated skin, per-monitor placement, config/hotplug rebuilds, the shared
module host/factory, the configured overflow row, and the `workspaces`, `clock`,
`window`, `custom`, `audio`, `battery`, `wifi`, `bluetooth`, `backlight`, `cpu`,
`memory`, `power-profile`, `keyboard`, `language`, `inhibit`, `notifications`,
`dnd`, `tray` and the global `menu` (protocols, dictionary, Orbit, desktop
actions and AT-SPI) are now ported and wired into the Rust bar, and so is the
lock screen:
its policy, PAM boundary, surfaces and logind triggers are Rust.  The start
menu (`start`), its drawn mark, popover and About window are Rust too; only its
settings-window action remains a C surface.  The idle
manager's `IdleHint` watch, inhibitor, timer and DND flag are Rust too.  The
notification service owns the session-bus methods and the store, and the Rust
notification centre owns the panel and toast surfaces.  The Wi-Fi/Bluetooth
network panel, including saved-profile keys and device connect/disconnect, is a
Rust surface.  The launcher overlay is Rust: applications, windows, commands,
calculator, files, web, generic menus, SSH hosts, force-quit and the session
menu are wired; the clipboard history, its bar module, picker mode and the
`--clipboard` shortcut are Rust as well.  The live menu-item mode reads a
snapshot of the focused application's menubar that the global menu module
publishes, so a menu can be found by what it says and chosen without opening
it; each row carries its action and bus context.
The night light is Rust end to end: arithmetic, schedule, controller, the
wlr-gamma-control and RandR backends, and the GLib timer that drives them.  It
opens its own display connection, so a compositor or X server without a gamma
table logs why and keeps the setting for a session that has one.  The volume/brightness HUD is a Rust surface.  The
lock screen follows the monitor model on both of its paths now: the
compatibility surfaces rebuild on a hotplug, and a protocol lock whose primary
output disappears rebuilds its windows from the outputs that remain.  The
global menu follows its
producers live: a `Changed` or `LayoutUpdated` costs one debounced re-read and
an unchanged tree is not rebuilt.  The Scheme widget bridge and the `scheme`
bar module are Rust too, and so are the desktop widgets.  Their kinds,
geometry, state file, reminders, weather and stock decoding and placement
arithmetic live in `src/widgets.rs` and `src/widgetstate.rs`; the cards
`src/desktopwidgets.rs` presents are those values drawn: a layer-shell surface
per widget, its skin, its grip-drag, and every kind's paint and refresh.  They
are rebuilt with the bars, so a settings save or a hotplug moves them too.  The
state file is the one the C shell writes, so a drag in either shell is where
the other one puts the card.  The wallpaper picker is Rust too: an overlay grid
of the collection whose click saves the `wallpaper-selected` preference and
repaints the background, reachable with `--wallpaper-picker`.  The settings
window is Rust too: `--settings` or the menu's Control Panel row builds it on
first use and draws the same nine pages over the same overrides the C window
writes, so a change in either shell is read by the other, and Reset is still
one file deletion.  The next slice is making the Rust
binary the installed `whelk` once the C and Rust suites both cover the same
behavior.

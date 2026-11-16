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
- `src/settings.rs` — the typed view of `(shell-wallpaper->alist)`: defaults,
  numeric clamps, `~/` expansion, and the XDG config path.
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
  from the collection/playlist.
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
- `src/config.rs` — the loaded wallpaper/bar/idle snapshot and its reload
  boundary.  The Guile handle lives with the shell, so a second load reuses it;
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
  (50 by default) and an end-ellipsized label.  River has no Rust push client
  yet, so it stays empty there like a session without IPC.
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
  disconnected readings, and `off`/`connected` classes.
- `src/bluetoothbar.rs` — the `bluetooth` module: bluetoothctl's adapter power
  and connected-device count, `{count}`/`{state}`, the off reading, and
  `off`/`connected` classes.
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
  retry delay, and no close button, Escape or click-outside.  `watch_logind()`
  starts the lock once per logind trigger, reusing a still-running process.
- `src/guile.rs` and `src/guile/sys.rs` — a small libguile bridge. It initializes
  Guile, prepends the checkout's `scheme/` directory to `%load-path`, registers
  `(whelk-rust-version)` as a Rust primitive, loads the real
  `(whelk core)`/`(whelk state)` modules and reads both the wallpaper and bar
  alists. Nested alists and exact/inexact numbers survive the typed conversion.
- `src/ui.rs` — a GTK4 top bar per selected monitor, whose edge, layer, margins,
  thickness, clock format and generated skin come from its Scheme bar. Every
  configured left, centre, right and overflow module goes through the factory; the
  bootstrap name, wallpaper summary and clock remain only where the
  configuration leaves them empty, and a configured `clock` is not duplicated
  by the fallback.  The configured overflow row is built with the bar and
  opens behind its chevron.  It watches the settings directory with a 250 ms debounce,
  watches the GDK monitor model for hotplug, and rebuilds by destroying the old
  windows (and their module hosts/CSS/clock timers) before building the new
  plan.  It uses `gtk4-layer-shell` on Wayland and falls back to normal windows
  on Broadway/X11 test displays.
- `src/main.rs` — `--version`, `--check-config`, `--config PATH`, `--help`.

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
module empty; River's push interface is not ported yet.  `clock` is a strftime
pattern, and its tooltip is the month calendar.  `custom` runs `#:exec` and
shows its first line.  `audio` prefers wpctl and falls back to pactl; `battery`
reads `/sys/class/power_supply`; `wifi` and `bluetooth` read nmcli and
bluetoothctl.  `backlight`, `cpu` and `memory` read sysfs/proc directly;
`power-profile`, `language` and `inhibit` run powerprofilesctl, niri and
systemd-inhibit; `keyboard` reads the Caps Lock LED.  The configured `#:overflow`
list is built too and expands from the chevron at the end of the right group.

Every module built by the host accepts the shared mouse options
(`#:on-click`, `#:on-click-right`, `#:on-click-middle`, `#:on-scroll-up`,
`#:on-scroll-down`) and `#:css-class`.  `#:command-timeout` is a deadline in
seconds for the module's command, defaulting to 5; a timed-out or cancelled
command leaves the last good reading in place.  Set it to 0 only for a command
that is known to finish.

The C Wi-Fi and Bluetooth modules open the network panel on a primary click.
The Rust shell does not have that panel yet, so its modules only run an
explicit `#:on-click`; without one they are readouts.

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
failed load logs the error and keeps the previous bars.  Reload currently
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

Run from `rust/`, or use `--manifest-path rust/Cargo.toml` from the repository
root.

```sh
cargo test
cargo run -- --version
cargo run -- --check-config
cargo run -- --config /path/to/settings.scm
cargo run -- --check-lock
cargo run -- --lock
```

`--check-lock` is the lock screen's diagnosis without a password: the library
candidates, the one that loads, the service stack that would answer, and
whether this machine has a setuid helper for the store-built case.  `--lock`
shows the Rust lock screen itself; like the C one, only a successful
authentication for the session user removes it.

`--check-config` loads the real settings file through Guile, prints the
wallpaper policy, walks the collection and resolves the current file. A missing
file is the C shell's "no configuration" case and uses defaults; a file that
Guile rejects exits non-zero with the reason.

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
| `tests/wallpaper_view.rs` | Transition progress clamping and the Ken Burns zoom curve |
| `tests/wallpaper_surface.rs` | Animated-frame delay policy, including the 100 ms zero-delay fallback |
| `tests/bar.rs` | C bar defaults, edge/layer/monitor rules, clamps, module lists and typed options |
| `tests/barmonitors.rs` | Monitor-rule selection, output-count conditions and flattened bar placements |
| `tests/config.rs` | Missing-file defaults, the configuration label, and reloading a real two-bar config through one Guile handle |
| `tests/style.rs` | CSS ids, colour alpha handling, material/effects/shadows and generated accent rules |
| `tests/chord.rs` | Both notations, named keys and refusals, GTK accelerators, formatting and display, sequences, and the typed commands |
| `tests/launcher.rs` | Search scoring and ordering, calculator evaluation/refusals/formatting, and the card's scroll, hit-test and bounds |
| `tests/guile.rs` | Real `(whelk core)`/`(whelk state)` modules, a real bar and wallpaper, repeated loads, caught exceptions |
| `tests/lockauth.rs` | Candidate order and dedupe, the private PAM service's modes and reuse, refused unsafe runtime directories, the setuid helper's ownership, and that a host library is never reported as missing host policy |
| `tests/cli.rs` | Version, check-config, missing config and rejected config without starting GTK |
| `tests/barmodule.rs` | Module-host defaults, one-request guard, stale tokens, shared actions and the factory table |
| `tests/barmodules.rs` | Clock calendar, window/custom truncation, audio parsing, CPU/memory/backlight/battery/Caps Lock fixtures, niri layout parsing, Wi-Fi, Bluetooth and power-profile readings |
| unit tests | Scheme literal escaping, `--config` parsing |

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

`(whelk-rust-version)` is the one callback from Scheme into Rust. It allocates a
Scheme string from a compile-time constant, so it cannot panic across the C ABI.

## Wallpaper rendering notes

The Rust surface is deliberately a first slice, not a feature-for-feature copy
of the C engine:

- It creates one background surface per monitor, all showing the same current
  item. Per-monitor media cleanup and the C picker are not ported yet.
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
`memory`, `power-profile`, `keyboard`, `language` and `inhibit` modules are now
ported and wired into the Rust bar, and so is the lock screen: its policy, PAM
boundary, surfaces and logind triggers are Rust.  The idle manager's `IdleHint`
watch, inhibitor, timer and DND flag are Rust too; the locker's per-monitor
rebuild while the protocol lock is up is still in C, as are the remaining module
kinds (clipboard, tray, menu/start, notifications and the rest), the network
panel, launcher, notifications, night-light policies and session management.
The next slices should move those surface-owning modules onto the Rust host,
then make the Rust binary the installed `whelk` once the C and Rust suites both
cover the same behavior.

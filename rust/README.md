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
- `src/session.rs` — session detection, naming and probes: niri, sway,
  Hyprland and river socket variables first, X11 from `XDG_SESSION_TYPE` or a
  `DISPLAY` with no `WAYLAND_DISPLAY`, then a bounded probe of each compositor
  when the environment names none. Detection is cached once and logged with
  the evidence that decided it.
- `src/workspaces.rs` — the two workspace answers through one shape: niri's
  `idx`/`is_active`/`active_window_id` and Hyprland's `id`/`windows` plus the
  active workspace on its monitors, with special workspaces left out and the
  focus commands for both.
- `src/command.rs` — the asynchronous program runner: shell command lines,
  optional stdin for secrets that must not appear in `ps`, exit status, one
  byte-exact reader for NUL-separated menu output, the explicit `WHELK_RETV`
  prefix, and fire-and-forget spawns.
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
- `src/ui.rs` — a GTK4 top bar whose edge, layer, margins, thickness, clock
  format and generated skin come from the Scheme bar. It still renders the
  bootstrap labels; its configured module list is not instantiated yet. It uses
  `gtk4-layer-shell` when the compositor supports it and falls back to a normal
  window on Broadway/X11 test displays.
- `src/main.rs` — `--version`, `--check-config`, `--config PATH`, `--help`.

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
| `tests/style.rs` | CSS ids, colour alpha handling, material/effects/shadows and generated accent rules |
| `tests/chord.rs` | Both notations, named keys and refusals, GTK accelerators, formatting and display, sequences, and the typed commands |
| `tests/launcher.rs` | Search scoring and ordering, calculator evaluation/refusals/formatting, and the card's scroll, hit-test and bounds |
| `tests/guile.rs` | Real `(whelk core)`/`(whelk state)` modules, a real bar and wallpaper, repeated loads, caught exceptions |
| `tests/lockauth.rs` | Candidate order and dedupe, the private PAM service's modes and reuse, refused unsafe runtime directories, the setuid helper's ownership, and that a host library is never reported as missing host policy |
| `tests/cli.rs` | Version, check-config, missing config and rejected config without starting GTK |
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

This is still a bootstrap, not a replacement of the whole shell. The bar model
and its generated skin are now ported, and so is the lock screen: its policy,
PAM boundary, surfaces and logind triggers are Rust.  The idle manager's
`IdleHint` watch, inhibitor, timer and DND flag are Rust too; the locker's
per-monitor rebuild while the protocol lock is up is still in C, as are the
module factories, launcher, notifications, panels, night-light policies and
session management. The next slices should move the module protocol and one
real module family behind the Rust process, then make the Rust binary the
installed `whelk` once the C and Rust suites both cover the same behavior.

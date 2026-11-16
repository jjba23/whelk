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
- `src/guile.rs` and `src/guile/sys.rs` — a small libguile bridge. It initializes
  Guile, prepends the checkout's `scheme/` directory to `%load-path`, registers
  `(whelk-rust-version)` as a Rust primitive, loads the real
  `(whelk core)`/`(whelk state)` modules and reads the wallpaper alist.
- `src/ui.rs` — a GTK4 top bar with the wallpaper summary and a clock. It uses
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
```

`--check-config` loads the real settings file through Guile and prints the
wallpaper policy. A missing file is the C shell's "no configuration" case and
uses defaults; a file that Guile rejects exits non-zero with the reason.

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
| `tests/guile.rs` | Real `(whelk core)`/`(whelk state)` modules, a real settings file, repeated loads, caught exceptions |
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

## Scope and next steps

This is a bootstrap, not a replacement of the bar yet. The C surface, module
system, launcher, notifications and session management remain in place. The next
slices should move the bar model and module protocol behind the Rust process,
then make the Rust binary the installed `whelk` once the C and Rust suites both
cover the same behavior.

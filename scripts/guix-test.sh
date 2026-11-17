#!/bin/sh
# whelk -- run the Cargo gate inside the pinned Guix shell.
#
# Cargo build scripts are dynamically linked.  Guix puts libgcc_s in the
# profile's library directory but not in the loader's path, so a fresh build
# fails to start its own build script.  Bake that directory into the binaries
# Cargo links instead of changing LD_LIBRARY_PATH for Cargo itself.
set -eu

if [ -n "${LIBRARY_PATH:-}" ]; then
  RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-C link-arg=-Wl,-rpath,${LIBRARY_PATH}"
  export RUSTFLAGS
fi

# lambdock's gate throws the target directory away first: a Byggsteg job clone
# survives between jobs, and artifacts from another toolchain can otherwise
# outlive the sources they were built from.  Rebuilding also means the passing
# tests are the ones that match this commit.
cargo clean

# Cargo checks `$CARGO_HOME/bin` before PATH, and rustup keeps the host's
# `cargo-clippy` there.  A private home leaves the shell's matching clippy to
# be found.
export CARGO_HOME="$PWD/target/cargo-home"

if [ "${1:-test}" = clippy ]; then
  cargo clippy --workspace --all-targets --locked -- -D warnings
  exit 0
fi

cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

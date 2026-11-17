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

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

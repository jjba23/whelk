#!/usr/bin/env bash
# whelk -- build and test with AddressSanitizer and UndefinedBehaviorSanitizer.
#
# This is a developer check, not an install: the sanitizer runtimes stay in the
# build directory and the installed binary is never replaced.
set -euo pipefail
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/.."
root=$PWD
build=${WHELK_SANITIZE_BUILD:-build-sanitize}

ASAN_OPTIONS=${ASAN_OPTIONS:-detect_leaks=0}
export ASAN_OPTIONS

if [[ -d "$build" ]]; then
  meson setup --wipe "$build" \
    -Db_sanitize=address,undefined \
    -Db_lto=false \
    -Dbuildtype=debugoptimized \
    -Dstrip=false
else
  meson setup "$build" \
    -Db_sanitize=address,undefined \
    -Db_lto=false \
    -Dbuildtype=debugoptimized \
    -Dstrip=false
fi
ninja -C "$build"
meson test -C "$build" --print-errorlogs

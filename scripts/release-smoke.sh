#!/usr/bin/env bash
# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>
#
# Build, install and identify a release without relying on the source build
# directory.  Pass --source to smoke the current checkout; pass a tarball to
# smoke exactly the artifact that was published.
set -euo pipefail
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/.."
root=$PWD
input=${1:---source}
version=${2:-}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/whelk-smoke.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM

if [[ "$input" == "--source" ]]; then
  source_dir=$root
  if [[ -z "$version" ]]; then
    version=$(sed -n "s/^  version: '\([^']*\)',/\1/p" \
      "$source_dir/meson.build" | head -n 1)
  fi
else
  [[ -f "$input" ]] || {
    printf 'release-smoke: no such tarball: %s\n' "$input" >&2
    exit 2
  }
  mkdir "$tmp/source"
  tar -xzf "$input" -C "$tmp/source"
  source_dir=$(find "$tmp/source" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  if [[ -z "$version" ]]; then
    version=$(sed -n "s/^  version: '\([^']*\)',/\1/p" \
      "$source_dir/meson.build" | head -n 1)
  fi
fi

[[ -n "$version" ]] || {
  printf 'release-smoke: could not determine the version\n' >&2
  exit 2
}

meson setup "$tmp/build" "$source_dir" --prefix=/usr
ninja -C "$tmp/build"
meson install -C "$tmp/build" --destdir "$tmp/dest"

export HOME="$tmp/home"
export XDG_RUNTIME_DIR="$tmp/runtime"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"

binary="$tmp/dest/usr/bin/whelk"
[[ -x "$binary" ]] || {
  printf 'release-smoke: installed binary is missing: %s\n' "$binary" >&2
  exit 1
}

actual=$(timeout 10 "$binary" --version 2>&1)
expected="whelk $version"
[[ "$actual" == "$expected" ]] || {
  printf 'release-smoke: expected %q, got %q\n' "$expected" "$actual" >&2
  exit 1
}

printf 'release-smoke: %s\n' "$actual"

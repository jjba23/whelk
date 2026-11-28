#!/usr/bin/env bash
# bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
#
# Install the part a host shell loads at run time: the (bibelot theme) module
# and its documentation.  The Rust library is built by the host from source.
set -euo pipefail
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/.."
source_root=$PWD

usage() {
  cat <<'EOF'
Usage: scripts/install.sh [--prefix DIR] [--destdir DIR] [--no-build]

  --prefix DIR   install prefix (default: /usr/local)
  --destdir DIR  staging root, as used by package recipes (or $DESTDIR)
  --no-build     accepted for symmetry with a package build; nothing is built
EOF
}

prefix=/usr/local
destdir=${DESTDIR:-}
build=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      prefix=$2
      shift 2
      ;;
    --prefix=*)
      prefix=${1#--prefix=}
      shift
      ;;
    --destdir)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      destdir=$2
      shift 2
      ;;
    --destdir=*)
      destdir=${1#--destdir=}
      shift
      ;;
    --no-build)
      build=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'install: unknown option %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ $build -eq 1 ]]; then
  cargo build --locked --release --lib
fi

if [[ -n "$destdir" ]]; then
  root=$destdir${prefix}
else
  root=$prefix
fi

install -d "$root/share/guile/site/3.0"
cp -R "$source_root/scheme/bibelot" "$root/share/guile/site/3.0/bibelot"
install -Dm644 "$source_root/README.org" \
  "$root/share/doc/bibelot/README.org"
install -Dm644 "$source_root/docs/widgets.org" \
  "$root/share/doc/bibelot/widgets.org"
install -Dm644 "$source_root/LICENSE" \
  "$root/share/licenses/bibelot/LICENSE"

printf 'installed (bibelot theme) to %s\n' \
  "$root/share/guile/site/3.0/bibelot"

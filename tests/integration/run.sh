#!/usr/bin/env bash
# whelk -- run the headless locker integration test without touching the
# caller's session bus or /etc/pam.d.  The private mount namespace exists only
# because PAM reads its service files from a fixed system directory.
set -euo pipefail
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
root=$PWD
command -v unshare >/dev/null 2>&1 || {
  printf 'run-locker: unshare is required\n' >&2
  exit 77
}
command -v dbus-run-session >/dev/null 2>&1 || {
  printf 'run-locker: dbus-run-session is required\n' >&2
  exit 77
}

pamdir=$(mktemp -d "${TMPDIR:-/tmp}/whelk-pam.XXXXXX")
runtime=$(mktemp -d "${TMPDIR:-/tmp}/whelk-runtime.XXXXXX")
trap 'rm -rf "$pamdir" "$runtime" 2>/dev/null || true' EXIT INT TERM
export XDG_RUNTIME_DIR="$runtime"
export WHELK_TEST_RUNTIME_DIR="$runtime"
export ATSPI_DBUS_IMPLEMENTATION=dbus-daemon

if [[ ${WHELK_TEST_PRIVATE_BUS:-1} == 1 ]]; then
  exec unshare -r -m sh -c '
    mount --bind "$1" /etc/pam.d
    exec dbus-run-session -- "$2"
  ' sh "$pamdir" "$root/tests/integration/locker.sh"
fi

exec unshare -r -m sh -c '
  mount --bind "$1" /etc/pam.d
  exec "$2"
' sh "$pamdir" "$root/tests/integration/locker.sh"

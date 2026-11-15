#!/bin/sh
# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>

# The night-light keybinding command, end to end: a private Xvfb and session
# bus, a real whelk, a second whelk forwarding --night-light to it, and the
# saved override checked between the two toggles.  The unit tests cover the
# Scheme and bridge halves; this is the part that proves the option is wired
# into the GApplication command line and reaches the running shell.

set -eu

binary=$1

if [ "${WHELK_CLI_INNER:-0}" != 1 ]; then
  command -v xvfb-run >/dev/null 2>&1 || {
    echo "SKIP: xvfb-run is not installed" >&2
    exit 77
  }
  command -v dbus-run-session >/dev/null 2>&1 || {
    echo "SKIP: dbus-run-session is not installed" >&2
    exit 77
  }
  command -v dbus-send >/dev/null 2>&1 || {
    echo "SKIP: dbus-send is not installed" >&2
    exit 77
  }

  export WHELK_CLI_INNER=1
  exec xvfb-run -a dbus-run-session -- "$0" "$binary"
fi

root=$(mktemp -d "${TMPDIR:-/tmp}/whelk-cli-nightlight.XXXXXX")
pid=

cleanup() {
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$root"
}
trap cleanup EXIT INT TERM

mkdir -p "$root/config/whelk" "$root/state" "$root/runtime" "$root/home"
chmod 700 "$root/runtime"

cat >"$root/config/whelk/settings.scm" <<'EOF'
(configure-shell!
 #:bars
 (list (bar #:name 'cli-test #:position 'top #:height 28
            #:right (list (clock-module #:format "%H:%M"))))
 #:night-light (night-light #:enabled? #f
                           #:temperature 3200
                           #:start "20:30"
                           #:end "06:15"
                           #:transition 45))
EOF

export XDG_CONFIG_HOME="$root/config"
export XDG_STATE_HOME="$root/state"
export XDG_RUNTIME_DIR="$root/runtime"
export HOME="$root/home"
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export GTK_A11Y=test
export WHELK_NO_SUPERVISE=1
export WHELK_TEST_APP_NAME=whelk-cli-nightlight-test
unset WAYLAND_DISPLAY 2>/dev/null || true

"$binary" >"$root/shell.log" 2>&1 &
pid=$!

ready=0
i=0
while [ "$i" -lt 100 ]; do
  if dbus-send --session --dest=org.jointhefreeworld.whelk \
      --type=method_call --print-reply \
      /org/jointhefreeworld/whelk org.freedesktop.DBus.Peer.Ping \
      >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  i=$((i + 1))
  sleep 0.1
done

if [ "$ready" != 1 ]; then
  echo "the test shell did not register on the private bus" >&2
  tail -40 "$root/shell.log" >&2 || true
  exit 1
fi

prefs="$root/state/whelk/preferences-settings.scm"

"$binary" --night-light >"$root/on.out" 2>&1
grep -q "night light: on" "$root/on.out" || {
  echo "the first toggle did not report on" >&2
  cat "$root/on.out" >&2
  exit 1
}
grep -Fq '(night-light-enabled? . #t)' "$prefs" || {
  echo "the first toggle did not save an enabled override" >&2
  cat "$prefs" >&2
  exit 1
}

"$binary" --night-light >"$root/off.out" 2>&1
grep -q "night light: off" "$root/off.out" || {
  echo "the second toggle did not report off" >&2
  cat "$root/off.out" >&2
  exit 1
}
grep -Fq '(night-light-enabled? . #f)' "$prefs" || {
  echo "the second toggle did not save a disabled override" >&2
  cat "$prefs" >&2
  exit 1
}

grep -q "using randr" "$root/shell.log" || {
  echo "the running shell did not use the X11 night-light backend" >&2
  tail -40 "$root/shell.log" >&2
  exit 1
}
grep -q "3200 K, 20:30 to 06:15" "$root/shell.log" || {
  echo "the running shell did not apply the configured night light" >&2
  tail -40 "$root/shell.log" >&2
  exit 1
}

echo "CLI toggle: off -> on -> off; override saved; X11 backend applied 3200 K"

"$binary" --quit >/dev/null 2>&1 || true
i=0
while [ "$i" -lt 50 ] && kill -0 "$pid" 2>/dev/null; do
  i=$((i + 1))
  sleep 0.1
done
wait "$pid" 2>/dev/null || true
pid=

#!/usr/bin/env bash
# whelk -- headless screen-locker integration tests.
#
# Requires sway, wtype, Python 3 with Atspi, and a test PAM service installed at
# /etc/pam.d/whelk-lock.  Run as root to let the script install a temporary
# service, or prepare it with WHELK_TEST_PAM_READY=1.  This script never
# touches a live desktop: it creates its own headless Sway session.
set -euo pipefail
[[ -n ${WHELK_TEST_TRACE:-} ]] && set -x
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
root=$PWD
build=${WHELK_BUILD_DIR:-"$root/build-host"}
helper="$root/tests/integration/atspi-locker.py"
fake_logind="$root/tests/integration/fake-logind.py"
test_password=whelk-test-password

tmp=$(mktemp -d "${TMPDIR:-/tmp}/whelk-locker-test.XXXXXX")
runtime=${WHELK_TEST_RUNTIME_DIR:-$tmp/runtime}
mkdir -p "$runtime"
chmod 700 "$runtime"
sway_pid=
whelk_pid=
fake_bus_pid=
fake_logind_pid=
session_bus_pid=
pam_helper=
temp_pam_helper=
pam_backup=
installed_pam=0

cleanup() {
  [[ -n "$whelk_pid" ]] && kill -TERM "$whelk_pid" 2>/dev/null || true
  [[ -n "$fake_logind_pid" ]] && kill "$fake_logind_pid" 2>/dev/null || true
  [[ -n "$fake_bus_pid" ]] && kill "$fake_bus_pid" 2>/dev/null || true
  [[ -n "$sway_pid" ]] && kill "$sway_pid" 2>/dev/null || true
  [[ -n "$session_bus_pid" ]] && kill "$session_bus_pid" 2>/dev/null || true

  if [[ "$installed_pam" == 1 ]]; then
    rm -f /etc/pam.d/whelk-lock
    if [[ -n "$pam_backup" ]]; then
      mv "$pam_backup" /etc/pam.d/whelk-lock
    fi
  fi
  [[ -n "$temp_pam_helper" ]] && rm -f "$temp_pam_helper"

  rm -rf "$tmp" || true
  if [[ "$runtime" != "$tmp/runtime" ]]; then
    rm -rf "$runtime" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

die() {
  printf 'locker-integration: %s\n' "$*" >&2
  [[ -f "$tmp/whelk.log" ]] && tail -50 "$tmp/whelk.log" >&2 || true
  [[ -f "$tmp/sway.log" ]] && tail -30 "$tmp/sway.log" >&2 || true
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'locker-integration: missing %s; skipping\n' "$1" >&2
    exit 77
  }
}

need sway
need swaymsg
need python3
need timeout
need dbus-daemon

python3 - <<'PY' || {
import sys
try:
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Atspi
except Exception as error:
    print(error, file=sys.stderr)
    sys.exit(1)
PY
  printf 'locker-integration: Python Atspi is unavailable; skipping\n' >&2
  exit 77
}

[[ -x "$build/whelk" ]] || die "build/whelk is missing; run ninja -C build-host"
[[ -x "$build/lock_probe" ]] || die "build/lock_probe is missing"

# A deterministic auth service keeps the test independent of the user's real
# password.  The installed release still uses pam_unix in production.
pam_file=/etc/pam.d/whelk-lock
temp_pam_helper="$tmp/pam-test-auth"
pam_helper=$temp_pam_helper
install -m 755 "$root/tests/integration/pam-test-auth" "$temp_pam_helper"

if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
  if [[ -e "$pam_file" ]]; then
    if ! grep -q 'whelk-pam-test-auth' "$pam_file"; then
      die "$pam_file exists and is not a test service; refusing to replace it"
    fi
    pam_backup="$tmp/whelk-lock.pam"
    cp -p "$pam_file" "$pam_backup"
  fi
  sed "s|__AUTH_HELPER__|$pam_helper|" \
    "$root/tests/integration/pam-test-service" >"$pam_file"
  installed_pam=1
else
  [[ "$(id -u)" -eq 0 ]] || {
    if [[ ! -r "$pam_file" ]] || ! grep -q 'whelk-pam-test-auth' "$pam_file"; then
      printf '%s\n' \
        'locker-integration: /etc/pam.d/whelk-lock test service is absent.' \
        'Run this script as root, or install the test service as root first.' >&2
      exit 77
    fi
    auth=$(sed -n 's/^auth .*pam_exec.so .* //p' "$pam_file" | head -n 1)
    [[ -x "$auth" ]] || exit 77
    pam_helper=$auth
  }
fi

start_sway() {
  mkdir -p "$tmp/config/whelk"
  cat >"$tmp/config/whelk/settings.scm" <<'EOF'
(configure-shell!
 #:bars (list (bar #:name 'main
                   #:position 'top
                   #:height 24
                   #:left '()
                   #:center '()
                   #:right '()))
 #:widgets '())
EOF
  cat >"$tmp/sway.conf" <<'EOF'
xwayland disable
output HEADLESS-1 resolution 1280x800
EOF
  env -u DISPLAY -u WAYLAND_DISPLAY \
    WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=pixman \
    WLR_LIBINPUT_NO_DEVICES=1 XDG_RUNTIME_DIR="$runtime" \
    SWAYSOCK="$tmp/sway.sock" sway -c "$tmp/sway.conf" \
    >"$tmp/sway.log" 2>&1 &
  sway_pid=$!

  local socket=
  for _ in $(seq 1 100); do
    socket=$(find "$runtime" -maxdepth 1 -type s -name 'wayland-*' \
      -print -quit)
    [[ -n "$socket" ]] && break
    kill -0 "$sway_pid" 2>/dev/null || die "sway exited"
    sleep 0.1
  done
  [[ -n "$socket" ]] || die "sway did not create a Wayland socket"
  WAYLAND_DISPLAY=$(basename "$socket")
  export WAYLAND_DISPLAY
  export XDG_RUNTIME_DIR="$runtime"
  printf 'locker-integration: display=%s runtime=%s\n' \
    "$WAYLAND_DISPLAY" "$runtime" >&2

  for _ in $(seq 1 100); do
    SWAYSOCK="$tmp/sway.sock" swaymsg -t get_version >/dev/null 2>&1 && return
    sleep 0.1
  done
  die "sway IPC did not become ready"
}

sway() {
  SWAYSOCK="$tmp/sway.sock" swaymsg "$@"
}

atspi() {
  WHELK_TEST_APP_NAME=whelk-test python3 "$helper" "$@"
}

start_whelk() {
  local mode=$1
  shift
  local system_bus=${DBUS_SYSTEM_BUS_ADDRESS:-}
  : >"$tmp/whelk.log"
  env -u DISPLAY GDK_BACKEND=wayland GSK_RENDERER=cairo \
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
    XDG_RUNTIME_DIR="$runtime" XDG_CONFIG_HOME="$tmp/config" \
    WHELK_NO_SUPERVISE=1 WHELK_TEST_NON_UNIQUE=1 \
    WHELK_TEST_APP_NAME=whelk-test \
    WHELK_LOCK_STATE_FILE="$tmp/failures" \
    DBUS_SYSTEM_BUS_ADDRESS="$system_bus" \
    "$build/whelk" "$@" >"$tmp/whelk.log" 2>&1 &
  whelk_pid=$!
  local expected=locked
  [[ "$mode" == "unlocked" ]] && expected=up
  for _ in $(seq 1 100); do
    if [[ "$expected" == locked ]]; then
      grep -q 'screen locked (ext-session-lock-v1)' "$tmp/whelk.log" && return
    else
      grep -q "Shell.*up" "$tmp/whelk.log" && return
    fi
    kill -0 "$whelk_pid" 2>/dev/null || die "whelk exited"
    sleep 0.1
  done
  die "whelk did not become $mode"
}

stop_whelk() {
  [[ -n "$whelk_pid" ]] || return 0
  kill -TERM "$whelk_pid" 2>/dev/null || true
  wait "$whelk_pid" 2>/dev/null || true
  whelk_pid=
}

submit_password() {
  if [[ ${WHELK_TEST_KEYBOARD:-0} == 1 ]]; then
    command -v wtype >/dev/null 2>&1 ||
      die "WHELK_TEST_KEYBOARD=1 needs wtype on PATH"
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime" \
      wtype -t "$1" || die "wtype could not type the password"
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime" \
      wtype -k Return || die "wtype could not press Return"
    return 0
  fi

  atspi submit "$1" >/dev/null || { atspi state || true; die "could not submit password"; }
}

wait_unlocked() {
  atspi wait-unlocked 30 >/dev/null || die "lock did not release"
}

test_pam_backoff_and_success() {
  start_whelk locked --lock
  atspi wait-lock 30 >/dev/null || die "lock UI did not appear"
  submit_password 'wrong-password'
  for _ in $(seq 1 30); do
    [[ -s "$tmp/failures" ]] && break
    sleep 0.05
  done
  [[ -s "$tmp/failures" ]] || die "failed attempt was not persisted"

  local before after
  before=$(grep -c 'PAM refused authentication' "$tmp/whelk.log" || true)
  atspi submit 'wrong-during-backoff' >/dev/null 2>&1 || true
  sleep 0.3
  after=$(grep -c 'PAM refused authentication' "$tmp/whelk.log" || true)
  [[ "$after" == "$before" ]] || die "backoff allowed a second PAM attempt"

  # The first retry delay is one second; wait for it before the successful try.
  sleep 1.2
  submit_password "$test_password"
  wait_unlocked
  [[ ! -e "$tmp/failures" ]] || die "successful auth did not clear the failure state"
  stop_whelk
}

test_primary_output_replacement() {
  start_whelk locked --lock
  atspi wait-lock 30 >/dev/null || die "lock UI did not appear"
  atspi state | grep -q 'entries=1' || die "primary output has no entry"
  sway output HEADLESS-1 unplug >/dev/null 2>&1 || die "could not unplug primary"
  sway create_output >/dev/null 2>&1 || die "could not create a replacement output"
  atspi wait-entry 30 >/dev/null ||
    die "replacement output has no password entry"
  submit_password "$test_password"
  wait_unlocked
  stop_whelk
}

test_sigterm_keeps_protocol_lock() {
  start_whelk locked --lock
  atspi wait-lock 30 >/dev/null || die "lock UI did not appear"

  if WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime" \
      "$build/lock_probe" >/dev/null 2>&1; then
    die "lock probe should have been refused while locked"
  fi

  kill -TERM "$whelk_pid"
  wait "$whelk_pid" 2>/dev/null || true
  whelk_pid=

  if WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime" \
      "$build/lock_probe" >/dev/null 2>&1; then
    die "SIGTERM released the compositor lock"
  fi
}

start_fake_logind() {
  local bus_file="$tmp/fake-bus.env"
  dbus-daemon --session --fork --print-address=1 --print-pid=1 >"$bus_file"
  local address pid
  address=$(sed -n '1p' "$bus_file")
  pid=$(sed -n '2p' "$bus_file")
  fake_bus_pid=$pid
  export DBUS_SYSTEM_BUS_ADDRESS="$address"
  WHELK_FAKE_LOGIND_TRIGGER="$tmp/fake-logind.trigger" \
    DBUS_SYSTEM_BUS_ADDRESS="$address" python3 "$fake_logind" \
    >"$tmp/fake-logind.log" 2>&1 &
  fake_logind_pid=$!
  for _ in $(seq 1 100); do
    grep -q '^ready ' "$tmp/fake-logind.log" && return
    sleep 0.1
  done
  die "fake logind did not become ready"
}

trigger_fake_logind() {
  printf '%s\n' "$1" >"$tmp/fake-logind.trigger"
}

test_logind_signals() {
  start_fake_logind
  start_whelk unlocked
  atspi wait-unlocked 10 >/dev/null || die "unlocked shell has a lock UI"

  trigger_fake_logind session-lock
  atspi wait-lock 30 >/dev/null || die "Session.Lock did not lock"
  submit_password "$test_password"
  wait_unlocked
  stop_whelk

  start_whelk unlocked
  trigger_fake_logind prepare-sleep
  atspi wait-lock 30 >/dev/null || die "PrepareForSleep did not lock"
  submit_password "$test_password"
  wait_unlocked
  stop_whelk

  start_whelk unlocked
  trigger_fake_logind session-inactive
  atspi wait-lock 30 >/dev/null || die "session Active=false did not lock"
  submit_password "$test_password"
  wait_unlocked
  stop_whelk

  start_whelk unlocked
  trigger_fake_logind idle-hint
  atspi wait-lock 30 >/dev/null || die "logind IdleHint did not lock"
  submit_password "$test_password"
  wait_unlocked
  stop_whelk
}

start_sway
test_pam_backoff_and_success
test_primary_output_replacement
test_logind_signals
# Run this last: it deliberately leaves the compositor locked until cleanup.
test_sigterm_keeps_protocol_lock

printf 'locker-integration: all checks passed\n'

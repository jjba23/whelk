#!/bin/sh
# Render tests/bar_preview under Broadway, a headless GTK display server, and
# diff the pixels against the committed reference.  Broadway is the compositor
# the bar is actually presented on; nothing here is an offscreen mock.
set -eu

build_dir="$1"
source_dir="$2"
output="$3"
reference="$4"
display=":77"

log_file="${TMPDIR:-/tmp}/whelk-golden-broadway.$$.log"

gtk4-broadwayd "$display" >"$log_file" 2>&1 &
daemon=$!
trap 'kill "$daemon" 2>/dev/null || true; wait "$daemon" 2>/dev/null || true' EXIT INT TERM

# Give the daemon its socket.  It is a local process; a short bounded wait is
# enough, and a failure here is reported with the daemon's own log.
tries=0
while ! grep -q "Listening on" "$log_file" 2>/dev/null && [ "$tries" -lt 40 ]; do
  if ! kill -0 "$daemon" 2>/dev/null; then
    echo "golden: broadwayd exited" >&2
    cat "$log_file" >&2
    exit 1
  fi
  sleep 0.05
  tries=$((tries + 1))
done

if ! grep -q "Listening on" "$log_file" 2>/dev/null; then
  echo "golden: broadwayd did not become ready" >&2
  cat "$log_file" >&2
  exit 1
fi

WHELK_GOLDEN_OUTPUT="$output" \
WHELK_GOLDEN_REFERENCE="$reference" \
GDK_BACKEND=broadway \
BROADWAY_DISPLAY="$display" \
GSK_RENDERER=cairo \
timeout 30 "$build_dir/bar_preview"

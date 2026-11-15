#!/bin/sh
# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>

# The X11 night-light test needs a server with RandR gamma tables.  A developer
# on a Wayland session usually has only XWayland, which reports gamma tables but
# is also somebody's live desktop: the test must not overwrite that display's
# calibration.  Xvfb is a real software X server with RandR 1.6 and a 256-entry
# CRTC gamma table, so always use a private one.

set -eu

binary=$1

if ! command -v xvfb-run >/dev/null 2>&1; then
  echo "SKIP: xvfb-run is not installed; not touching the ambient DISPLAY" >&2
  exit 77
fi

exec xvfb-run -a -s "-screen 0 1280x800x24 +extension RANDR" "$binary"

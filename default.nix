# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>

{
  lib,
  stdenv,
  meson,
  ninja,
  pkg-config,
  wrapGAppsHook4,
  glib,
  gtk4,
  gtk4-layer-shell,
  guile,
  libX11,
  hicolor-icon-theme,
  adwaita-icon-theme,
  xvfb-run,
  dbus,
  fontconfig,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "whelk";
  version = "0.4.0";

  strictDeps = true;
  __structuredAttrs = true;

  # `build/' is not source: a tree that has been built here would otherwise be
  # unpacked into the sandbox and Meson would find a configured directory from
  # another Meson version inside it.
  src = lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      let base = baseNameOf (toString path);
      in !(builtins.elem base [ "build" "build-asan" "build-release" "result"
                                ".direnv" ])
         && lib.cleanSourceFilter path type;
  };

  nativeBuildInputs = [
    meson
    ninja
    pkg-config
    glib
    wrapGAppsHook4
    # The theme tests are a Guile script, and Meson only registers them when it
    # finds `guile' at configure time.
    guile
  ];

  buildInputs = [
    gtk4
    gtk4-layer-shell
    guile
    # On an X11 desktop a bar is a dock window with EWMH struts, which needs
    # Xlib at build time; without it the Wayland-only build is what you get.
    libX11
  ];

  doCheck = true;

  nativeCheckInputs = [
    xvfb-run
    dbus
    fontconfig
    hicolor-icon-theme
    adwaita-icon-theme
  ];

  # The suite is small but not headless: the menubar tests build real GTK
  # widgets, and the theme tests run Guile.  Xvfb with the software renderer is
  # enough for both, and a session bus keeps GIO's portals quiet.
  checkPhase = ''
    runHook preCheck

    export HOME=$TMPDIR
    export XDG_RUNTIME_DIR=$TMPDIR
    export FONTCONFIG_FILE=${fontconfig.out}/etc/fonts/fonts.conf
    export XDG_DATA_DIRS=${hicolor-icon-theme}/share:${adwaita-icon-theme}/share
    export GTK_A11Y=test
    export GDK_BACKEND=x11
    export GSK_RENDERER=cairo

    dbus-daemon --config-file=${dbus}/share/dbus-1/session.conf --fork --print-address 5 5>dbus_address
    export DBUS_SESSION_BUS_ADDRESS=$(cat dbus_address)

    xvfb-run -s "-screen 0 1280x800x24" meson test --print-errorlogs

    runHook postCheck
  '';

  meta = with lib; {
    description = "Wayland shell configured with GNU Guile Scheme";
    homepage = "https://github.com/jjba23/whelk";
    license = licenses.mit;
    platforms = platforms.linux;
    mainProgram = "whelk";
  };
})

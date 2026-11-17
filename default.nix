# whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>

{
  lib,
  rustPlatform,
  pkg-config,
  wayland-scanner,
  wayland,
  makeWrapper,
  wrapGAppsHook4,
  glib,
  gtk4,
  gtk4-layer-shell,
  guile,
  pam,
  wtype,
  hicolor-icon-theme,
  adwaita-icon-theme,
  xvfb-run,
  dbus,
  fontconfig,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "whelk";
  version = "0.39.2";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        base = baseNameOf (toString path);
      in
      # Only the workspace's own build directories are dropped.  Vendored
      # crates have fixtures in directories named target, and cargo refuses
      # to build when those are missing.
      !(builtins.elem (toString path) [
        (toString ./target)
        (toString ./rust/target)
      ])
      && !(builtins.elem base [
        "build"
        "build-asan"
        "build-sanitize"
        "build-release"
        "result"
        ".direnv"
        "dist"
      ])
      && lib.cleanSourceFilter path type;
  };

  # The workspace root is the virtual manifest; the shell is the `rust/`
  # member.  The Scheme tests walk back to the checkout, so the whole source
  # stays available.
  buildAndTestSubdir = "rust";
  cargoLock.lockFile = ./Cargo.lock;
  cargoBuildFlags = [
    "--package"
    "whelk-rs"
    "--bin"
    "whelk"
  ];
  cargoInstallFlags = [
    "--locked"
    "--bin"
    "whelk"
  ];

  nativeBuildInputs = [
    pkg-config
    wayland-scanner
    makeWrapper
    wrapGAppsHook4
    glib
    guile
  ];

  buildInputs = [
    gtk4
    gtk4-layer-shell
    guile
    pam
    wayland
    wtype
  ];

  doCheck = true;

  nativeCheckInputs = [
    xvfb-run
    dbus
    fontconfig
    hicolor-icon-theme
    adwaita-icon-theme
    guile
  ];

  dontWrapGApps = true;

  preFixup = ''
    makeWrapperArgs+=("''${gappsWrapperArgs[@]}")
    makeWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [ wtype ]})
  '';

  postFixup = ''
    wrapProgram $out/bin/whelk "''${makeWrapperArgs[@]}"
  '';

  # GTK and the Scheme tests need a private display and data directories, but
  # no compositor: Xvfb with the software renderer is enough.
  checkPhase = ''
    runHook preCheck

    export HOME=$TMPDIR
    export XDG_RUNTIME_DIR=$TMPDIR
    export FONTCONFIG_FILE=${fontconfig.out}/etc/fonts/fonts.conf
    export XDG_DATA_DIRS=${hicolor-icon-theme}/share:${adwaita-icon-theme}/share
    export GTK_A11Y=test
    export GDK_BACKEND=x11
    export GSK_RENDERER=cairo

    xvfb-run -a cargo test --workspace --locked

    runHook postCheck
  '';

  postInstall = ''
    install -Dm644 $src/data/org.jointhefreeworld.whelk.desktop \
      $out/share/applications/org.jointhefreeworld.whelk.desktop
    mkdir -p $out/share/guile/site/3.0
    cp -r $src/scheme/whelk $out/share/guile/site/3.0/whelk
    cp -r $src/data/icons $out/share/icons
    install -Dm644 $src/README.org $out/share/doc/whelk/README.org
    install -Dm644 $src/LICENSE $out/share/licenses/whelk/LICENSE
  '';

  meta = with lib; {
    description = "Wayland shell with bars, a launcher and notifications, configured in Guile Scheme";
    homepage = "https://github.com/jjba23/whelk";
    license = licenses.mit;
    platforms = platforms.linux;
    mainProgram = "whelk";
  };
})

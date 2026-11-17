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
  mesa,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "whelk";
  version = "0.42.0";

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
    # The GL renderer's drivers, named by the wrapper below; without them EGL
    # finds no vendor and every surface falls back to cairo.
    mesa
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

  # A nix-built whelk links libEGL from libglvnd, whose compiled-in vendor
  # directory is inside the nix store.  A machine that is not NixOS keeps its
  # GPU drivers somewhere else, so libEGL finds no vendor library, GTK's GL
  # renderer dies with `Failed to create EGL display`, and every surface falls
  # back to cairo -- which is what makes a blurred, frosted bar feel slow.
  #
  # `__EGL_VENDOR_LIBRARY_FILENAMES` is libglvnd's own override, so the nix Mesa
  # vendor is named explicitly; its DRI driver (radeonsi, iris, llvmpipe ...)
  # talks to the kernel's render node directly, so no library from the host is
  # loaded into this process and the glibc it was built against does not
  # matter.  GLX (an X11 session) gets the same treatment through
  # `__GLX_VENDOR_LIBRARY_NAME` and `LIBGL_DRIVERS_PATH`.  These are
  # `--set-default`s: an environment that already names its vendors wins, so a
  # host GL setup such as nixGL, or a user who wants the proprietary NVIDIA
  # path, still decides.  `EGL_PLATFORM` stays GTK's business -- GTK asks for
  # the platform its GDK backend is on (Wayland, X11, headless), and forcing it
  # from a wrapper would break the others.
  preFixup = ''
    makeWrapperArgs+=("''${gappsWrapperArgs[@]}")
    makeWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [ wtype ]})
    makeWrapperArgs+=(
      --set-default __EGL_VENDOR_LIBRARY_FILENAMES ${mesa}/share/glvnd/egl_vendor.d/50_mesa.json
      --set-default __GLX_VENDOR_LIBRARY_NAME mesa
      --set-default LIBGL_DRIVERS_PATH ${mesa}/lib/dri
    )
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

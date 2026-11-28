# bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT

{
  lib,
  rustPlatform,
  pkg-config,
  gtk4,
  gtk4-layer-shell,
  guile,
  wayland,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "bibelot";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        base = baseNameOf (toString path);
      in
      !(builtins.elem (toString path) [ (toString ./target) ])
      && !(builtins.elem base [ "result" "dist" ".direnv" ])
      && lib.cleanSourceFilter path type;
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [
    gtk4
    gtk4-layer-shell
    guile
    wayland
  ];

  doCheck = true;

  # bibelot is a library and a Scheme module, not a program.  The card tests do
  # not map a window, so they need no compositor; Guile is a link dependency.
  checkPhase = ''
    runHook preCheck

    export HOME=$TMPDIR
    export XDG_RUNTIME_DIR=$TMPDIR

    cargo test --locked

    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall

    install -d $out/share/guile/site/3.0
    cp -R scheme/bibelot $out/share/guile/site/3.0/bibelot
    install -Dm644 README.org $out/share/doc/bibelot/README.org
    install -Dm644 docs/widgets.org $out/share/doc/bibelot/widgets.org
    install -Dm644 LICENSE $out/share/licenses/bibelot/LICENSE

    runHook postInstall
  '';

  meta = with lib; {
    description = "Floating desktop widget cards in Rust, GTK4 and Guile Scheme";
    homepage = "https://github.com/jjba23/bibelot";
    license = licenses.mit;
    platforms = platforms.linux;
  };
})

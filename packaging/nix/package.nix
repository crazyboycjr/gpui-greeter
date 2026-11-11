# gpui-greeter built from the working tree this file sits in, as the
# flake's package and its overlay give it. nixpkgs' own package.nix
# builds a tagged release with the same inputs and install steps; keep the
# two alike.
{
  lib,
  rustPlatform,
  pkg-config,
  fontconfig,
  freetype,
  libxcb,
  libxkbcommon,
  pam,
  tzdata,
  vulkan-loader,
  versionCheckHook,
}:

let
  root = ../..;
in
rustPlatform.buildRustPackage {
  pname = "gpui-greeter";
  version = (lib.importTOML (root + "/Cargo.toml")).package.version;

  # What the build, the tests and the install read, so that a change to
  # the docs or the packaging rebuilds nothing.
  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (root + "/README.md")
      (root + "/assets")
      (root + "/dist")
      (root + "/examples")
      (root + "/skills")
      (root + "/src")
      (root + "/tests")
      (root + "/vendor")
    ];
  };

  cargoLock.lockFile = root + "/Cargo.lock";

  nativeBuildInputs = [ pkg-config ];

  # fontconfig and freetype for build scripts of gpui's font loading, which
  # the greeter does not use at run time.
  buildInputs = [
    fontconfig
    freetype
    libxcb
    libxkbcommon
    pam
  ];

  # The library's tests; tests/ drives apply-settings --root, which only a
  # debug build takes. Their dates are in named time zones.
  cargoTestFlags = [ "--lib" ];
  preCheck = ''
    export TZDIR=${tzdata}/share/zoneinfo
  '';

  postInstall = ''
    install -Dm644 -t $out/lib/systemd/system \
      dist/gpui-greeter-bing.service dist/gpui-greeter-bing.timer
    substituteInPlace $out/lib/systemd/system/gpui-greeter-bing.service \
      --replace-fail /usr/bin/gpui-greeter $out/bin/gpui-greeter \
      --replace-fail /usr/share/doc $out/share/doc
    install -Dm644 dist/tmpfiles.conf $out/lib/tmpfiles.d/gpui-greeter.conf
    install -Dm644 -t $out/share/applications dist/gpui-greeter-settings.desktop
    # The icon the desktop file names, in every size it is drawn at.
    mkdir -p $out/share/icons
    cp -r assets/icon/linux/hicolor $out/share/icons/
    # Every setting with its default, for reference: the NixOS module
    # writes /etc/gpui-greeter/config.toml from its own settings.
    install -Dm644 dist/config.toml $out/share/gpui-greeter/config.toml
    install -Dm644 -t $out/share/doc/gpui-greeter README.md assets/fonts/OFL-*.txt
    install -Dm644 skills/gpui-greeter-theme/dial/fonts/OFL.txt \
      $out/share/doc/gpui-greeter/OFL-NotoSerifDisplay.txt
    # The skill for designing themes with an agent, and its example theme.
    find skills -type f -exec install -Dm644 {} $out/share/gpui-greeter/{} \;
  '';

  # wgpu loads Vulkan with dlopen, so the loader goes on the RUNPATH after
  # the fixup has shrunk it to the linked libraries.
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath [ vulkan-loader ]} $out/bin/gpui-greeter
  '';

  nativeInstallCheckInputs = [ versionCheckHook ];
  doInstallCheck = true;

  meta = {
    description = "Greeter for greetd on X11, drawn with GPUI, with a lock screen";
    homepage = "https://github.com/crazyboycjr/gpui-greeter";
    # gpui-greeter's code is under Apache-2.0; the bundled fonts are under
    # the SIL Open Font License.
    license = with lib.licenses; [
      asl20
      ofl
    ];
    mainProgram = "gpui-greeter";
    platforms = lib.platforms.linux;
  };
}

{
  description = "gpui-greeter, a greeter for greetd on X11, drawn with GPUI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      forAllSystems = f: lib.genAttrs [ "x86_64-linux" ] (system: f nixpkgs.legacyPackages.${system});
      packageOf = pkgs: self.packages.${pkgs.stdenv.hostPlatform.system}.gpui-greeter;
    in
    {
      packages = forAllSystems (pkgs: {
        default = packageOf pkgs;
        gpui-greeter = pkgs.callPackage ./packaging/nix/package.nix { };
      });

      # nix run github:crazyboycjr/gpui-greeter -- preview DIR, -- lock or
      # -- settings; the greeter itself needs greetd.
      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = lib.getExe (packageOf pkgs);
          meta.description = (packageOf pkgs).meta.description;
        };
      });

      overlays.default = final: prev: {
        gpui-greeter = final.callPackage ./packaging/nix/package.nix { };
      };

      # services.gpui-greeter, with this flake's package.
      nixosModules.default =
        { pkgs, ... }:
        {
          imports = [ ./packaging/nix/module.nix ];
          services.gpui-greeter.package = lib.mkDefault (packageOf pkgs);
        };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ (packageOf pkgs) ];
          # The tools scripts/try drives an invisible X server with.
          packages = with pkgs; [
            clippy
            rust-analyzer
            rustfmt
            imagemagick
            xdotool
            xrandr
            xvfb
          ];
          # What the package puts on the binary's RUNPATH, for `cargo run`.
          env = {
            LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.vulkan-loader ];
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        };
      });

      checks = forAllSystems (pkgs: {
        package = packageOf pkgs;
        # A NixOS machine that logs in and locks through gpui-greeter.
        nixos = import ./packaging/nix/test.nix {
          inherit pkgs;
          module = self.nixosModules.default;
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}

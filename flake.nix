{
  description = "COSMIC panel applet for ICE train speed and trip info";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = self.packages.${system}.cosmic-applet-db-ice;

          cosmic-applet-db-ice = pkgs.rustPlatform.buildRustPackage {
            pname = "cosmic-applet-db-ice";
            version = "0.1.0";
            src = ./.;

            nativeBuildInputs = with pkgs; [
              pkg-config
            ];

            buildInputs = with pkgs; [
              dbus
              wayland
              libxkbcommon
              vulkan-loader
              mesa
            ];

            postInstall = ''
              install -Dm644 data/dev.smephite.CosmicAppletDbIce.desktop \
                $out/share/applications/dev.smephite.CosmicAppletDbIce.desktop
            '';

            meta = with pkgs.lib; {
              description = "COSMIC panel applet showing ICE train speed and trip info";
              license = licenses.mit;
              platforms = platforms.linux;
              mainProgram = "cosmic-applet-db-ice";
            };
          };
        }
      );

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.cosmic-applet-db-ice ];

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.vulkan-loader
              pkgs.mesa
            ];
          };
        }
      );
    };
}

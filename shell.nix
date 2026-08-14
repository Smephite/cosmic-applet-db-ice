{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config ];

  buildInputs = with pkgs; [
    dbus
    wayland
    libxkbcommon
    vulkan-loader
    mesa
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    wayland
    libxkbcommon
    vulkan-loader
    mesa
  ]);
}

{
  description = "Settings daemon for the COSMIC desktop environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils }:
    utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };

        runtimeDependencies = with pkgs; [
        ];

      in {
        devShells.default = with pkgs; mkShell rec {
          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            libxkbcommon
            libinput
            libpulseaudio.dev
            pipewire.dev
            systemd
            openssl
          ];

          RUST_SRC_PATH = rustPlatform.rustLibSrc;
          RUSTFLAGS = "-C link-arg=-Wl,-rpath,${pkgs.lib.makeLibraryPath runtimeDependencies}";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };
      }
    );
}

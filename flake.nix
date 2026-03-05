{
  description = "Settings daemon for the COSMIC desktop environment";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  nixConfig.bash-prompt-suffix = "[nix]: "; # shows when inside of nix shell

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      pkgsForSystem =
        system:
        (import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        });

      commonFor =
        system:
        let
          pkgs = pkgsForSystem system;
          rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          buildInputs = with pkgs; [
            rustToolchain
            libxkbcommon
            libinput
            libpulseaudio.dev
            pipewire.dev
            systemd
            openssl
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          runtimeDependencies = with pkgs; [
          ];
        in
        {
          inherit
            pkgs
            rustToolchain
            buildInputs
            nativeBuildInputs
            runtimeDependencies
            ;
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          c = commonFor system;
          rustPlatform = c.pkgs.makeRustPlatform {
            cargo = c.rustToolchain;
            rustc = c.rustToolchain;
          };
        in
        rec {
          default = cosmic-settings-daemon;
          cosmic-settings-daemon = rustPlatform.buildRustPackage {
            name = "cosmic-settings-daemon";
            src = c.pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = c.pkgs.lib.fileset.unions [
                ./config
                ./cosmic-settings-daemon-config
                ./data
                ./geonames
                ./src
                ./Cargo.toml
                ./Cargo.lock
              ];
            };

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            buildInputs = c.buildInputs;
            nativeBuildInputs = c.nativeBuildInputs;
            runtimeDependencies = c.runtimeDependencies;
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          c = commonFor system;
        in
        {
          default = c.pkgs.mkShell {
            buildInputs = c.buildInputs;
            inputsFrom = [ self.packages.${system}.cosmic-settings-daemon ];

            LD_LIBRARY_PATH = c.pkgs.lib.makeLibraryPath c.buildInputs;
            RUSTFLAGS = "-C link-arg=-Wl,-rpath,${c.pkgs.lib.makeLibraryPath c.runtimeDependencies}";
          };
        }
      );
    };
}

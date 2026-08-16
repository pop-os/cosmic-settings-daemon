{
  description = "Settings daemon for the COSMIC desktop environment";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

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
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

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
            udev
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
        {
          default = self.packages.${system}.cosmic-settings-daemon;
          cosmic-settings-daemon = rustPlatform.buildRustPackage {
            pname = "cosmic-settings-daemon";
            version = "1.0.8-dev";

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
                ./Makefile
              ];
            };

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            separateDebugInfo = true;

            buildInputs = c.buildInputs;
            nativeBuildInputs = c.nativeBuildInputs;
            runtimeDependencies = c.runtimeDependencies;

            makeFlags = [
              "prefix=${placeholder "out"}"
              "CARGO_TARGET_DIR=target/${c.pkgs.stdenv.hostPlatform.rust.cargoShortTarget}"
            ];

            dontCargoInstall = true;

            meta = with c.pkgs.lib; {
              description = "Settings daemon for the COSMIC desktop environment";
              homepage = "https://github.com/pop-os/cosmic-settings-daemon";
              license = licenses.gpl3Only;
              platforms = platforms.linux;
              mainProgram = "cosmic-settings-daemon";
            };

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
            inputsFrom = [ self.packages.${system}.cosmic-settings-daemon ];

            packages = with c.pkgs; [
              c.rustToolchain
              rust-analyzer
            ];

            LD_LIBRARY_PATH = c.pkgs.lib.makeLibraryPath c.runtimeDependencies;
            RUSTFLAGS = "-C link-arg=-Wl,-rpath,${c.pkgs.lib.makeLibraryPath c.runtimeDependencies}";

            shellHook = ''
              echo "COSMIC Settings Daemon development environment"
              echo "Run 'cargo build' to build, 'cargo test' to test"
            '';
          };
        }
      );
    };
}

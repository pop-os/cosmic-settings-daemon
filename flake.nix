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
            libinput
            libpulseaudio
            openssl
            udev
            pipewire
            libxkbcommon
            wayland
          ];

          nativeBuildInputs = with pkgs; [
            makeBinaryWrapper
            pkg-config
            rustPlatform.bindgenHook
            just
          ];

          runtimeDependencies = with pkgs; [
            wayland
            libxkbcommon
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
          common = commonFor system;
          rustPlatform = common.pkgs.makeRustPlatform {
            cargo = common.rustToolchain;
            rustc = common.rustToolchain;
          };
        in
        {
          default = self.packages.${system}.cosmic-settings-daemon;
          cosmic-settings-daemon = rustPlatform.buildRustPackage {
            pname = "cosmic-settings-daemon";
            version = "1.m.M-dev";

            src = common.pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = common.pkgs.lib.fileset.unions [
                ./audio-client
                ./audio-core
                ./audio-server
                ./config
                ./cosmic-pipewire
                ./cosmic-settings-daemon-config
                ./data
                ./geonames
                ./src
                ./varlink-server
                ./Cargo.toml
                ./Cargo.lock
                ./cargo.just
                ./justfile
              ];
            };

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            dontCargoInstall = true;

            # default recipe missing --target and --profile release already used.
            dontUseJustBuild = true;
            dontUseJustCheck = true; # no just test recipe.

            inherit (common) buildInputs nativeBuildInputs;

            separateDebugInfo = true;
            __structuredAttrs = true;

            justFlags = [
              "--set"
              "prefix"
              (placeholder "out")
              "--set"
              "cargo-target-dir"
              "target/${common.pkgs.stdenv.hostPlatform.rust.cargoShortTarget}"
            ];

            postFixup = ''
              wrapProgram $out/bin/cosmic-settings-daemon \
                --prefix LD_LIBRARY_PATH : ${
                  common.pkgs.lib.makeLibraryPath [
                    common.pkgs.wayland
                    common.pkgs.libxkbcommon
                  ]
                }
            '';

            meta = with common.pkgs.lib; {
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
          common = commonFor system;
        in
        {
          default = common.pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.cosmic-settings-daemon ];

            packages = with common.pkgs; [
              common.rustToolchain
              rust-analyzer
              just
            ];

            LD_LIBRARY_PATH = common.pkgs.lib.makeLibraryPath common.runtimeDependencies;
            RUSTFLAGS = "-C link-arg=-Wl,-rpath,${common.pkgs.lib.makeLibraryPath common.runtimeDependencies}";

            shellHook = ''
              echo "COSMIC Settings Daemon development environment"
              echo "Run 'cargo build' to build, 'cargo test' to test"
            '';
          };
        }
      );
    };
}

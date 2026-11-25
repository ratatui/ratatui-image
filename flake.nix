{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    rust-overlay,
    ...
  }:
    # Not `eachDefaultSystem`: its list includes x86_64-darwin, which nixpkgs 26.11
    # no longer supports (evaluating it throws), so e.g. `nix flake show` fails.
    # Re-add it if we pin a nixpkgs that still supports Intel Macs (<= 26.05).
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ] (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };
      inherit (pkgs) lib;

      # Build chafa with static library support (uses autotools)
      chafaStatic = pkgs.chafa.overrideAttrs (old: {
        configureFlags = (old.configureFlags or []) ++ [
          "--enable-static"
          "--enable-shared"
        ];
      });

      # We also need static glib for full static linking (uses meson)
      glibStatic = pkgs.glib.overrideAttrs (old: {
        mesonFlags = (old.mesonFlags or []) ++ [
          "-Ddefault_library=both"
        ];
      });

      # Chafa build dependencies for chafa-dyn and chafa-static features
      chafaBuildArgs = {
        nativeBuildInputs = with pkgs; [
          pkg-config
          llvmPackages.libclang
        ];
        buildInputs = with pkgs; [
          chafaStatic
          chafaStatic.dev
          glibStatic.dev
          libsysprof-capture
          pcre2.dev
          libffi.dev
          zlib.dev
        ];
        PKG_CONFIG_PATH = "${chafaStatic.dev}/lib/pkgconfig:${glibStatic.dev}/lib/pkgconfig:${pkgs.libsysprof-capture}/lib/pkgconfig:${pkgs.pcre2.dev}/lib/pkgconfig:${pkgs.libffi.dev}/lib/pkgconfig:${pkgs.zlib.dev}/lib/pkgconfig";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";
      };

      # Toolchain pinned in rust-toolchain.toml (shared with rustup and CI).
      rustToolchain = p: p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

      unfilteredRoot = ./.;
      src = lib.fileset.toSource {
        root = unfilteredRoot;
        fileset = lib.fileset.unions [
          # Default files from crane (Rust and cargo files)
          (craneLib.fileset.commonCargoSources unfilteredRoot)
          (lib.fileset.maybeMissing ./assets)
          (lib.fileset.fileFilter (file: file.hasExt "snap") ./src)
        ];
      };

      commonArgs = {
        inherit src;
        strictDeps = true;

        buildInputs = with pkgs;
          [
            cargo-watch
            cargo-semver-checks
            cargo-release
            cargo-make
            rust-analyzer
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

        # Additional environment variables can be set directly
        # MY_CUSTOM_VAR = "some value";
      };

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      ratatui-image = craneLib.buildPackage (commonArgs
        // chafaBuildArgs
        // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--no-default-features --features chafa-static,image-defaults,crossterm";
        });

      ratatui-demo = craneLib.buildPackage (commonArgs
        // chafaBuildArgs
        // {
          inherit cargoArtifacts;
          pname = "demo";
          cargoExtraArgs = "--example demo --no-default-features --features chafa-static,image-defaults,crossterm";
          installPhaseCommand = ''
            mkdir -p $out/bin
            cp target/release/examples/demo $out/bin/
          '';
        });

      screenshotTests = import ./screenshot-tests.nix { inherit pkgs src self system; };

      # Feature matrix for checks
      featureMatrix = {
        default        = { args = ""; extraArgs = chafaBuildArgs; };
        no-chafa       = { args = "--no-default-features --features image-defaults,crossterm"; extraArgs = {}; };
        chafa-static   = { args = "--no-default-features --features chafa-static,image-defaults,crossterm"; extraArgs = chafaBuildArgs; };
        full           = { args = "--features serde,tokio,chafa-dyn"; extraArgs = chafaBuildArgs; };
        thread-example = { args = "--features thread-example"; extraArgs = chafaBuildArgs; };
      };

      mkClippy = name: { args, extraArgs }:
        craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "${args} --all-targets -- --deny warnings";
        } // extraArgs);

      mkNextest = name: { args, extraArgs }:
        craneLib.cargoNextest (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = args;
          partitions = 1;
          partitionType = "count";
        } // extraArgs);

      mkDoc = name: { args, extraArgs }:
        craneLib.cargoDoc (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = args;
        } // extraArgs);

      matrixChecks = lib.foldl' (acc: name: acc // {
        "clippy-${name}"  = mkClippy name featureMatrix.${name};
        "nextest-${name}" = mkNextest name featureMatrix.${name};
        "doc-${name}"     = mkDoc name featureMatrix.${name};
      }) {} (lib.attrNames featureMatrix);

      # Cross-compile to Windows
      pkgsWindows = import nixpkgs {
        overlays = [ (import rust-overlay) ];
        localSystem = system;
        crossSystem = {
          config = "x86_64-w64-mingw32";
          libc = "msvcrt";
        };
      };
      craneLibWindows = (crane.mkLib pkgsWindows).overrideToolchain (p:
        (rustToolchain p).override {
          targets = [ "x86_64-pc-windows-gnu" ];
        }
      );
      ratatui-image-windows = craneLibWindows.buildPackage {
        src = craneLibWindows.cleanCargoSource ./.;
        strictDeps = true;
        doCheck = false;
        cargoExtraArgs = "--no-default-features --features image-defaults,crossterm";
      };

    in {
      checks = {
        inherit ratatui-image;
        ratatui-image-fmt = craneLib.cargoFmt { inherit src; };
      } // matrixChecks;

      packages =
        {
          default = ratatui-image;
          demo = ratatui-demo;
        }
        # The windows cross build needs wine, which nixpkgs only provides on linux.
        // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          windows = ratatui-image-windows;
        }
        // screenshotTests;

      apps.default = flake-utils.lib.mkApp {
        drv = ratatui-image;
      };

      devShells.default = craneLib.devShell {
        # Inherit inputs from checks.
        checks = self.checks.${system};

        # Additional dev-shell environment variables can be set directly
        # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = with pkgs; [
          cargo-release
          cargo-insta
          cargo-flamegraph
          chafa
          chafaStatic       # for chafa-static feature (has libchafa.a)
          chafaStatic.dev
          glibStatic.dev    # required by chafa.pc (has libglib-2.0.a)
          # Dependencies needed for static linking
          libsysprof-capture
          pcre2.dev
          libffi.dev
          zlib.dev
          pkg-config
          llvmPackages.libclang
        ];
        LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.chafa ];
        # For chafa-static feature, bindgen needs LIBCLANG_PATH and C headers
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.glibc.dev}/include";
        # Ensure chafaStatic is found first for static linking
        PKG_CONFIG_PATH = "${chafaStatic.dev}/lib/pkgconfig:${glibStatic.dev}/lib/pkgconfig";
      };
    });
}

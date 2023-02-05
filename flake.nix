{
  description = "Alpacka: A generational package manager for neovim";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    advisory-db,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      rustToolchain = pkgs.pkgsBuildHost.rust-bin.stable.latest.default.override {
        extensions = ["rust-src" "rust-analyzer"];
      };

      inherit (pkgs) lib;

      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      src = craneLib.cleanCargoSource ./.;

      buildInputs =
        [
          # libgit2 deps
          pkgs.pkg-config
          pkgs.openssl
        ]
        ++ lib.optionals pkgs.stdenv.isDarwin [
          # libgit2 deps
          pkgs.darwin.apple_sdk.frameworks.Security
          # rust dep
          pkgs.libiconv
        ];

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly {
        inherit src buildInputs;
      };

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      alpacka = craneLib.buildPackage {
        inherit cargoArtifacts src buildInputs;
        doCheck = false;
      };
    in {
      checks = {
        # Build the crate as part of `nix flake check` for convenience, with tests
        # sandboxing must be disabled
        # since nextest doesn't support doctests yet, we are not using it.
        alpacka = craneLib.buildPackage {
          inherit cargoArtifacts src buildInputs;
          doCheck = true;
        };

        # Run clippy (and deny all warnings) on the crate source,
        # again, resuing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        alpacka-clippy = craneLib.cargoClippy {
          inherit cargoArtifacts src buildInputs;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        };

        alpacka-doc = craneLib.cargoDoc {
          inherit cargoArtifacts src buildInputs;
        };

        # Check formatting
        alpacka-fmt = craneLib.cargoFmt {
          inherit src;
        };

        # Audit dependencies
        alpacka-audit = craneLib.cargoAudit {
          inherit src advisory-db;
        };
      };

      packages.default = alpacka;

      apps.default = flake-utils.lib.mkApp {
        drv = alpacka;
      };

      devShells.default = pkgs.mkShell {
        inputsFrom = builtins.attrValues self.checks;

        # Extra inputs can be added here
        nativeBuildInputs = with pkgs;
          [
            rustToolchain
            alejandra
            rnix-lsp
            pkg-config
            openssl
            git
          ]
          ++ buildInputs;
      };
    });
}

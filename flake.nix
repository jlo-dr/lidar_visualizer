{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    nci = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    parts,
    nci,
    ...
  }:
    parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux"];
      imports = [
        nci.flakeModule
        ./crates.nix
      ];
      perSystem = {
        pkgs,
        config,
        ...
      }: let
        crateOutputs = config.nci.outputs."lidar_vis_webapp";
      in {
        devShells.default = crateOutputs.devShell.overrideAttrs (old: {
          packages = (old.packages or []) ++ [
            pkgs.rust-analyzer
          ];
        });
        packages.default = crateOutputs.packages.release;
      };
    };
}

{...}: {
  perSystem = {
    pkgs,
    config,
    ...
  }: let
    crateName = "lidar_vis_webapp";
  in {
    nci.projects.${crateName}.path = ./.;
    nci.crates.${crateName} = {};
  };
}

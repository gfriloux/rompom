{
  lib,
  config,
  pkgs,
  namespace,
  ...
}: let
  cfg = config.${namespace}.rompom;
in {
  options.${namespace}.rompom = {
    enable = lib.mkEnableOption "rompom ROM packager";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.${namespace}.rompom;
      defaultText = lib.literalExpression "pkgs.${namespace}.rompom";
      description = "The rompom package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [cfg.package];
  };
}

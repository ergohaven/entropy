# Home-manager module for Entropy
#
# Installs the app and, with ibus.enable, the Universal Symbols IBus engine.
#
# Two caveats compared to nixosModules.default:
#   - the Vial udev rule needs root and lives only in the NixOS module;
#   - IBus registration here works through IBUS_COMPONENT_PATH, which the
#     NixOS `i18n.inputMethod` wrapper overwrites with --set. On NixOS use the
#     NixOS module for the engine; this path is for standalone home-manager.
#
# The app's own setup screen will keep reporting the engine as "not installed"
# either way — it only looks at $XDG_DATA_HOME/entropy/ibus/entropy-ibus-engine.
# The input method works regardless; pressing the install button just drops an
# unmanaged copy next to this one.
#
# Usage: imports = [ entropy.homeManagerModules.default ];
#        programs.entropy.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.entropy;
in
{
  options.programs.entropy = {
    enable = lib.mkEnableOption "Entropy keyboard configurator";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "entropy.packages.\${system}.entropy";
      description = "The Entropy package to use.";
    };

    ibus = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Register the Entropy Universal Symbols IBus engine by pointing
          IBUS_COMPONENT_PATH at it. Has no effect if IBus comes from the NixOS
          `i18n.inputMethod` module — use programs.entropy.ibus there instead.
        '';
      };

      package = lib.mkOption {
        type = lib.types.package;
        defaultText = lib.literalExpression "entropy.packages.\${system}.entropy-ibus-engine";
        description = "Package providing the wrapped IBus engine.";
      };
    };
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      { home.packages = [ cfg.package ]; }

      (lib.mkIf cfg.ibus.enable {
        home.packages = [ cfg.ibus.package ];

        # IBus reads components from this list only; keep the distro default so
        # the other engines do not disappear.
        home.sessionVariables.IBUS_COMPONENT_PATH = "${cfg.ibus.package}/share/ibus/component:${pkgs.ibus}/share/ibus/component";

        # IBus caches the registry — after switching generations run
        # `ibus write-cache && ibus restart`, or just log out and back in.
      })
    ]
  );
}

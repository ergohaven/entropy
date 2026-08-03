# Home-manager module for Entropy
#
# Installs the app and, with ibus.enable, the Entropy IBus engine that backs
# Text Expander on Wayland.
#
# Two caveats compared to nixosModules.default:
#   - the Vial udev rule needs root and lives only in the NixOS module;
#   - IBus registration here works through IBUS_COMPONENT_PATH, which the
#     NixOS `i18n.inputMethod` wrapper overwrites with --set. On NixOS use the
#     NixOS module for the engine; this path is for standalone home-manager.
#
# Entropy detects the engine registered this way and replaces the install
# action with "Reload IBus registry", which is all that is left to do here.
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
        default = false;
        example = true;
        description = ''
          Register the Entropy IBus engine, which backs the Text Expander
          feature on Wayland, by pointing IBUS_COMPONENT_PATH at it.

          Opt-in: enabling Entropy alone neither installs nor selects an input
          method. Has no effect if IBus comes from the NixOS
          `i18n.inputMethod` module, which pins IBUS_COMPONENT_PATH with
          `--set` — use programs.entropy.ibus in the NixOS module there.
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

        # IBus reads components from this list only; prepend our paths and
        # keep whatever IBUS_COMPONENT_PATH already had, so other engines
        # relying on it do not disappear.
        home.sessionSearchVariables.IBUS_COMPONENT_PATH = [
          "${cfg.ibus.package}/share/ibus/component"
          "${pkgs.ibus}/share/ibus/component"
        ];

        # IBus caches the registry, so a daemon started before this generation
        # keeps serving the old one and the layouts stay missing. Entropy's
        # setup screen offers "Reload IBus registry" for exactly this; the
        # equivalent by hand is `ibus write-cache && ibus restart`, or logging
        # out and back in.
      })
    ]
  );
}

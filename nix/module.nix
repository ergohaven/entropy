# NixOS module for Entropy
#
# Covers everything the app would otherwise install imperatively from its GUI,
# which does not survive on NixOS:
#   - linux/udev/install-vial-rules.sh -> /etc/udev/rules.d/59-vial.rules
#   - linux/ibus/install-user.sh       -> ~/.local/share/ibus/component/...
#
# Usage in your flake:
#
#   inputs.entropy.url = "github:ergohaven/entropy";
#
#   nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
#     modules = [
#       entropy.nixosModules.default
#       { programs.entropy.enable = true; }
#     ];
#   };
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.entropy;

  # Mirrors linux/udev/install-vial-rules.sh: match Vial's magic serial and the
  # Ergohaven Bluetooth HID id, then hand the node to the local session.
  #
  # Shipped as a package rather than through services.udev.extraRules because
  # the app looks for a file named exactly 59-vial.rules under /etc, /run,
  # /usr/lib or /lib (src/ui/app_settings.rs: linux_vial_udev_rules_installed).
  # extraRules is merged into 99-local.rules, which would leave Entropy claiming
  # the rule is missing even though hidraw access works.
  vialUdevRules = pkgs.writeTextFile {
    name = "entropy-vial-udev-rules";
    destination = "/lib/udev/rules.d/59-vial.rules";
    text = ''
      # Entropy Vial hidraw access v2
      KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{serial}=="*vial:f64c2b3c*", MODE="0660", GROUP="${cfg.group}", TAG+="uaccess", TAG+="udev-acl"
      KERNEL=="hidraw*", SUBSYSTEM=="hidraw", KERNELS=="0005:E126:*", MODE="0660", GROUP="${cfg.group}", TAG+="uaccess", TAG+="udev-acl"
    '';
  };
in
{
  # nixpkgs carries its own programs/ergohaven-entropy.nix declaring the same
  # option, so importing both aborts the eval. This module supersedes it: it
  # tracks the version this repo is at and lets the package be overridden.
  disabledModules = [ "programs/ergohaven-entropy.nix" ];

  options.programs.entropy = {
    enable = lib.mkEnableOption "Entropy keyboard configurator";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "entropy.packages.\${system}.entropy";
      description = "The Entropy package to use.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "input";
      description = ''
        Group granted read/write access to Vial hidraw devices. Members can
        talk to the keyboard even where uaccess does not apply (over SSH, or
        from a session that does not own the seat).
      '';
    };

    ibus = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Register the Entropy IBus engine, which backs the Text Expander
          feature on Wayland. Universal Symbols do not need it on firmware
          that exposes native RMK key actions. Enables IBus as the system
          input method if nothing else has claimed that slot.
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
      {
        environment.systemPackages = [ cfg.package ];

        services.udev.packages = [ vialUdevRules ];
      }

      (lib.mkIf cfg.ibus.enable {
        # ibus-with-plugins hardcodes IBUS_COMPONENT_PATH to its own store path,
        # so an engine is only ever found if it is joined into that package —
        # dropping the component XML into XDG_DATA_DIRS does nothing.
        i18n.inputMethod = {
          enable = lib.mkDefault true;
          type = lib.mkDefault "ibus";
          ibus.engines = [ cfg.ibus.package ];
        };

        warnings = lib.optional (config.i18n.inputMethod.type or null != "ibus") ''
          programs.entropy.ibus.enable is on, but i18n.inputMethod.type is
          "${toString config.i18n.inputMethod.type}" — the Entropy engine will not
          be loaded. Set i18n.inputMethod.type = "ibus" or turn the option off.
        '';
      })
    ]
  );
}

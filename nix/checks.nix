# Evaluation checks for the NixOS module, run by `nix flake check`.
#
# Everything here is decided at evaluation time: each `assertMsg` aborts the
# build of the marker derivation below, so a regression fails CI rather than
# silently shipping a module that enables an input method behind the user's
# back or collides with the Entropy module nixpkgs itself ships.
{
  self,
  nixpkgs,
  system,
  pkgs,
}:
let
  inherit (nixpkgs) lib;

  # A throwaway machine that only exists to evaluate the module. Note that
  # nixosSystem pulls in *all* of nixpkgs' modules, including
  # nixos/modules/programs/ergohaven-entropy.nix — so every check below also
  # proves the two can coexist.
  evalWith =
    module:
    (nixpkgs.lib.nixosSystem {
      modules = [
        self.nixosModules.default
        {
          nixpkgs.hostPlatform = system;
          boot.loader.grub.devices = [ "/dev/sda" ];
          fileSystems."/" = {
            device = "/dev/sda1";
            fsType = "ext4";
          };
          system.stateVersion = "25.11";
        }
        module
      ];
    }).config;

  bare = evalWith { programs.entropy.enable = true; };

  withIbus = evalWith {
    programs.entropy.enable = true;
    programs.entropy.ibus.enable = true;
    i18n.inputMethod = {
      enable = true;
      type = "ibus";
    };
  };

  ibusWithoutInputMethod = evalWith {
    programs.entropy.enable = true;
    programs.entropy.ibus.enable = true;
  };

  customGroup = evalWith {
    programs.entropy.enable = true;
    programs.entropy.group = "plugdev";
  };

  hasPackage = pname: packages: lib.any (drv: drv.pname or drv.name == pname) packages;

  engineNames = config: map (drv: drv.pname or drv.name) config.i18n.inputMethod.ibus.engines;

  assertions = [
    # The app and the Vial rule are what `enable` is for.
    (lib.assertMsg (hasPackage "entropy" bare.environment.systemPackages) "programs.entropy.enable does not install the entropy package")
    (lib.assertMsg (hasPackage "entropy-vial-udev-rules" bare.services.udev.packages) "the Vial udev rule is missing from services.udev.packages")

    # The rule names a group; that group has to exist, including the default
    # "input" (which NixOS declares itself) and any custom one.
    (lib.assertMsg (bare.users.groups ? "input") "the default hidraw group is not declared")
    (lib.assertMsg (
      customGroup.users.groups ? "plugdev"
    ) "a custom programs.entropy.group is not created")
    (lib.assertMsg (
      bare.users.groups.input.gid == 174
    ) "redeclaring the input group dropped its well-known gid")

    # Enabling Entropy must not drag in an input method.
    (lib.assertMsg (
      !bare.programs.entropy.ibus.enable
    ) "programs.entropy.ibus.enable defaults to true; it has to be opt-in")
    (lib.assertMsg (
      !bare.i18n.inputMethod.enable
    ) "enabling Entropy turned on i18n.inputMethod, which is the system owner's choice")
    (lib.assertMsg (
      bare.i18n.inputMethod.ibus.engines == [ ]
    ) "enabling Entropy registered an IBus engine without ibus.enable")
    (lib.assertMsg (bare.warnings == [ ]) "a plain programs.entropy.enable emitted warnings")

    # Opting in registers the engine and stays quiet when IBus is the choice.
    (lib.assertMsg (
      engineNames withIbus == [ "entropy-ibus-engine" ]
    ) "ibus.enable did not register exactly the Entropy engine")
    (lib.assertMsg (
      withIbus.warnings == [ ]
    ) "ibus.enable warned even though i18n.inputMethod.type is \"ibus\"")

    # ... and warns instead of silently doing nothing when it is not.
    (lib.assertMsg (
      lib.length ibusWithoutInputMethod.warnings == 1
    ) "ibus.enable without an IBus input method did not emit exactly one warning")

    # Coexistence with nixpkgs' own module: ours wins, and it is our package
    # that gets installed, not pkgs.ergohaven-entropy.
    (lib.assertMsg (
      bare.programs.entropy ? ibus
    ) "programs.entropy.ibus is absent — nixpkgs' Entropy module took precedence")
    (lib.assertMsg (
      bare.programs.entropy.package == self.packages.${system}.entropy
    ) "programs.entropy.package is not the package from this flake")
  ];
in
{
  # Forces the assertions above; the output itself carries no information.
  module-eval =
    assert lib.all lib.id assertions;
    pkgs.runCommand "entropy-module-eval" { } "touch $out";
}

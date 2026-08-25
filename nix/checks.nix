# Evaluation checks for the NixOS and home-manager modules, run by
# `nix flake check`.
#
# Everything here is decided at evaluation time: each `assertMsg` aborts the
# build of the marker derivation below, so a regression fails CI rather than
# silently shipping a module that enables an input method behind the user's
# back or collides with the Entropy module nixpkgs itself ships.
{
  self,
  nixpkgs,
  home-manager,
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
          # Nothing here builds system.build.toplevel, so no bootloader or
          # filesystem definitions are needed; stateVersion only silences the
          # eval warning about its default.
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

  # Evaluate the public module through home-manager itself so its real option
  # types and module plumbing stay covered as both projects evolve.
  evalHomeManagerWith =
    module:
    (home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules = [
        self.homeManagerModules.default
        {
          home.username = "entropy-test";
          home.homeDirectory = "/home/entropy-test";
          home.stateVersion = "25.11";
        }
        module
      ];
    }).config;

  homeBare = evalHomeManagerWith { programs.entropy.enable = true; };

  homeWithIbus = evalHomeManagerWith {
    programs.entropy.enable = true;
    programs.entropy.ibus.enable = true;
  };

  hasPackage = pname: packages: lib.any (drv: drv.pname or drv.name == pname) packages;

  engineNames = config: map (drv: drv.pname or drv.name) config.i18n.inputMethod.ibus.engines;

  nixosAssertions = [
    # The app and the Vial rule are what `enable` is for.
    (lib.assertMsg (hasPackage "entropy" bare.environment.systemPackages) "programs.entropy.enable does not install the entropy package")
    (lib.assertMsg (hasPackage "entropy-vial-udev-rules" bare.services.udev.packages) "the Vial udev rule is missing from services.udev.packages")

    # The rule names a group; both the dedicated default and an override have
    # to exist so the installed rule never points at a missing group.
    (lib.assertMsg (
      bare.programs.entropy.group == "entropy"
    ) "the default hidraw group is not dedicated to Entropy")
    (lib.assertMsg (bare.users.groups ? "entropy") "the default hidraw group is not declared")
    (lib.assertMsg (
      customGroup.users.groups ? "plugdev"
    ) "a custom programs.entropy.group is not created")

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

  homeManagerAssertions = [
    (lib.assertMsg (hasPackage "entropy" homeBare.home.packages) "home-manager enable does not install the entropy package")
    (lib.assertMsg (
      !homeBare.programs.entropy.ibus.enable
    ) "home-manager ibus.enable defaults to true; it has to be opt-in")
    (lib.assertMsg (
      !(homeBare.home.sessionSearchVariables ? IBUS_COMPONENT_PATH)
    ) "home-manager enable registered the IBus engine without ibus.enable")
    (lib.assertMsg (hasPackage "entropy-ibus-engine" homeWithIbus.home.packages) "home-manager ibus.enable does not install the Entropy engine")
    (lib.assertMsg (
      homeWithIbus.home.sessionSearchVariables.IBUS_COMPONENT_PATH == [
        "${self.packages.${system}.entropy-ibus-engine}/share/ibus/component"
        "${pkgs.ibus}/share/ibus/component"
      ]
    ) "home-manager ibus.enable registered the wrong component search paths")
  ];
in
{
  # Force the assertions above; the outputs themselves carry no information.
  module-eval =
    assert lib.all lib.id nixosAssertions;
    pkgs.runCommand "entropy-module-eval" { } "touch $out";

  home-manager-module-eval =
    assert lib.all lib.id homeManagerAssertions;
    pkgs.runCommand "entropy-home-manager-module-eval" { } "touch $out";
}

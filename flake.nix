{
  description = "Entropy - desktop app for configuring Vial-QMK and Vial-RMK programmable keyboards";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      # Read version from Cargo.toml so the flake never drifts from the crate.
      cargoToml = fromTOML (builtins.readFile ./Cargo.toml);
      version = cargoToml.package.version;

      # Compile-time native deps, resolved through pkg-config.
      #   gtk3/glib  - rfd 0.15 uses the GTK3 backend for file dialogs
      #   libxkbcommon/wayland/xorg - winit backends enabled by eframe features
      #   libGL      - eframe "glow" renderer
      # hidapi is built with `linux-native-basic-udev`, which is pure Rust
      # (basic-udev), so no libudev/libhidapi is required.
      commonNativeDeps =
        pkgs: with pkgs; [
          gtk3
          glib
          libxkbcommon
          wayland
          libGL
          libx11
          libxcursor
          libxrandr
          libxi
          libxcb
        ];

      # Libraries that are dlopen'd at runtime (x11-dl, wayland-sys, glutin,
      # libxkbcommon) and therefore never land in the binary's RUNPATH.
      runtimeLibs =
        pkgs: with pkgs; [
          libGL
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxrandr
          libxi
          libxcb
        ];

      # Upstream ships no icon for Linux (only assets/entropy.ico for Windows);
      # scripts/build_linux_appimage.sh generates this SVG inline. Same artwork.
      iconSvg =
        pkgs:
        pkgs.writeText "entropy.svg" ''
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
            <rect width="256" height="256" rx="48" fill="#101828"/>
            <path d="M63 68h130v34H103v35h80v34h-80v-23H63V68z" fill="#5EEAD4"/>
            <path d="M63 154h130v34H63z" fill="#F97316"/>
          </svg>
        '';
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          lib = pkgs.lib;
        in
        rec {
          entropy = pkgs.rustPlatform.buildRustPackage {
            pname = "entropy";
            inherit version;
            src = self;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [
              pkg-config
              # rfd pops GTK dialogs: needs GSettings schemas and pixbuf loaders.
              wrapGAppsHook3
              copyDesktopItems
            ];

            buildInputs = commonNativeDeps pkgs;

            # Unit tests are pure (i18n catalogs, layout parsing, bundled script
            # contents) — no keyboard, X server or network needed.
            doCheck = true;

            desktopItems = [
              (pkgs.makeDesktopItem {
                name = "entropy";
                exec = "entropy";
                icon = "entropy";
                desktopName = "Entropy";
                comment = cargoToml.package.description;
                categories = [
                  "Utility"
                  "Settings"
                ];
                keywords = [
                  "keyboard"
                  "vial"
                  "qmk"
                  "rmk"
                ];
              })
            ];

            postInstall = ''
              install -Dm644 ${iconSvg pkgs} \
                $out/share/icons/hicolor/scalable/apps/entropy.svg
            '';

            preFixup = ''
              gappsWrapperArgs+=(
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath (runtimeLibs pkgs)}"
              )
            '';

            meta = {
              inherit (cargoToml.package) description homepage;
              license = lib.licenses.gpl3Plus;
              mainProgram = "entropy";
              platforms = supportedSystems;
            };
          };

          # linux/ibus/entropy-ibus-engine is a python3 script that the app
          # normally copies into ~/.local/share via linux/ibus/install-user.sh.
          # On NixOS its `#!/usr/bin/env python3` shebang would resolve to an
          # interpreter without the gi/IBus bindings, so it is wrapped here.
          #
          # The component XML lands in share/ibus/component/, which IBus scans
          # through XDG_DATA_DIRS — putting this package in systemPackages (or
          # home.packages) is all the registration needed, no install script.
          entropy-ibus-engine = pkgs.stdenv.mkDerivation {
            pname = "entropy-ibus-engine";
            inherit version;
            src = self;

            nativeBuildInputs = with pkgs; [
              makeWrapper
              gobject-introspection
            ];
            buildInputs = [
              (pkgs.python3.withPackages (ps: [ ps.pygobject3 ]))
              pkgs.ibus
              pkgs.glib
            ];

            dontBuild = true;

            installPhase = ''
              runHook preInstall

              install -Dm755 linux/ibus/entropy-ibus-engine \
                $out/libexec/entropy-ibus-engine
              patchShebangs $out/libexec/entropy-ibus-engine
              wrapProgram $out/libexec/entropy-ibus-engine \
                --prefix GI_TYPELIB_PATH : "$GI_TYPELIB_PATH"

              substitute linux/ibus/entropy-universal-symbols.xml.in \
                $out/share/ibus/component/entropy-universal-symbols.xml \
                --replace-fail '@ENGINE_PATH@' \
                  "$out/libexec/entropy-ibus-engine"

              runHook postInstall
            '';

            preInstall = ''
              mkdir -p $out/share/ibus/component
            '';

            meta = {
              description = "IBus engine for Entropy Universal Symbols and Text Expander";
              inherit (cargoToml.package) homepage;
              license = pkgs.lib.licenses.gpl3Plus;
              platforms = supportedSystems;
              # Required by the i18n.inputMethod.ibus.engines option type;
              # ibus-with-plugins hardcodes IBUS_COMPONENT_PATH to its own
              # store path, so joining it there is the only registration that
              # IBus actually honours on NixOS.
              isIbusEngine = true;
            };
          };

          default = entropy;
        }
      );

      # Everything the two imperative install scripts would do: the Vial hidraw
      # udev rule (linux/udev/install-vial-rules.sh) and the IBus engine
      # registration (linux/ibus/install-user.sh), both declarative.
      nixosModules.default =
        { lib, pkgs, ... }:
        let
          selfPkgs = self.packages.${pkgs.stdenv.hostPlatform.system};
        in
        {
          imports = [ ./nix/module.nix ];
          programs.entropy.package = lib.mkDefault selfPkgs.entropy;
          programs.entropy.ibus.package = lib.mkDefault selfPkgs.entropy-ibus-engine;
        };

      # Same, minus the udev rule — that one needs root, use the NixOS module.
      homeManagerModules.default =
        { lib, pkgs, ... }:
        let
          selfPkgs = self.packages.${pkgs.stdenv.hostPlatform.system};
        in
        {
          imports = [ ./nix/hm-module.nix ];
          programs.entropy.package = lib.mkDefault selfPkgs.entropy;
          programs.entropy.ibus.package = lib.mkDefault selfPkgs.entropy-ibus-engine;
        };

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              rust-analyzer
              clippy
              rustfmt
              pkg-config
              # scripts/check_i18n.py
              python3
            ];

            buildInputs = commonNativeDeps pkgs;

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);

            # Same as wrapGAppsHook3 does for the packaged build.
            XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:${pkgs.hicolor-icon-theme}/share";
          };
        }
      );

      formatter = forAllSystems (system: (import nixpkgs { inherit system; }).nixfmt-tree);
    };
}

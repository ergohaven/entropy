# nfpm config for the Entropy desktop app (deb / rpm / archlinux).
# Values in ${...} are filled from the environment by the Taskfile:
#   PKG_ARCH     nfpm arch (amd64 / arm64)
#   VERSION      package version (from Cargo.toml)
#   ENTROPY_BIN  path to the already-built entropy binary for PKG_ARCH
# Dependencies are pinned per-format on purpose: auto-detection (ldd/find-requires)
# is unreliable under cross-compilation, so depends are declared by hand.
name: ergohaven-entropy
arch: ${PKG_ARCH}
platform: linux
version: ${VERSION}
section: utils
priority: optional
maintainer: Ergohaven <noreply@ergohaven.example>
description: |
  Desktop app for configuring Vial-QMK and Vial-RMK programmable keyboards.
  Modern interface for layouts, keycodes, macros, lighting, pointing controls
  and firmware settings of Vial-compatible HID devices.
homepage: https://github.com/ergohaven/entropy
license: GPL-3.0-or-later

overrides:
  deb:
    depends:
      - libc6
      - libgtk-3-0
      - libxkbcommon0
      - libx11-6
      - libxcb1
      - libgl1
      - libudev1
  rpm:
    depends:
      - gtk3
      - libxkbcommon
      - libX11
      - libxcb
      - mesa-libGL
  archlinux:
    depends:
      - gtk3
      - libxkbcommon
      - libx11
      - libxcb
      - libglvnd
      - systemd-libs

contents:
  - src: ${ENTROPY_BIN}
    dst: /usr/bin/entropy
    file_info:
      mode: 0755

  - src: ./packaging/linux/entropy.desktop
    dst: /usr/share/applications/entropy.desktop

  - src: ./packaging/linux/com.ergohaven.entropy.metainfo.xml
    dst: /usr/share/metainfo/com.ergohaven.entropy.metainfo.xml

  - src: ./packaging/linux/59-vial.rules
    dst: /usr/lib/udev/rules.d/59-vial.rules

  - src: ./assets/entropy.svg
    dst: /usr/share/icons/hicolor/scalable/apps/entropy.svg

  - src: ./assets/icons/hicolor/32x32/apps/entropy.png
    dst: /usr/share/icons/hicolor/32x32/apps/entropy.png
  - src: ./assets/icons/hicolor/64x64/apps/entropy.png
    dst: /usr/share/icons/hicolor/64x64/apps/entropy.png
  - src: ./assets/icons/hicolor/128x128/apps/entropy.png
    dst: /usr/share/icons/hicolor/128x128/apps/entropy.png
  - src: ./assets/icons/hicolor/256x256/apps/entropy.png
    dst: /usr/share/icons/hicolor/256x256/apps/entropy.png
  - src: ./assets/icons/hicolor/512x512/apps/entropy.png
    dst: /usr/share/icons/hicolor/512x512/apps/entropy.png

  - src: ./LICENSE
    dst: /usr/share/doc/ergohaven-entropy/LICENSE

scripts:
  postinstall: ./packaging/nfpm/postinstall.sh
  postremove: ./packaging/nfpm/postremove.sh

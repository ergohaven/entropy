# Entropy

Modern app for programmable keyboards and input devices, built by Ergohaven.

[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/badge/latest-v0.3.9-lightgrey.svg)](https://github.com/ergohaven/entropy/releases)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey.svg)](#platforms)
[![Firmware](https://img.shields.io/badge/firmware-Vial--QMK%20%7C%20Vial--RMK-lightgrey.svg)](#compatibility)

![Entropy layout editor](assets/entropy-layout-screenshot.png)

Entropy is a desktop app with a modern, minimalist, and intuitive interface for
configuring programmable input devices running Vial-QMK or Vial-RMK firmware:
split keyboards, macropads, trackballs, touchpad modules, and other hardware
that exposes keyboard-style firmware features through HID.

It is designed to feel direct and predictable: connect a device, pick it from the
device list, and work through layout, keycodes, macros, lighting, pointing controls,
and firmware settings from one coherent interface.

## Screenshots

<p align="center">
  <img src="assets/key-picker-dark.png" alt="Key Picker in dark theme" width="49%">
  <img src="assets/key-picker-light.png" alt="Key Picker in light theme" width="49%">
  <img src="assets/matrix-tester.png" alt="Matrix Tester" width="49%">
  <img src="assets/text-expander.png" alt="Text Expander" width="49%">
</p>

## Main Features

- Modern, minimalist, intuitive design for complex device configuration
- Complete Vial workflow: layouts, keycodes, macros, combos, tap dance,
  key overrides, RGB, pointing controls, and firmware settings
- Support for keyboards, macropads, trackballs, touchpads, encoders, displays,
  and modular input devices
- Text Expander for local shortcuts from programmable devices
- Firmware-native Universal Symbols for consistent EN/RU punctuation
- Fast keycode picker with layouts, symbols, modifiers, macros, and smart filtering
- Custom names for layers, combos, macros, tap dance entries, and other device objects
- Live Features as a built-in qmk-hid-host replacement for firmware host data
- Matrix Tester and Layout Indicator for testing and daily layer visibility
- Layer hover preview, encoder controls, custom labels, and multilingual legends
- Advanced pages for Auto Shift, Mouse Keys, Tap-Hold, One Shot, Grave Escape,
  Magic, Layer LEDs, touchpad settings, and modules
- Light/dark themes, accent color, UI scaling, settings import/export, and tray mode
- Linux udev helper plus optional IBus integration for Text Expander

## Platforms

| Platform | Status | Package |
| --- | --- | --- |
| Linux x86_64 | Primary target | AppImage |
| Windows x86_64 | Release target | Portable EXE |
| macOS arm64 (Apple Silicon) | Release target | Unsigned DMG |
| macOS x86_64 (Intel) | Release target | Unsigned DMG |

Public builds are published for Linux, Windows, and macOS. macOS builds are
unsigned and not notarized for now.

## Downloads

Release builds are published on the
[GitHub Releases](https://github.com/ergohaven/entropy/releases) page:

- `entropy-v0.3.9-x86_64.AppImage`
- `entropy-v0.3.9-windows-x86_64.exe`
- `entropy-v0.3.9-macos-arm64.dmg`
- `entropy-v0.3.9-macos-x86_64.dmg`

Stable tags such as `v0.3.9` publish a regular GitHub release and mark it as
latest. Tags with a suffix, such as `v0.3.9-rc.1`, publish the same artifacts as
a GitHub prerelease.

Windows builds are unsigned for now, so Windows SmartScreen may warn before
launching the app.

macOS DMG builds are unsigned and not notarized for now. On Apple Silicon,
use the `macos-arm64` build; the `macos-x86_64` build is for Intel Macs. To run
a downloaded DMG on macOS:

1. Open the `.dmg`
2. Drag `Entropy.app` to `/Applications`
3. Remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Entropy.app
```

4. Launch Entropy:

```sh
open /Applications/Entropy.app
```

## Quick Start

1. Download the build for your platform from GitHub Releases
2. Connect a Vial-compatible device
3. On Linux, install Vial udev rules if Entropy cannot open the device
   and install the IBus backend if you want Text Expander on Linux
4. Launch Entropy
5. Select the device from the top-left device dropdown
6. Edit layers, keycodes, advanced firmware features, or app settings
7. Save/write changes when the edited feature requires it

## Linux Device Access

Vial devices use hidraw access on Linux. If your device appears but cannot be opened,
use the **Install Vial udev rules** action in Entropy settings, or install the
included udev rule manually from a source checkout:

```sh
./linux/udev/install-vial-rules.sh
```

Replug the device after installing the rule.

## Linux IBus Backend

On Linux, Entropy uses IBus for Text Expander input. Use
the **Install IBus** action in Entropy settings to install the bundled Entropy
IBus engine. The AppImage includes the installer and engine, so a separate source
checkout is not required.

IBus itself and its Python bindings must still be installed by the system package
manager. On Debian/Ubuntu-like systems:

```sh
sudo apt-get install ibus python3-gi gir1.2-ibus-1.0
```

After installation, restart IBus if Entropy did not do it automatically, then add
an **Entropy Text Expander** layout as an input source in your desktop input settings.

## Universal Symbols

Universal Symbols are native RMK firmware actions for punctuation that should
produce the same character in English and Russian layouts. Supported firmware
tracks the active EN/RU layout and emits ordinary HID key presses, so assigned
symbols work without Entropy or another background service.

When Entropy is running, the existing Layout Sync bridge reports the active OS
layout to the keyboard and corrects firmware state after layout changes made by
the operating system. Manual Toggle, Sync, English, and Russian actions remain
available for fully autonomous use.

Entropy shows the Universal Symbols picker section only when connected firmware
advertises this capability. The catalog intentionally contains only punctuation
implemented by the firmware; the former Unicode typography, arrows, math, and
currency extras are not exposed.

## Compatibility

Entropy currently communicates with Vial-compatible HID devices. Its UI is designed
for programmable keyboards and adjacent input devices such as macropads, trackballs,
touchpads, and encoder/display modules when those features are exposed by firmware.

Best-tested hardware is Ergohaven hardware and Vial-compatible QMK/RMK-style devices.
Firmware support varies by device; Entropy hides firmware-gated pages when the
connected device does not expose the required capability.

Not in scope for this release:

- Browser-only configuration
- Mobile platforms

## Development

Install a stable Rust toolchain, then build the desktop app:

```sh
cargo run
cargo build --release
```

Linux builds require native GUI/HID dependencies. On Debian/Ubuntu-like systems:

```sh
sudo apt-get install \
  libhidapi-dev \
  libudev-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libxkbcommon-dev \
  libssl-dev \
  libgtk-3-dev
```

Build a macOS app bundle and DMG on macOS:

```sh
scripts/build_macos_app.sh
```

Build a Windows release binary from Linux with the GNU target:

```sh
cargo build --release --target x86_64-pc-windows-gnu
```

## Changelog

- [CHANGELOG.md](CHANGELOG.md)

## License

Entropy is licensed under GPL-3.0-or-later. See [LICENSE](LICENSE).

# Changelog

All notable public changes to Entropy are tracked here.

Entropy uses public release versions for GitHub releases and internal build versions
for development history. The first public beta is `v0.1.0-beta.1`.

## v0.1.5 - Public Beta

### Main Features

- Added About Entropy update checks through GitHub Releases for Linux, Windows, and macOS
- Added platform-aware update asset selection for AppImage, Windows executable, and macOS DMG downloads
- Added Changelog and Download actions to About Entropy after checking for updates
- Added macro choices to Tap Dance assignment for `On tap` and `On double tap`
- Tap Dance fields now show custom macro names when a macro keycode is assigned

### Fixes

- Centered About Entropy update action buttons under the settings list
- Removed the application version from the native window title bar
- Switched the application package version back to the public GitHub release channel

## v0.1.0-beta.1 - Public Beta

Based on internal build `v1.13.153`.

### Main Features

- Visual Vial layout editor with layers, key assignment, encoder controls, and custom keycode labels
- Modern keycode picker with Basic, Symbols, Modifiers, Special, RGB, Macro, Tap Dance, and Custom tabs
- Advanced firmware pages for Combos, Key Overrides, Auto Shift, Tap-Hold and One Shot, Mouse Keys, Magic, Grave Escape, Layer LEDs, RGB, Modules, Touchpad, and Live Features where supported by firmware
- Custom names for layers, combos, macros, tap dance entries, and other device objects
- Live Features as a built-in qmk-hid-host replacement for firmware host data
- Macro editor, Tap Dance editor, Combo editor, and Key Override editor
- Matrix Tester for supported Vial devices
- Layout Indicator companion window with opacity, pinning, layer labels, and pressed-key display
- App settings for language, key legends, shifted number symbols, accent color, UI scale, background mode, startup, and Linux Vial udev rules
- Diagnostics mode in App Settings writes focused rotating troubleshooting logs when enabled
- Local Text Expander and Universal Symbols integrations
- Linux IBus and Fcitx5 helper backends for Wayland input-method workflows

### Distribution

- Linux x86_64 AppImage
- Windows x86_64 portable ZIP
- SHA-256 checksum file

### Documentation

- README screenshot gallery for Key Picker, Matrix Tester, and Text Expander
- README now states the Vial-QMK and Vial-RMK firmware scope near the top
- README documents Linux IBus installation and required system dependencies

### Fixes

- Linux setup actions can run bundled IBus, Fcitx5, and udev scripts from packaged builds
- Encoder visibility now respects Vial layout-display conditions, so Phenom encoder press keys hide together with their encoder controls
- Segmented controls now shrink long localized labels to stay inside their button bounds
- Windows now keeps Entropy single-instance: repeated launches restore the existing tray instance instead of starting a second app

### Beta Notes

- Windows builds are unsigned during beta
- Firmware-gated features appear only when the connected device exposes the required Vial/QMK settings
- Browser-only configuration and mobile platforms are not supported in this beta

# Changelog

All notable public changes to Entropy are tracked here.

Entropy uses public release versions for GitHub releases and internal build versions
for development history. The first public beta is `v0.1.0-beta.1`.

## v0.2.0 - Public Beta

### Main Features

- Added the local Typing Trainer under Advanced with timed and word-count runs, focus mode, EN/RU language packs, punctuation/digits modifiers, result actions, and run history
- Added Live Features layout sync for ru/en custom firmware keys, with an on/off switch
- Added optional macro descriptions for picker tooltips and `.entlayout` export/import
- Added an automatic About Entropy update indicator when a newer GitHub Release is available
- Remapped Universal Symbols transport from F21-F24 to F13-F20 modifier banks for better cross-platform input compatibility (@IgorArkhipov, #51)
- Added One-Shot Mod chord choices for Ctrl+Shift, Ctrl+Alt, Shift+Alt, and Shift+GUI in the keycode picker (@IgorArkhipov, #55)
- Allowed custom keycodes in Tap Dance hold fields while keeping macros limited to tap-style fields (@IgorArkhipov, #52)

### Fixes

- Fixed the Linux udev autoconnect loop
- Key picker now opens on the tab that matches the clicked key's current keycode
- Saved combo entries correctly over Bluetooth (@IgorArkhipov, #33)
- Detected Bluetooth HID transport on macOS (@IgorArkhipov, #24)
- Retried macOS DMG creation when packaging hits transient detach timing (@IgorArkhipov, #57)
- Throttled tray background updates to reduce idle CPU usage (@IgorArkhipov, #54)
- Tried the macOS event tap even when the permission preflight is stale (@IgorArkhipov, #62)
- Fixed rapid Windows Universal Symbols punctuation pairs (@IgorArkhipov, #53)
- Verified keycode writebacks to catch silent device write failures (@IgorArkhipov, #61)
- Verified module setting writebacks to catch silent device write failures (@IgorArkhipov, #56)
- Guarded unsupported RMK macro layer actions (@IgorArkhipov, #60)
- Hid unsupported Tap-Hold settings when firmware support is absent (@IgorArkhipov, #63)
- Improved connect crash diagnostics for failure reports (@IgorArkhipov, #48)
- Polished Typing Trainer history, focus mode, final stats, language controls, and punctuation/digits toggles

## v0.1.5 - Public Beta

### Main Features

- Added About Entropy update checks through GitHub Releases for Linux, Windows, and macOS
- Added platform-aware update asset selection for AppImage, Windows executable, and macOS DMG downloads
- Added Changelog and Download actions to About Entropy after checking for updates
- Added macro choices to Tap Dance assignment for `On tap` and `On double tap`
- Tap Dance fields now show custom macro names when a macro keycode is assigned

- Added pressed-only Layout Indicator mode (@IgorArkhipov, #34)
- Added Universal Symbols arrow key support, including Linux backend sync and EditPlus fallback handling (@IgorArkhipov, #37, #44, #45)
- Exposed trackball module settings tabs (@IgorArkhipov, #36)
- Added single-instance lock override and repeated-launch guard (@IgorArkhipov, #26)
- Improved macOS packaging and CI with explicit release targets, package architecture validation, signed bundle metadata, Apple Silicon diagnostics, and arm64 PR coverage (@IgorArkhipov, #18, #20, #21, #22, #23)

### Fixes

- Centered About Entropy update action buttons under the settings list
- Removed the application version from the native window title bar
- Switched the application package version back to the public GitHub release channel
- Fixed held `MO` layer state in Layout Indicator (@IgorArkhipov, #31)
- Preserved unknown macro bytecode during macro round-trips (@IgorArkhipov, #32)
- Synced duplicate Layer LED color settings (@IgorArkhipov, #39)
- Guarded device connect against zero layer count (@IgorArkhipov, #46)
- Stabilized macOS HID initialization and bounded macOS automation waits (@IgorArkhipov, #19, #25)
- Reduced clippy and deprecation noise around connect-task payloads, complex types, and egui rect allocation (@IgorArkhipov, #29, #30, #40, #41, #43)

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

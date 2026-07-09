# Changelog

All notable public changes to Entropy are tracked here.

Entropy uses public release versions for GitHub releases and internal build versions
for development history. The first public beta is `v0.1.0-beta.1`.

## v0.1.5 - Public Beta

### Main Features

- Added Live Features layout sync for ru/en custom firmware keys, with an on/off switch
- Added a local Typing Trainer under Advanced with timed runs, live WPM, accuracy, and errors
- Added optional macro descriptions for picker tooltips and `.entlayout` export/import
- Added About Entropy update checks through GitHub Releases for Linux, Windows, and macOS
- Added platform-aware update asset selection for AppImage, Windows executable, and macOS DMG downloads
- Added Changelog and Download actions to About Entropy after checking for updates
- Added macro choices to Tap Dance assignment for `On tap` and `On double tap`
- Tap Dance fields now show custom macro names when a macro keycode is assigned

### IgorArkhipov Contributions

- Added pressed-only Layout Indicator mode
- Added Universal Symbols arrow key support, including Linux backend sync and EditPlus fallback handling
- Exposed trackball module settings tabs
- Added single-instance lock override and repeated-launch guard
- Improved macOS packaging and CI with explicit release targets, package architecture validation, signed bundle metadata, Apple Silicon diagnostics, and arm64 PR coverage

### Fixes

- Moved Typing Trainer stats into the final result view so they replace the text only after the run ends
- Moved the Typing Trainer restart button from the top controls into the former finished status position
- Raised the Typing Trainer focus-mode timer to the same height as the normal time stat
- Showed the Typing Trainer remaining time centered above the text while focus mode hides the chrome
- Anchored Typing Trainer focus-mode chrome to the original layout viewport so top navigation does not drift
- Kept Typing Trainer chrome and page geometry anchored during focus-mode fade transitions
- Smoothed Typing Trainer focus-mode fade-out and hid the bottom footer controls without moving the text
- Hid the full Entropy chrome while Typing Trainer focus mode is active, keeping the text block in a stable position
- Hid the Typing Trainer controls and stats while typing, restoring them on mouse movement or run finish
- Limited the Typing Trainer text window to four visible typing lines
- Paused the Typing Trainer timer while its page is inactive, resuming only when typing continues
- Kept Typing Trainer page updates filled with a uniform visible text block
- Updated the Typing Trainer text window by full visible pages instead of shifting one line at a time
- Kept the Typing Trainer text window following the caret so next words appear instead of typing into hidden text
- Continued Typing Trainer runs with a fresh text when the current text is fully typed before time runs out
- Moved the Typing Trainer finished status below the text area so it no longer overlaps the final line
- Centered About Entropy update action buttons under the settings list
- Removed the application version from the native window title bar
- Switched the application package version back to the public GitHub release channel
- Fixed held `MO` layer state in Layout Indicator
- Preserved unknown macro bytecode during macro round-trips
- Synced duplicate Layer LED color settings
- Guarded device connect against zero layer count
- Stabilized macOS HID initialization and bounded macOS automation waits
- Reduced clippy and deprecation noise around connect-task payloads, complex types, and egui rect allocation

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

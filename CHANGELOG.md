# Changelog

All notable public changes to Entropy are tracked here.

Entropy uses public release versions for GitHub releases and internal build versions
for development history. The first public beta is `v0.1.0-beta.1`.

## v0.3.20-rc.1 - Test Candidate

### Improvements

- Moved Macro and Tap Dance assignment into the first Special-key group and removed the separate Advanced picker tab
- Made Macro and Tap Dance lists and slot dialogs adapt to available window space, including localized action labels
- Rendered Text Expander picker choices and replacement-field emoji with the bundled color Twemoji assets

### Fixes

- Kept Layout Indicator polling in the root background lifecycle and repainted its viewport only after fresh matrix data, preventing focus-return freezes
- Normalized emoji presentation selectors before Twemoji lookup so the smiling-face picker entry no longer falls back to the old font

## v0.3.19-rc.1 - Test Candidate

### Improvements

- Rebuilt Macro actions as sequential in-section slots and anchored shared page actions outside scrolling content
- Aligned Advanced Macro and Tap Dance choices with the compact picker controls and restored interaction after closing slot selection with Escape
- Added persistent Linux IBus installation detection and corrected Entropy input-source names

### Fixes

- Moved Layout Indicator matrix reads to the serialized background HID worker to prevent focus-transfer stalls and reentrant device access
- Removed the repeated keymap-read timeout on incompatible QMK/Vial devices and added a fail-fast per-key compatibility path
- Rejected modifier keycodes as Key Override triggers and disabled invalid stored overrides before they can consume regular input

## v0.3.10 - Public Beta

### Fixes

- Kept Windows automatic startup hidden in the system tray after eframe's first-frame window reveal

## v0.3.9 - Public Beta

### Main Features

- Added firmware-native Universal Symbols and Russian letters to Combo outputs and all Tap Dance actions on compatible RMK firmware, with lossless `.entlayout` import and export
- Added per-combo activation layers with `All layers` and individual layer choices on compatible RMK firmware
- Added automatic firmware update checks in About Device with exact package downloads for all 14 supported Ergohaven RMK profiles
- Restored F13–F24 as regular assignable keys in the main and secondary pickers

### Improvements

- Added direct Windows startup in the system tray without creating a taskbar window
- Made K:04 encoder visibility follow the selected module and kept its three actions anchored to the replaced matrix key
- Removed the manual device-data refresh action; firmware checks and device loading now update automatically
- Removed the unsupported deep-sleep control and kept macro writes asynchronous and change-aware

### Fixes

- Prevented failed macro snapshots from retrying indefinitely or rewriting unchanged firmware buffers
- Kept K:04 encoder placement consistent in the editor, indicator, and PNG, SVG, and PDF exports
- Excluded unsupported standalone trackball firmware from RMK update discovery

## v0.3.4 - Public Beta

### Main Features

- Added portable firmware settings to `.entlayout` files so supported device configuration can be exported and restored together with layouts
- Restored explicit per-side K:04 module selection for None, Encoder, Trackball, and Touchpad instead of automatic module detection

### Improvements

- Made settings imports atomic, with structural legacy-field migration and rollback when applying imported application settings fails
- Pinned and verified the Linux AppImage packaging tool and hardened update-download URL validation

### Fixes

- Kept left and right K:04 module readback aligned over Linux Bluetooth after reconnecting or reopening Entropy
- Prevented optional firmware-version probes and disconnect handling from stalling device connections
- Resent layout state after HID reconnect so the device view stays synchronized

## v0.3.2-rc.1 - Public Beta

### Main Features

- Replaced application-injected Universal Symbols with firmware-native EN/RU punctuation actions on compatible RMK keyboards, with autonomous layout controls and optional Entropy Layout Sync
- Added a dedicated firmware-gated Universal picker tab with consistent `Universal` keycap labels in the picker, layout, previews, and exports
- Added firmware-native Russian `х`, `б`, `ю`, and `ъ` keys under Special > International

### Improvements

- Added native Universal Symbols layout tracking for KDE Plasma Wayland and GNOME Wayland
- Removed the legacy F13-F24 symbol transport, desktop Unicode injection, and unsupported typography, arrows, math, and currency extras; Linux IBus remains only for Text Expander
- Replaced direction arrows in Russian inversion-setting labels with text, enlarged circular encoder controls, and rendered macOS Command legends as `Cmd`
- Added native macOS Layout Indicator opacity support

### Fixes

- Prevented a Linux Bluetooth startup panic caused by retaining references to stale HID reports
- Rejected stale RMK native-action scan replies instead of aborting K:04 loading
- Prevented cached Bluetooth encoder replies from being decoded as layer names during staged loading
- Removed the long `Reading keymap…` delay on QMK-Vial keyboards when an unsupported RMK capabilities probe is echoed
- Kept the layer-name hover cursor stable while Bluetooth layers load in the background

## v0.3.1 - Public Beta

### Main Features

- Added a dedicated Trackball Settings page with localized controls for trackball availability and auto-layer timeout
- Added lossless RMK key-action loading and writing so nested shifted Mod-Tap actions remain visible, editable, undoable, mirrorable, and portable through `.entlayout` files
- Exposed shifted HID symbols in the Mod+Key, Mod-Tap, and Tap Dance key pickers whenever the target firmware can represent them
- Added Launch minimized after Launch at startup for Windows, Linux, and macOS
- Added Ctrl+GUI to Mod+Key choices for both Vial-RMK and Vial-QMK keyboards, including compact secondary and Tap Dance pickers

### Fixes

- Stabilized Linux Bluetooth Vial after fresh pairing and reconnects, including composite HOGP discovery, reliable BlueZ writes, kernel HID output routing, GATT replies, vendor-service selection, and keyboard identity preservation
- Made Choose device wait for an explicit selection instead of immediately reconnecting the only detected Bluetooth keyboard
- Retried incomplete split-battery reads promptly until both halves are available instead of keeping a single level for five minutes
- Derived firmware-managed layout geometry from the trackball availability setting and removed the duplicate Velvet control from Display Presets
- Restored right-click editing, modifier-side switching, hover tooltips, and layout hints for lossless RMK Mod-Tap actions with shifted tap symbols
- Restored separate List and Layout views in the Mod+Key and Mod-Tap secondary pickers
- Kept Bluetooth Settings stable while the shared Bluetooth HID session finishes background device loading
- Preloaded K:04 module selectors so installed encoders render as round controls on the first layout view
- Removed the redundant USB suffix from K:04 Qube device names while keeping Standalone USB and Bluetooth connections labeled
- Fixed the About Device manufacturer fallback when firmware metadata is incomplete

### Contributors

- Special thanks to @IgorArkhipov for the Clippy cleanup and encoder-visibility refactoring in #113, #114, and #116-#120

## v0.3.0 - Public Beta

### Main Features

- Added full Vial-RMK configuration over Bluetooth on Linux, Windows, and macOS, including dedicated device discovery, safe HID transport selection, clear USB/Bluetooth device labels, and automatic reconnect
- Extended Bluetooth support to Matrix Tester, Layout Indicator, Vial lock/unlock, firmware settings, and QMK Live Features through one shared, serialized HID session
- Added staged Bluetooth loading with schema caching, immediate first-layer availability, background loading for remaining layers and settings, and priority for interactive actions
- Added separate left/right battery levels below the active layer name and a firmware-gated Charge Indicator setting
- Reworked modular-device settings to follow the selected module and pointer mode, colocated encoder visibility with the relevant module, and added configurable encoder steps
- Moved layer operations into the Layout menu and made QMK Live Features availability depend on firmware capabilities instead of the currently selected OLED preset

### Fixes

- Fixed Linux Bluetooth report framing, reply routing, composite HID discovery, BlueZ/kernel transport fallback, and RMK hidraw access
- Fixed Windows and macOS Bluetooth Vial framing and HID sharing, including Windows Layout Indicator and Live Features transport
- Prevented a macOS input-source crash by keeping TIS calls on the main queue, and reduced Bluetooth Matrix Tester/Layout Indicator polling latency
- Kept Bluetooth startup, menus, hover feedback, Vial lock state, and configuration writes responsive while background HID work is active
- Fixed RMK Tap-Hold, module-setting, and Layer LED write responses, and serialized staged Config writes to prevent competing HID operations
- Fixed active-layer indication for Combo and Tap Dance actions and restored reliable Ctrl-wheel UI scaling

### Contributors

- Special thanks to @IgorArkhipov for continued testing and detailed engineering proposals around HID/settings lifecycle, module workflows, inherited-key presentation, and Vial unlock safety
- Special thanks to @ImmortalDragonm for the layer import/export, PDF export, and asynchronous native-dialog foundation that this release continues to build on

## v0.2.8 - Public Beta

### Main Features

- Added Layout/List views to key pickers, including macro, Mod-Tap, and Tap Dance dialogs (@IgorArkhipov, #49)
- Added whole-layer copy/paste, None/Inherit fill, geometry-aware mirroring, and one-step undo (@IgorArkhipov, #76)
- Added printable PDF export with one selected keyboard layer per A4 page and automatic orientation (@ImmortalDragonm, #78)
- Added nonblocking, verified module and touchpad setting saves with Saving, Saved, and Failed states, plus debouncing for rapid slider changes (@IgorArkhipov, #89, #90)
- Added reliable discovery and separate grouping for left/right split touchpad and controller settings (@IgorArkhipov, #94)
- Added automatic firmware-aware cache invalidation and a Refresh Device Data action under About Device (@IgorArkhipov, #87)
- Added runtime firmware-version reporting, Qube Live Features metadata, and separate battery levels for split devices
- Added ASCII hyphen-minus to Universal Symbols with platform-safe transport mappings (@IgorArkhipov, #71)
- Macro names and descriptions now persist across restarts, reconnects, and `.entlayout` import/export (@IgorArkhipov, #70)
- Imported layer names now persist to compatible firmware and survive reconnects (@ImmortalDragonm, #77)

### Fixes

- Prevented incomplete Combo drafts from reaching firmware, moved changed-slot saves off the UI thread, and verified saved values through readback (@IgorArkhipov, #82)
- Fixed Tap Dance and Tap-Hold writeback, stale module-setting readbacks, and mismatched QMK settings responses (@IgorArkhipov, #73, #75, #92)
- Kept Vial unlock sessions recoverable after transient polling failures (@IgorArkhipov, #68)
- Fixed multiline Text Expander output on Windows and prevented the X11 smart-input backend from starting in Wayland sessions (@IgorArkhipov, #72, #84)
- Reduced Windows idle CPU usage and hidden tray background polling (@IgorArkhipov, #69)
- Stabilized module settings grouping for mixed Left, Right, Auto Layer, and shared firmware tabs (@IgorArkhipov, #91)
- Fixed RMK layout apply timing, HID-open recovery, firmware cache refreshes, and stale Live Features media data
- Persisted app theme selection and made Universal Symbols transport safe for Windows shortcuts
- Improved layer editing: dimmed inherited `KC_TRNS` legends, fixed wheel navigation, limited bulk None/Inherit to the selected layer, and kept the layout visible during background operations
- Completed Russian localization for module settings and Bluetooth sleep timeouts
- Restored the standard scrollbar gutter on the Modules page
- Made the device-selection state compact, content-sized, and scrollable only when more than six devices are available
- Kept native import/export dialogs in front of Entropy without blocking the UI, and unified asynchronous PNG/SVG/PDF export handling (@ImmortalDragonm, #80)

### Contributors

- Special thanks to @IgorArkhipov for 25 merged PRs across key pickers, layer tools, HID reliability, module settings, platform input, and lifecycle fixes
- Special thanks to @ImmortalDragonm for persistent imported layer names, PDF export, and nonblocking native file dialogs (#77, #78, #80)

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
- Linux IBus helper backend for Wayland input-method workflows

### Distribution

- Linux x86_64 AppImage
- Windows x86_64 portable ZIP
- SHA-256 checksum file

### Documentation

- README screenshot gallery for Key Picker, Matrix Tester, and Text Expander
- README now states the Vial-QMK and Vial-RMK firmware scope near the top
- README documents Linux IBus installation and required system dependencies

### Fixes

- Linux setup actions can run bundled IBus and udev scripts from packaged builds
- Encoder visibility now respects Vial layout-display conditions, so Phenom encoder press keys hide together with their encoder controls
- Segmented controls now shrink long localized labels to stay inside their button bounds
- Windows now keeps Entropy single-instance: repeated launches restore the existing tray instance instead of starting a second app

### Beta Notes

- Windows builds are unsigned during beta
- Firmware-gated features appear only when the connected device exposes the required Vial/QMK settings
- Browser-only configuration and mobile platforms are not supported in this beta

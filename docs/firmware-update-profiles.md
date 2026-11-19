# Adding an RMK keyboard to the firmware updater

Rynk support enables communication with a keyboard. In-app flashing additionally
requires a reviewed entry in `src/firmware_flash/profiles.rs`. The registry
covers all 17 native imagesets published in Ergohaven RMK 0.3.0.

| Firmware families | Update order |
| --- | --- |
| Phenom, Phenom Mini, Phenom Micro | Other half, then the original USB half |
| K:04, Mini and Micro standalone | Right half, then left primary |
| K:03, Imperial44 and OP36 standalone | Right half, then left primary |
| Velvet standalone | Left half, then right primary |
| K:04, Mini, Micro, K:03, Imperial44, OP36 and Velvet with Qube | Left half, right half, then Qube |

K:03, Imperial44, OP36 and Velvet share a USB product ID and Rynk UID between
their standalone and Qube builds. Entropy requires the firmware's explicit
`entropy.firmwareUpdate.asset` and checks it again on directly connected
components before bootloader entry and on reconnect. SM56 and SM62 have no
packages in this native RMK series.

The updater has four boundaries:

- `profiles.rs` owns USB product IDs, release archive and image names, component
  labels, application memory maps, connection notes and transfer order.
- `packages.rs` validates the exact ZIP members, build provenance, SHA-256 sums
  and each UF2 image against its component's memory map.
- `firmware_flash.rs` owns download verification, Rynk bootloader entry, binding
  to the original physical USB port, writing and fresh version verification.
- `platform.rs` enumerates USB ports and mounted bootloader volumes. The UI uses
  the profile's roles and labels to guide the same preparation and retry flow.

For a new keyboard using an existing supported UF2 bootloader:

1. Add its product ID and exact release package prefix to `PROFILES`. The
   registry also supplies About-device eligibility and the USB allowlist on every
   supported OS. Add the corresponding download catalog entry in `app_update.rs`
   if needed. When two profiles share a product ID, the Rynk definition must
   identify the archive explicitly.
2. Define its ordered `Part` values. Each specifies a role, Rynk firmware target,
   bilingual label, image filename suffix and `FlashLayout`. Put peripherals
   first and the default USB primary or dongle last. For a board where either
   half can become primary, enable `either_half_primary` so the directly
   connected half reported by revision 3 is updated last. A single-piece board
   uses one `Standalone` firmware target. For maintenance-capable halves, map
   the local unlock coordinates to the factory keys displayed in the guide.
3. Verify the chosen layout against the board's linker configuration and
   bootloader. Existing limits are specific application intervals, not defaults
   for every RP2040 or nRF52840 board. Add a layout for a different interval.
   New MCUs or bootloaders also require explicit UF2 family, bootloader identity
   and entry/recovery support; Rynk alone does not supply these.
4. Match the release convention: `<archive>-<version>.zip`, containing
   `<output>_<suffix>.uf2` for each part plus `BUILD_COMMIT.txt` and
   `SHA256SUMS.txt`. A different packaging convention needs an explicit parser
   change; do not relax the existing exact-member and checksum checks.
5. Add package and topology fixtures, cross-model rejection checks, and exercise
   the real published archive. Verify guidance in both languages. Synthetic
   single-piece RP2040 and nRF52840 fixtures demonstrate extension without
   registering a real unsupported keyboard.

Runtime flashing requires Entropy 0.6.0+, stable native RMK 0.3.0+, and USB.
Entropy accepts Ergohaven Rynk revisions 2 and 3. Revision 3 components report
their image target and can enter update mode after physical-key unlock. The
guide connects directly to maintenance-only peripherals, without loading the
keyboard editor. A legacy peripheral still needs BOOT or Reset for its first
installation. Entropy verifies the target and version if the new image exposes
them after restarting. A transfer without version readback remains unverified.

Source tests and CI do not establish hardware support. Before enabling a real
model for release, validate its bootloader transition, original-port mapping,
transfer, recovery and available version readback on hardware.

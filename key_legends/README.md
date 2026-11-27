# Built-in key legends

These TOML files describe **displayed keycap legends in Entropy**, not the
interface translation, keyboard geometry, firmware keymap or host input method.
English is the built-in base. The selected profile supplies additional legends;
positions absent from the profile retain their existing English/action captions.

Only repository files are supported. There is no user-directory lookup, file
import, plugin, download or runtime TOML loader. New profiles enter the product
through a reviewed pull request and the next application build.

## Add a profile

1. Copy `ru.toml` to `<stable-id>.toml` in this directory
2. Set a unique concrete `id` matching the filename and `schema_version = 2`
3. For a new secondary language/layout, set `family_id`, both family names and
   `short_name`; its first profile must have `default_variant = true`
4. For an additional variant of an existing family, retain that family's ID,
   names and short name, choose a unique `variant_id` and both variant names,
   and use `default_variant = false` unless the PR deliberately changes the default
5. Remove the copied `[legacy]` section; it is only for migrating historical
   saved selections, not for naming new profiles
6. Replace the key entries with the intended variant's displayed legends
7. Describe the specific layout/OS variant and supply screenshots of App Settings,
   the canvas, picker and number-row symbols in the pull request

The build script discovers the TOML files automatically in stable ID order. No
new Rust enum variant, UI option list or per-language renderer is needed. Include
only actual profiles in a PR; examples and documentation do not need TOML files.

## Metadata

`schema_version = 2`, `id`, `family_id`, `variant_id`, `variant_name_en`,
`variant_name_ru`, `default_variant`, `name_en`, `name_ru`, `short_name` and
`keys` are required.

App Settings has three rows: the fixed English **Primary layout**, a **Secondary
layout** family (`None`, Russian, and future contributed families), and a **Layout
variant** filtered to that family. With no secondary family, the variant is None
and disabled. The current Russian profile is Standard; Typewriter, Engineering
and other variants appear only after their actual TOML data is contributed.

A family groups concrete profiles such as `ru` and a future `ru-typewriter`.
Exactly one profile per family is its default, and variant IDs must be unique
within the family. Family names and `short_name` must agree across its profiles.
Selecting the same family preserves its variant; switching families selects the
new family's default. Switching a variant preserves legend order.

The existing concrete `id` plus order remain the only persisted selection, so
old Russian settings still resolve to `ru` / Standard. Family and variant UI
values are derived from that ID, not stored as competing settings. New variant
IDs are not inferred from translated names or filename prefixes.
`id` uses lowercase ASCII letters, digits, `-` and `_`, begins with a letter and
is at most 64 bytes. `en` and `english` are reserved for the English base.
Use a permanent variant-specific ID when two operating systems or input variants
have different legends. Do not silently change an existing ID to mean another
layout.

`name_en` and `name_ru` are the profile name used inside the localized
`English + …` choice. The Russian name is written in sentence case, for example
`русская`; image-export labels capitalize its first letter. `short_name` is used
in the existing legend-order menu, for example `Legends: EN / RU`. These names do
not translate the rest of the application.

The optional `[legacy]` table names old serialized settings:
`english_first` and `secondary_first`. Aliases must be globally unique and must
not collide with profile IDs. Existing `english`, `russian` and
`russian_primary` selections migrate to the same profile/order. New saves use a
stable `layout` ID plus independent `order`, never a catalog index. An unavailable
saved profile falls back to English with a diagnostic, without rejecting the
other application settings.

## Key entries

`[keys]` maps canonical **Keyboard/Keypad HID usage IDs**, not key positions or
letters printed on a particular board. Write each key exactly as four uppercase
hex digits with a lowercase prefix, for example `"0x0004"` for the A position.
Profiles cannot redefine firmware actions, modifier wrappers or keycode values.

Each entry has `kind`, `normal` and an optional `shift` string:

- `pair`: an additional legend paired with the existing English caption
- `symbol`: the existing physical-style two-row symbol presentation; `normal`
  appears alongside the English Shift symbol, above the English base symbol
- `number`: a number-row base legend; `shift` supplies the additional Shift
  legend when shifted number symbols are shown

`symbol` and `number` require `shift`. Number style is limited to the number row;
symbol style requires a known English symbol position. `shift` also describes
the existing explicit shifted-symbol keycode caption. These are **display
strings**, not a table of characters actually emitted by the OS. The migrated
Russian file intentionally preserves the existing captions and visual ordering.

Strings can contain several Unicode codepoints, including combining sequences;
they are not limited to one Rust `char`. They must be nonempty, have no control
characters/newlines and contain at most 64 characters. Formatting, line breaks,
legend order and duplicate suppression belong to the shared renderer, not TOML.
The existing font stack must cover the proposed script; a data file alone does
not add a font or implement an IME.

Invalid TOML, unknown fields/schema versions, malformed IDs/usages, collisions,
unsupported entry kinds and invalid strings fail the normal build with a
profile-specific diagnostic. Profiles are limited to 256 entries and 256 KiB.

## Shared owners

- `src/key_legends_build.rs`: TOML discovery, validation and generated catalog
- `src/key_legends.rs`: stable selection, legacy settings and shared formatting
- `src/key_legends/english.rs`: the common English symbol positions
- `src/keycode.rs`: existing keycode/action caption path, delegating legends

Settings, the device menu, picker/search, number-row helpers and image export
consume this same catalog. Keep actual host-output/typing-trainer models and the
SM62 firmware's separate display-lettering setting independent of this catalog.

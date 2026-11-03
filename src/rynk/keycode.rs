//! Rynk key actions next to the u16 keycodes the key editor works in.
//!
//! A keycode means on a Rynk board what it meant on a Vial RMK board: the two
//! conversions below are the firmware's own (`rmk/src/host/via/keycode_convert.rs`
//! at the pinned `rmk-types-rynk` revision). An action is shown as a keycode only
//! when that keycode converts back to the very same action, profile index
//! included; every other action stays a `KeyBinding::Rynk`, so reading a key and
//! writing it back never changes it.

use std::fmt;

use rmk_types_rynk::action::{Action, KeyAction, KeyboardAction};
use rmk_types_rynk::keycode::{HidKeyCode, KeyCode, SpecialKey};
use rmk_types_rynk::modifier::ModifierCombination;

use crate::keyboard::KeyBinding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyBindingError {
    /// The firmware has no action for this keycode: over Vial it stored `No`.
    UnsupportedKeycode(u16),
    /// Native v1 actions belong to Vial boards; imports upgrade them first.
    NativeV1,
}

impl fmt::Display for KeyBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedKeycode(keycode) => {
                write!(f, "keycode {keycode:#06x} has no RMK action")
            }
            Self::NativeV1 => f.write_str("native v1 action on a Rynk board"),
        }
    }
}

impl std::error::Error for KeyBindingError {}

/// The binding the key editor shows for an action read from a Rynk board.
pub(crate) fn key_binding(action: KeyAction) -> KeyBinding {
    exact_vial_keycode(action).map_or(KeyBinding::Rynk(action), KeyBinding::Vial)
}

/// What a Rynk board takes for a binding the editor built: the native v1
/// actions the picker and other boards' layers bring cross as their upgrade,
/// held as an action read from the board would be. `None` when the upgrade
/// would change the action.
pub(crate) fn rynk_board_binding(binding: KeyBinding) -> Option<KeyBinding> {
    match binding {
        KeyBinding::Rmk(action) => {
            let upgrade = super::native_v1::upgrade_native_v1_key_action(action);
            upgrade
                .losses
                .is_empty()
                .then(|| key_binding(upgrade.action))
        }
        KeyBinding::VialTapHold { .. } => None,
        other => Some(other),
    }
}

/// What a Vial board takes for a binding a Rynk board's layer brought: the
/// action's keycode, or with `native_v1` the native v1 action that upgrades
/// back to exactly it. `None` when neither holds the action.
pub(crate) fn vial_board_binding(binding: KeyBinding, native_v1: bool) -> Option<KeyBinding> {
    let KeyBinding::Rynk(action) = binding else {
        return Some(binding);
    };
    if let Some(keycode) = exact_vial_keycode(action) {
        return Some(KeyBinding::Vial(keycode));
    }
    let v1 = super::native_v1::v1_label_action(action).filter(|_| native_v1)?;
    let back = super::native_v1::upgrade_native_v1_key_action(v1);
    (back.losses.is_empty() && identical(back.action, action)).then_some(KeyBinding::Rmk(v1))
}

/// The action a binding writes to a Rynk board.
pub(crate) fn binding_action(binding: KeyBinding) -> Result<KeyAction, KeyBindingError> {
    match binding {
        KeyBinding::Vial(keycode) => {
            vial_keycode_action(keycode).ok_or(KeyBindingError::UnsupportedKeycode(keycode))
        }
        KeyBinding::Rynk(action) => Ok(action),
        KeyBinding::Rmk(_) | KeyBinding::VialTapHold { .. } => Err(KeyBindingError::NativeV1),
    }
}

/// The action `keycode` writes, `None` when the firmware has none for it. The
/// action may read back as another keycode of the same meaning: RS Enter reads
/// back as `RSFT_T(KC_ENT)`, as it does over Vial.
pub(crate) fn vial_keycode_action(keycode: u16) -> Option<KeyAction> {
    let action = from_vial_keycode(keycode);
    match exact_vial_keycode(action)? {
        // A keycode the firmware has no action for decodes to an empty key,
        // a hole in the HID page to an HID no-key.
        0 if keycode != 0 => None,
        _ => Some(action),
    }
}

/// The keycode that holds exactly `action`. An empty key is `KC_NO` in each
/// shape the firmware stores it in, as its Vial path reads them all.
pub(crate) fn exact_vial_keycode(action: KeyAction) -> Option<u16> {
    if matches!(
        action,
        KeyAction::No
            | KeyAction::Single(Action::No)
            | KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::No)))
    ) {
        return Some(0);
    }
    let keycode = to_vial_keycode(action);
    identical(from_vial_keycode(keycode), action).then_some(keycode)
}

/// Equal down to the tap-hold profile index, which `KeyAction`'s `PartialEq`
/// leaves out.
pub(crate) fn identical(a: KeyAction, b: KeyAction) -> bool {
    let profile = |action| match action {
        KeyAction::TapHold(_, _, profile) => Some(profile),
        _ => None,
    };
    a == b && profile(a) == profile(b)
}

/// The firmware's `to_via_keycode`: 0 for an action without a keycode.
pub(crate) fn to_vial_keycode(key_action: KeyAction) -> u16 {
    match key_action {
        KeyAction::No => 0x0000,
        KeyAction::Transparent => 0x0001,
        KeyAction::Single(a) => match a {
            Action::Key(k) => match k {
                KeyCode::Hid(hid_keycode) => hid_keycode as u16,
                KeyCode::Consumer(consumer_key) => {
                    consumer_key.to_hid_keycode().map_or(0, |hid| hid as u16)
                }
                KeyCode::SystemControl(system_key) => {
                    system_key.to_hid_keycode().map_or(0, |hid| hid as u16)
                }
                _ => 0,
            },
            Action::KeyWithModifier(k, m) => ((m.into_packed_bits() as u16) << 8) | k as u16,
            Action::LayerToggleOnly(l) => 0x5200 | l as u16,
            Action::LayerOn(l) => 0x5220 | l as u16,
            Action::DefaultLayer(l) => 0x5240 | l as u16,
            Action::PersistentDefaultLayer(l) => 0x52E0 | l as u16,
            Action::LayerToggle(l) => 0x5260 | l as u16,
            Action::TriLayerLower => 0x7c77,
            Action::TriLayerUpper => 0x7c78,
            Action::TriggerMacro(idx) => 0x7700 + (idx as u16),
            Action::OneShotLayer(l) => {
                if l < 32 {
                    0x5280 | l as u16
                } else {
                    0x0000
                }
            }
            Action::OneShotModifier(m) => 0x52A0 | m.into_packed_bits() as u16,
            Action::LayerOnWithModifier(l, m) => {
                if l < 16 {
                    0x5000 | ((l as u16) << 5) | ((m.into_packed_bits() & 0b11111) as u16)
                } else {
                    0
                }
            }
            Action::KeyboardControl(c) => match c {
                KeyboardAction::Bootloader => 0x7c00,
                KeyboardAction::Reboot => 0x7c01,
                KeyboardAction::ClearEeprom => 0x7c03,
                KeyboardAction::ComboOn => 0x7c50,
                KeyboardAction::ComboOff => 0x7c51,
                KeyboardAction::ComboToggle => 0x7c52,
                KeyboardAction::CapsWordToggle => 0x7c73,
                _ => 0,
            },
            Action::Special(special_key) => match special_key {
                SpecialKey::GraveEscape => 0x7c16,
                SpecialKey::Repeat => 0x7c79,
                _ => 0,
            },
            Action::User(id) if id < 64 => id as u16 | 0x7E00,
            _ => 0,
        },
        KeyAction::Tap(_) => 0,
        KeyAction::TapHold(tap, hold, _) => match hold {
            Action::LayerOn(l) if tap == Action::LayerToggle(l) && l < 32 => 0x52C0 | l as u16,
            Action::LayerOn(l) => {
                if l > 16 {
                    0
                } else {
                    let keycode = match tap {
                        Action::Key(KeyCode::Hid(k)) => k as u16,
                        _ => 0,
                    };
                    0x4000 | ((l as u16) << 8) | keycode
                }
            }
            Action::Modifier(m) => match tap {
                Action::KeyWithModifier(k, sm) if sm == ModifierCombination::LSHIFT => match k {
                    HidKeyCode::Kc9 if m == ModifierCombination::LCTRL => 0x7C18,
                    HidKeyCode::Kc0 if m == ModifierCombination::RCTRL => 0x7C19,
                    HidKeyCode::Kc9 if m == ModifierCombination::LSHIFT => 0x7C1A,
                    HidKeyCode::Kc0 if m == ModifierCombination::RSHIFT => 0x7C1B,
                    HidKeyCode::Kc9 if m == ModifierCombination::LALT => 0x7C1C,
                    HidKeyCode::Kc0 if m == ModifierCombination::RALT => 0x7C1D,
                    _ => 0,
                },
                Action::Key(KeyCode::Hid(k)) => {
                    0x2000 | ((m.into_packed_bits() as u16) << 8) | (k as u16)
                }
                _ => 0,
            },
            _ => 0x0000,
        },
        KeyAction::Morse(index) => 0x5700 | (index as u16),
        _ => 0,
    }
}

/// The firmware's `from_via_keycode`: `No` for a keycode without an action.
pub(crate) fn from_vial_keycode(via_keycode: u16) -> KeyAction {
    let space_cadet = |key, hold| {
        KeyAction::TapHold(
            Action::KeyWithModifier(key, ModifierCombination::LSHIFT),
            Action::Modifier(hold),
            u8::MAX,
        )
    };
    match via_keycode {
        0x0000 => KeyAction::No,
        0x0001 => KeyAction::Transparent,
        0x0002..=0x00FF => KeyAction::Single(Action::Key(KeyCode::Hid((via_keycode as u8).into()))),
        0x0100..=0x1FFF => {
            let modifier = ModifierCombination::from_packed_bits((via_keycode >> 8) as u8);
            KeyAction::Single(Action::KeyWithModifier(
                (via_keycode as u8).into(),
                modifier,
            ))
        }
        0x2000..=0x3FFF => {
            let keycode = KeyCode::Hid((via_keycode as u8).into());
            let modifier =
                ModifierCombination::from_packed_bits(((via_keycode >> 8) & 0b11111) as u8);
            KeyAction::TapHold(Action::Key(keycode), Action::Modifier(modifier), u8::MAX)
        }
        0x4000..=0x4FFF => {
            let layer = (via_keycode >> 8) & 0xF;
            let keycode = KeyCode::Hid((via_keycode as u8).into());
            KeyAction::TapHold(Action::Key(keycode), Action::LayerOn(layer as u8), u8::MAX)
        }
        0x5000..=0x51FF => {
            let layer = (via_keycode >> 5) & 0xF;
            let modifier = ModifierCombination::from_packed_bits((via_keycode & 0b11111) as u8);
            KeyAction::Single(Action::LayerOnWithModifier(layer as u8, modifier))
        }
        0x5200..=0x521F => KeyAction::Single(Action::LayerToggleOnly(via_keycode as u8 & 0x1F)),
        0x5220..=0x523F => KeyAction::Single(Action::LayerOn(via_keycode as u8 & 0x1F)),
        0x5240..=0x525F => KeyAction::Single(Action::DefaultLayer(via_keycode as u8 & 0x1F)),
        0x5260..=0x527F => KeyAction::Single(Action::LayerToggle(via_keycode as u8 & 0x1F)),
        0x5280..=0x529F => KeyAction::Single(Action::OneShotLayer(via_keycode as u8 & 0x1F)),
        0x52A0..=0x52BF => {
            let m = ModifierCombination::from_packed_bits((via_keycode & 0x1F) as u8);
            KeyAction::Single(Action::OneShotModifier(m))
        }
        0x52C0..=0x52DF => {
            let layer = via_keycode as u8 & 0x1F;
            KeyAction::TapHold(Action::LayerToggle(layer), Action::LayerOn(layer), u8::MAX)
        }
        0x52E0..=0x52FF => {
            KeyAction::Single(Action::PersistentDefaultLayer(via_keycode as u8 & 0x1F))
        }
        0x5700..=0x57FF => KeyAction::Morse((via_keycode & 0xFF) as u8),
        0x7700..=0x77FF => KeyAction::Single(Action::TriggerMacro(via_keycode as u8)),
        0x7C00 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::Bootloader)),
        0x7C01 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::Reboot)),
        0x7C03 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::ClearEeprom)),
        0x7C50 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::ComboOn)),
        0x7C51 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::ComboOff)),
        0x7C52 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::ComboToggle)),
        0x7C16 => KeyAction::Single(Action::Special(SpecialKey::GraveEscape)),
        0x7C73 => KeyAction::Single(Action::KeyboardControl(KeyboardAction::CapsWordToggle)),
        0x7C77 => KeyAction::Single(Action::TriLayerLower),
        0x7C78 => KeyAction::Single(Action::TriLayerUpper),
        0x7C79 => KeyAction::Single(Action::Special(SpecialKey::Repeat)),
        0x7C18 => space_cadet(HidKeyCode::Kc9, ModifierCombination::LCTRL),
        0x7C19 => space_cadet(HidKeyCode::Kc0, ModifierCombination::RCTRL),
        0x7C1A => space_cadet(HidKeyCode::Kc9, ModifierCombination::LSHIFT),
        0x7C1B => space_cadet(HidKeyCode::Kc0, ModifierCombination::RSHIFT),
        0x7C1C => space_cadet(HidKeyCode::Kc9, ModifierCombination::LALT),
        0x7C1D => space_cadet(HidKeyCode::Kc0, ModifierCombination::RALT),
        // RS Enter: decode only, re-encodes as the equivalent RShift mod-tap.
        0x7C1E => KeyAction::TapHold(
            Action::Key(KeyCode::Hid(HidKeyCode::Enter)),
            Action::Modifier(ModifierCombination::RSHIFT),
            u8::MAX,
        ),
        0x7E00..=0x7E3F => KeyAction::Single(Action::User(via_keycode as u8 & 0x3F)),
        _ => KeyAction::No,
    }
}

/// Moves left-hand modifiers to the right hand and back, as
/// `rmk_native::toggle_handed_key_action` does for native v1 actions.
pub(crate) fn toggle_handed_key_action(action: KeyAction) -> KeyAction {
    fn swap_modifiers(modifiers: ModifierCombination) -> ModifierCombination {
        ModifierCombination::from_bits(modifiers.into_bits().rotate_right(4))
    }

    fn swap_action(action: Action) -> Action {
        match action {
            Action::Modifier(modifiers) => Action::Modifier(swap_modifiers(modifiers)),
            Action::KeyWithModifier(key, modifiers) => {
                Action::KeyWithModifier(key, swap_modifiers(modifiers))
            }
            other => other,
        }
    }

    match action {
        KeyAction::Single(action) => KeyAction::Single(swap_action(action)),
        KeyAction::Tap(action) => KeyAction::Tap(swap_action(action)),
        KeyAction::TapHold(tap, hold, profile) => {
            KeyAction::TapHold(swap_action(tap), swap_action(hold), profile)
        }
        other => other,
    }
}

#[cfg(test)]
pub(crate) mod catalog {
    //! Every action a key can hold, for the round-trip tests here and in the
    //! other converters.

    use rmk_types_rynk::action::{Action, KeyAction, KeyboardAction, LightAction};
    use rmk_types_rynk::keycode::{ConsumerKey, HidKeyCode, KeyCode, SpecialKey, SystemControlKey};
    use rmk_types_rynk::modifier::ModifierCombination;
    use serde::de::DeserializeOwned;

    /// Every variant of a fieldless enum, by postcard variant index.
    pub(crate) fn variants<T: DeserializeOwned>() -> Vec<T> {
        (0..1024u32)
            .filter_map(|index| {
                let bytes = postcard::to_slice(&index, &mut [0u8; 5]).ok()?.to_vec();
                postcard::from_bytes(&bytes).ok()
            })
            .collect()
    }

    pub(crate) fn modifiers() -> impl Iterator<Item = ModifierCombination> {
        (0..=u8::MAX).map(ModifierCombination::from_bits)
    }

    /// Every `Action` except steno keys, with every value of its fields.
    pub(crate) fn actions() -> Vec<Action> {
        let hid = variants::<HidKeyCode>();
        let mut actions = vec![Action::No, Action::TriLayerLower, Action::TriLayerUpper];
        actions.extend(hid.iter().map(|&k| Action::Key(KeyCode::Hid(k))));
        actions.extend(
            variants::<ConsumerKey>()
                .into_iter()
                .map(|k| Action::Key(KeyCode::Consumer(k))),
        );
        actions.extend(
            variants::<SystemControlKey>()
                .into_iter()
                .map(|k| Action::Key(KeyCode::SystemControl(k))),
        );
        actions.extend(modifiers().map(Action::Modifier));
        for &key in &hid {
            actions.extend(modifiers().map(|m| Action::KeyWithModifier(key, m)));
        }
        actions.extend(hid.iter().map(|&k| Action::OneShotKey(k)));
        for layer in 0..=u8::MAX {
            actions.extend([
                Action::LayerOn(layer),
                Action::LayerOff(layer),
                Action::LayerToggle(layer),
                Action::DefaultLayer(layer),
                Action::LayerToggleOnly(layer),
                Action::TriggerMacro(layer),
                Action::OneShotLayer(layer),
                Action::User(layer),
                Action::PersistentDefaultLayer(layer),
            ]);
        }
        for layer in 0..40 {
            actions.extend(modifiers().map(|m| Action::LayerOnWithModifier(layer, m)));
        }
        actions.extend(modifiers().map(Action::OneShotModifier));
        actions.extend(variants::<LightAction>().into_iter().map(Action::Light));
        actions.extend(
            variants::<KeyboardAction>()
                .into_iter()
                .map(Action::KeyboardControl),
        );
        actions.extend(variants::<SpecialKey>().into_iter().map(Action::Special));
        actions
    }

    /// Every single, tap and morse key, and tap-hold keys over the shapes
    /// keycodes spell (mod-tap, layer-tap, layer-tap-toggle, space cadet) plus
    /// a spread of others, each at three profile indices.
    pub(crate) fn key_actions() -> Vec<KeyAction> {
        let actions = actions();
        let mut keys = vec![KeyAction::No, KeyAction::Transparent];
        keys.extend(actions.iter().map(|&a| KeyAction::Single(a)));
        keys.extend(actions.iter().map(|&a| KeyAction::Tap(a)));
        keys.extend((0..=u8::MAX).map(KeyAction::Morse));
        let hid = variants::<HidKeyCode>();
        let mut holds: Vec<Action> = modifiers().map(Action::Modifier).collect();
        holds.extend((0..40).map(Action::LayerOn));
        holds.extend([Action::LayerToggle(1), Action::User(3), Action::No]);
        let mut taps: Vec<Action> = hid.iter().map(|&k| Action::Key(KeyCode::Hid(k))).collect();
        taps.extend(
            [HidKeyCode::Kc9, HidKeyCode::Kc0, HidKeyCode::A]
                .into_iter()
                .flat_map(|k| modifiers().map(move |m| Action::KeyWithModifier(k, m))),
        );
        taps.extend((0..40).map(Action::LayerToggle));
        taps.extend([Action::No, Action::User(70), Action::TriggerMacro(2)]);
        for profile in [0, 7, u8::MAX] {
            for &tap in &taps {
                for &hold in &holds {
                    keys.push(KeyAction::TapHold(tap, hold, profile));
                }
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_action_reads_back_and_writes_unchanged() {
        let mut native = 0;
        for action in catalog::key_actions() {
            let binding = key_binding(action);
            let written = binding_action(binding).unwrap_or_else(|error| {
                panic!("{action:?} shown as {binding:?} does not write back: {error}")
            });
            // An empty key in any shape is KC_NO and writes back as `No`.
            let empty = binding == KeyBinding::Vial(0) && written == KeyAction::No;
            assert!(
                identical(written, action) || empty,
                "{action:?} -> {binding:?} -> {written:?}"
            );
            native += usize::from(matches!(binding, KeyBinding::Rynk(_)));
        }
        assert!(native > 0);
    }

    #[test]
    fn every_keycode_writes_an_action_that_reads_back_as_itself_or_an_alias() {
        let mut supported = 0;
        for keycode in 0..=u16::MAX {
            let Some(action) = vial_keycode_action(keycode) else {
                assert!(binding_action(KeyBinding::Vial(keycode)).is_err());
                continue;
            };
            supported += 1;
            let KeyBinding::Vial(shown) = key_binding(action) else {
                panic!("{keycode:#06x} writes {action:?}, which has no keycode");
            };
            assert!(
                identical(from_vial_keycode(shown), action),
                "{keycode:#06x}"
            );
            assert_eq!(vial_keycode_action(shown), Some(action), "{keycode:#06x}");
            if shown != keycode {
                assert!(
                    firmware_alias(keycode),
                    "{keycode:#06x} reads back as {shown:#06x}"
                );
            }
        }
        assert!(supported > 10_000, "{supported}");
    }

    /// Keycodes the firmware decodes to the action of another keycode, so they
    /// read back as that one over Vial too.
    fn firmware_alias(keycode: u16) -> bool {
        // RS Enter is RSFT_T(KC_ENT).
        let rs_enter = keycode == 0x7C1E;
        // The right-hand flag without a modifier is no modifier.
        let packed = match keycode {
            0x2000..=0x3FFF => (keycode >> 8) & 0x1F,
            0x5000..=0x51FF | 0x52A0..=0x52BF => keycode & 0x1F,
            _ => 0,
        };
        // A byte the HID page leaves undefined is KC_NO.
        let undefined_key = matches!(keycode, 0x0100..=0x4FFF)
            && keycode & 0xFF != 0
            && HidKeyCode::from(keycode as u8) == HidKeyCode::No;
        rs_enter || packed == 0x10 || undefined_key
    }

    #[test]
    fn keycodes_without_an_action_are_refused() {
        for keycode in [
            0x7000, // QMK function
            0x7800, // backlight / RGB
            0x7C02, // QK_DEBUG_TOGGLE
            0x7E40, // QK_KB_64: user keys past 63 are native only
            0x00C3, // a gap in the HID page
            0xFFFF,
        ] {
            assert_eq!(vial_keycode_action(keycode), None, "{keycode:#06x}");
            assert_eq!(
                binding_action(KeyBinding::Vial(keycode)),
                Err(KeyBindingError::UnsupportedKeycode(keycode))
            );
        }
        assert_eq!(vial_keycode_action(0), Some(KeyAction::No));
        assert_eq!(
            binding_action(KeyBinding::Rmk(rmk_types::action::KeyAction::No)),
            Err(KeyBindingError::NativeV1)
        );
    }

    #[test]
    fn every_empty_key_is_kc_no() {
        // The firmware stores `No` from keyboard.toml and an unset encoder as
        // an HID no-key; its Vial path reads all three as KC_NO.
        for empty in [
            KeyAction::No,
            KeyAction::Single(Action::No),
            KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::No))),
        ] {
            assert_eq!(exact_vial_keycode(empty), Some(0), "{empty:?}");
            assert_eq!(key_binding(empty), KeyBinding::Vial(0), "{empty:?}");
        }
    }

    #[test]
    fn actions_keycodes_cannot_hold_stay_native() {
        let a = Action::Key(KeyCode::Hid(HidKeyCode::A));
        for action in [
            // Profile index other than the default the keycodes decode to.
            KeyAction::TapHold(a, Action::Modifier(ModifierCombination::LCTRL), 0),
            KeyAction::Tap(a),
            KeyAction::Single(Action::User(64)),
            // Media key on the consumer page: its keycode decodes to the HID key.
            KeyAction::Single(Action::Key(KeyCode::Consumer(
                rmk_types_rynk::keycode::ConsumerKey::Mute,
            ))),
            // Both hands at once: the packed modifier keeps one.
            KeyAction::Single(Action::KeyWithModifier(
                HidKeyCode::A,
                ModifierCombination::LCTRL | ModifierCombination::RALT,
            )),
            // Layer 16 would spill into LM's range.
            KeyAction::TapHold(a, Action::LayerOn(16), u8::MAX),
            KeyAction::Single(Action::KeyWithModifier(
                HidKeyCode::A,
                ModifierCombination::new(),
            )),
        ] {
            assert_eq!(key_binding(action), KeyBinding::Rynk(action), "{action:?}");
        }
    }

    /// The firmware's own test vectors for both conversions.
    #[test]
    fn firmware_vectors() {
        let a = Action::Key(KeyCode::Hid(HidKeyCode::A));
        let b = Action::Key(KeyCode::Hid(HidKeyCode::B));
        let single = KeyAction::Single;
        let mods = ModifierCombination::new_from;
        let tap_hold = |tap, hold| KeyAction::TapHold(tap, hold, u8::MAX);
        for (keycode, action) in [
            (0x0004, single(a)),
            (
                0x00E5,
                single(Action::Key(KeyCode::Hid(HidKeyCode::RShift))),
            ),
            (0x7E00, single(Action::User(0))),
            (0x7E10, single(Action::User(16))),
            (0x7E3F, single(Action::User(63))),
            (
                0x7C03,
                single(Action::KeyboardControl(KeyboardAction::ClearEeprom)),
            ),
            (0x5223, single(Action::LayerOn(3))),
            (0x5283, single(Action::OneShotLayer(3))),
            (
                0x52B1,
                single(Action::OneShotModifier(mods(
                    true, false, false, false, true,
                ))),
            ),
            (0x5243, single(Action::DefaultLayer(3))),
            (0x52E0, single(Action::PersistentDefaultLayer(0))),
            (0x52EF, single(Action::PersistentDefaultLayer(15))),
            (
                0x0104,
                single(Action::KeyWithModifier(
                    HidKeyCode::A,
                    mods(false, false, false, false, true),
                )),
            ),
            (
                0x1104,
                single(Action::KeyWithModifier(
                    HidKeyCode::A,
                    mods(true, false, false, false, true),
                )),
            ),
            (
                0x0704,
                single(Action::KeyWithModifier(
                    HidKeyCode::A,
                    mods(false, false, true, true, true),
                )),
            ),
            (
                0x0F04,
                single(Action::KeyWithModifier(
                    HidKeyCode::A,
                    mods(false, true, true, true, true),
                )),
            ),
            (
                0x021F,
                single(Action::KeyWithModifier(
                    HidKeyCode::Kc2,
                    ModifierCombination::LSHIFT,
                )),
            ),
            (0x4004, tap_hold(a, Action::LayerOn(0))),
            (0x4304, tap_hold(a, Action::LayerOn(3))),
            (
                0x2604,
                tap_hold(a, Action::Modifier(mods(false, false, true, true, false))),
            ),
            (
                0x3D05,
                tap_hold(b, Action::Modifier(mods(true, true, true, false, true))),
            ),
            (
                0x2F04,
                tap_hold(a, Action::Modifier(mods(false, true, true, true, true))),
            ),
            (
                0x2705,
                tap_hold(b, Action::Modifier(mods(false, false, true, true, true))),
            ),
            (
                0x5022,
                single(Action::LayerOnWithModifier(1, ModifierCombination::LSHIFT)),
            ),
            (
                0x5039,
                single(Action::LayerOnWithModifier(
                    1,
                    ModifierCombination::new()
                        .with_right_gui(true)
                        .with_right_ctrl(true),
                )),
            ),
            (
                0x7C51,
                single(Action::KeyboardControl(KeyboardAction::ComboOff)),
            ),
            (0x7C16, single(Action::Special(SpecialKey::GraveEscape))),
            (0x7C79, single(Action::Special(SpecialKey::Repeat))),
            (0x52C1, tap_hold(Action::LayerToggle(1), Action::LayerOn(1))),
            (
                0x52DF,
                tap_hold(Action::LayerToggle(31), Action::LayerOn(31)),
            ),
            (0x5700, KeyAction::Morse(0)),
            (0x5705, KeyAction::Morse(5)),
            (0x57FF, KeyAction::Morse(255)),
        ] {
            assert!(
                identical(from_vial_keycode(keycode), action),
                "{keycode:#06x}"
            );
            assert_eq!(to_vial_keycode(action), keycode, "{action:?}");
        }
        let shifted = |key| Action::KeyWithModifier(key, ModifierCombination::LSHIFT);
        for (keycode, tap, hold) in [
            (0x7C18, HidKeyCode::Kc9, ModifierCombination::LCTRL),
            (0x7C19, HidKeyCode::Kc0, ModifierCombination::RCTRL),
            (0x7C1A, HidKeyCode::Kc9, ModifierCombination::LSHIFT),
            (0x7C1B, HidKeyCode::Kc0, ModifierCombination::RSHIFT),
            (0x7C1C, HidKeyCode::Kc9, ModifierCombination::LALT),
            (0x7C1D, HidKeyCode::Kc0, ModifierCombination::RALT),
        ] {
            let action = tap_hold(shifted(tap), Action::Modifier(hold));
            assert!(
                identical(from_vial_keycode(keycode), action),
                "{keycode:#06x}"
            );
            assert_eq!(to_vial_keycode(action), keycode);
        }
        for layer in 0..32u8 {
            for (base, action) in [
                (0x5200, Action::LayerToggleOnly(layer)),
                (0x5220, Action::LayerOn(layer)),
                (0x5240, Action::DefaultLayer(layer)),
                (0x5260, Action::LayerToggle(layer)),
                (0x5280, Action::OneShotLayer(layer)),
                (0x52E0, Action::PersistentDefaultLayer(layer)),
            ] {
                let keycode = base | u16::from(layer);
                assert!(identical(from_vial_keycode(keycode), single(action)));
                assert_eq!(to_vial_keycode(single(action)), keycode);
            }
        }
        let rs_enter = tap_hold(
            Action::Key(KeyCode::Hid(HidKeyCode::Enter)),
            Action::Modifier(ModifierCombination::RSHIFT),
        );
        assert!(identical(from_vial_keycode(0x7C1E), rs_enter));
        assert_eq!(to_vial_keycode(rs_enter), 0x3228);
        assert_eq!(to_vial_keycode(single(Action::OneShotLayer(32))), 0);
        assert_eq!(to_vial_keycode(single(Action::User(0x90))), 0);
        let no_key = Action::Key(KeyCode::Hid(HidKeyCode::No));
        assert_eq!(
            to_vial_keycode(tap_hold(Action::LayerToggle(2), Action::LayerOn(1))),
            0x4100
        );
        assert_eq!(
            to_vial_keycode(tap_hold(no_key, Action::LayerOn(1))),
            0x4100
        );
    }

    #[test]
    fn handed_toggle_swaps_modifiers_and_keeps_the_profile() {
        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::A, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::LCTRL),
            3,
        );
        let toggled = toggle_handed_key_action(action);
        assert!(identical(
            toggled,
            KeyAction::TapHold(
                Action::KeyWithModifier(HidKeyCode::A, ModifierCombination::RSHIFT),
                Action::Modifier(ModifierCombination::RCTRL),
                3,
            )
        ));
        assert!(identical(toggle_handed_key_action(toggled), action));
        let key = KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::B)));
        assert!(identical(toggle_handed_key_action(key), key));
    }

    #[test]
    fn picker_bindings_cross_to_a_rynk_board_as_their_upgrade() {
        use rmk_types::action::{Action as V1Action, KeyAction as V1KeyAction};
        use rmk_types::keycode::HidKeyCode as V1HidKeyCode;
        use rmk_types::modifier::ModifierCombination as V1Modifiers;
        use rmk_types::morse::MorseProfile as V1MorseProfile;

        let user_ids = crate::universal_symbols::CONTROLS
            .iter()
            .map(|control| control.user_id)
            .chain(
                crate::universal_symbols::SYMBOLS
                    .iter()
                    .map(|symbol| symbol.user_id),
            )
            .chain(
                crate::universal_symbols::RUSSIAN_LETTERS
                    .iter()
                    .map(|letter| letter.user_id),
            );
        for user_id in user_ids {
            let taken = rynk_board_binding(crate::universal_symbols::binding(user_id)).unwrap();
            assert_eq!(
                binding_action(taken),
                Ok(KeyAction::Single(Action::User(user_id)))
            );
        }

        let tap = V1Action::KeyWithModifier(V1HidKeyCode::LeftBracket, V1Modifiers::LSHIFT);
        let hold = V1Action::Modifier(V1Modifiers::RCTRL);
        let mod_tap = V1KeyAction::TapHold(tap, hold, Default::default());
        let taken = rynk_board_binding(KeyBinding::Rmk(mod_tap)).unwrap();
        let upgraded = super::super::native_v1::upgrade_native_v1_key_action(mod_tap).action;
        assert!(identical(binding_action(taken).unwrap(), upgraded));
        assert_eq!(
            taken,
            key_binding(upgraded),
            "held as if read from the board"
        );

        // A tap-hold with its own timing would change on the way.
        let custom = V1KeyAction::TapHold(tap, hold, V1MorseProfile::from(0x4000_00C8));
        assert_eq!(rynk_board_binding(KeyBinding::Rmk(custom)), None);

        assert_eq!(
            rynk_board_binding(KeyBinding::Vial(0x04)),
            Some(KeyBinding::Vial(0x04))
        );
        let rynk = KeyBinding::Rynk(KeyAction::Single(Action::User(200)));
        assert_eq!(rynk_board_binding(rynk), Some(rynk));
    }

    #[test]
    fn rynk_actions_cross_to_a_vial_board_only_unchanged() {
        use rmk_types::action::{Action as V1Action, KeyAction as V1KeyAction};
        use rmk_types::keycode::HidKeyCode as V1HidKeyCode;
        use rmk_types::modifier::ModifierCombination as V1Modifiers;

        let a = KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::A)));
        assert_eq!(
            vial_board_binding(KeyBinding::Rynk(a), false),
            Some(KeyBinding::Vial(0x04))
        );

        let user = KeyBinding::Rynk(KeyAction::Single(Action::User(0x90)));
        assert_eq!(vial_board_binding(user, false), None);
        assert_eq!(
            vial_board_binding(user, true),
            Some(KeyBinding::Rmk(V1KeyAction::Single(V1Action::User(0x90))))
        );

        let tap = Action::KeyWithModifier(HidKeyCode::LeftBracket, ModifierCombination::LSHIFT);
        let hold = Action::Modifier(ModifierCombination::RCTRL);
        let mod_tap = KeyAction::TapHold(tap, hold, u8::MAX);
        assert_eq!(
            vial_board_binding(KeyBinding::Rynk(mod_tap), true),
            Some(KeyBinding::Rmk(V1KeyAction::TapHold(
                V1Action::KeyWithModifier(V1HidKeyCode::LeftBracket, V1Modifiers::LSHIFT),
                V1Action::Modifier(V1Modifiers::RCTRL),
                Default::default(),
            )))
        );
        // Native v1 has no profile table: another profile would be lost.
        let profiled = KeyAction::TapHold(tap, hold, 3);
        assert_eq!(vial_board_binding(KeyBinding::Rynk(profiled), true), None);

        for other in [
            KeyBinding::Vial(0x05),
            KeyBinding::Rmk(V1KeyAction::Single(V1Action::User(1))),
        ] {
            assert_eq!(vial_board_binding(other, true), Some(other));
        }
    }
}

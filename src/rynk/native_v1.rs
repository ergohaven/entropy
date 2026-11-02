//! Native v1 actions (`rmk_types`) next to the Rynk revision (`rmk_types_rynk`).
//!
//! Both encode an action the same way except for two types: v1
//! `KeyboardAction` still has DebugToggle and the three Output variants, and v1
//! `TapHold` carries a packed `MorseProfile` where Rynk holds an index into the
//! profile table. Everything else crosses as postcard bytes; the tests compare
//! every variant of the shared enums by name.

use rmk_types::action::{
    Action as V1Action, KeyAction as V1KeyAction, KeyboardAction as V1KeyboardAction,
};
use rmk_types::morse::MorseProfile as V1MorseProfile;
use rmk_types_rynk::action::{Action, KeyAction, KeyboardAction};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Firmware user keys that took over the v1 Output actions on every ergohaven
/// board (`rmk/src/processor/builtin/ergohaven.rs`).
const USER_BT_OUTPUT: u8 = 37;
const USER_USB_OUTPUT: u8 = 38;

/// Profile index the firmware's default-profile tap-hold keys carry; no table
/// entry, so the key takes the default profile.
const DEFAULT_PROFILE_INDEX: u8 = u8::MAX;

/// What a v1 action lost on its way to Rynk. The import reports these instead
/// of dropping them silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeV1Loss {
    /// DebugToggle and OutputAuto did nothing on v1 firmware and have no Rynk
    /// action; the key becomes `No`.
    KeyboardAction(V1KeyboardAction),
    /// A tap-hold key with its own packed profile. Rynk keys reference the
    /// profile table, so the key takes the default profile.
    TapHoldProfile(V1MorseProfile),
    /// An action this converter does not know.
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeV1Upgrade {
    pub(crate) action: KeyAction,
    pub(crate) losses: Vec<NativeV1Loss>,
}

/// Converts a native v1 action, as Vial boards and v7 .entlayout files store
/// it, to the Rynk revision.
pub(crate) fn upgrade_native_v1_key_action(action: V1KeyAction) -> NativeV1Upgrade {
    let mut losses = Vec::new();
    let mut upgrade = |action| match upgrade_action(action) {
        Ok(action) => action,
        Err(loss) => {
            losses.push(loss);
            Action::No
        }
    };
    let action = match action {
        V1KeyAction::No => KeyAction::No,
        V1KeyAction::Transparent => KeyAction::Transparent,
        V1KeyAction::Single(action) => match upgrade(action) {
            Action::No => KeyAction::No,
            action => KeyAction::Single(action),
        },
        V1KeyAction::Tap(action) => match upgrade(action) {
            Action::No => KeyAction::No,
            action => KeyAction::Tap(action),
        },
        V1KeyAction::TapHold(tap, hold, profile) => {
            let action = KeyAction::TapHold(upgrade(tap), upgrade(hold), DEFAULT_PROFILE_INDEX);
            if profile != V1MorseProfile::default() {
                losses.push(NativeV1Loss::TapHoldProfile(profile));
            }
            action
        }
        V1KeyAction::Morse(index) => KeyAction::Morse(index),
        _ => {
            losses.push(NativeV1Loss::Unsupported);
            KeyAction::No
        }
    };
    NativeV1Upgrade { action, losses }
}

fn upgrade_action(action: V1Action) -> Result<Action, NativeV1Loss> {
    let V1Action::KeyboardControl(control) = action else {
        return bridge(&action).ok_or(NativeV1Loss::Unsupported);
    };
    let control = match control {
        V1KeyboardAction::Bootloader => KeyboardAction::Bootloader,
        V1KeyboardAction::Reboot => KeyboardAction::Reboot,
        V1KeyboardAction::ClearEeprom => KeyboardAction::ClearEeprom,
        V1KeyboardAction::ComboOn => KeyboardAction::ComboOn,
        V1KeyboardAction::ComboOff => KeyboardAction::ComboOff,
        V1KeyboardAction::ComboToggle => KeyboardAction::ComboToggle,
        V1KeyboardAction::CapsWordToggle => KeyboardAction::CapsWordToggle,
        V1KeyboardAction::OutputBluetooth => return Ok(Action::User(USER_BT_OUTPUT)),
        V1KeyboardAction::OutputUsb => return Ok(Action::User(USER_USB_OUTPUT)),
        removed => return Err(NativeV1Loss::KeyboardAction(removed)),
    };
    Ok(Action::KeyboardControl(control))
}

/// The v1 action that labels a Rynk action in the key editor, which still
/// draws native actions through the v1 code. Written to a board only through
/// [`downgrade_key_action`]. `None` for actions v1 cannot hold, such as steno
/// keys.
pub(crate) fn v1_label_action(action: KeyAction) -> Option<V1KeyAction> {
    Some(match action {
        KeyAction::No => V1KeyAction::No,
        KeyAction::Transparent => V1KeyAction::Transparent,
        KeyAction::Single(action) => V1KeyAction::Single(v1_action(action)?),
        KeyAction::Tap(action) => V1KeyAction::Tap(v1_action(action)?),
        KeyAction::TapHold(tap, hold, _) => {
            V1KeyAction::TapHold(v1_action(tap)?, v1_action(hold)?, V1MorseProfile::default())
        }
        KeyAction::Morse(index) => V1KeyAction::Morse(index),
        _ => return None,
    })
}

/// The native v1 action a Vial board takes for a Rynk action from a file, if
/// it is the same action: one that upgrades back unchanged. A tap-hold key
/// with a profile from the table has none.
pub(crate) fn downgrade_key_action(action: KeyAction) -> Option<V1KeyAction> {
    let v1 = v1_label_action(action)?;
    let upgrade = upgrade_native_v1_key_action(v1);
    (upgrade.losses.is_empty() && crate::rynk::keycode::identical(upgrade.action, action))
        .then_some(v1)
}

/// The label of a Rynk action the v1 code cannot draw: its variant name and
/// argument, so the key never shows up blank.
pub(crate) fn fallback_label(action: KeyAction) -> String {
    let debug = match action {
        KeyAction::Single(action) | KeyAction::Tap(action) => format!("{action:?}"),
        other => format!("{other:?}"),
    };
    match debug.split_once('(') {
        Some((name, argument)) => format!("{name}\n{}", &argument[..argument.len() - 1]),
        None => debug,
    }
}

fn v1_action(action: Action) -> Option<V1Action> {
    let Action::KeyboardControl(control) = action else {
        return bridge(&action);
    };
    Some(V1Action::KeyboardControl(match control {
        KeyboardAction::Bootloader => V1KeyboardAction::Bootloader,
        KeyboardAction::Reboot => V1KeyboardAction::Reboot,
        KeyboardAction::ClearEeprom => V1KeyboardAction::ClearEeprom,
        KeyboardAction::ComboOn => V1KeyboardAction::ComboOn,
        KeyboardAction::ComboOff => V1KeyboardAction::ComboOff,
        KeyboardAction::ComboToggle => V1KeyboardAction::ComboToggle,
        KeyboardAction::CapsWordToggle => V1KeyboardAction::CapsWordToggle,
        _ => return None,
    }))
}

/// Moves a value between the two crates through its postcard bytes, for types
/// both encode alike.
fn bridge<A: Serialize, B: DeserializeOwned>(value: &A) -> Option<B> {
    let mut buf = [0u8; 32];
    let bytes = postcard::to_slice(value, &mut buf).ok()?;
    postcard::from_bytes(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmk_types::keycode::{HidKeyCode as V1HidKeyCode, KeyCode as V1KeyCode};
    use rmk_types::modifier::ModifierCombination as V1Modifiers;
    use rmk_types_rynk::keycode::{HidKeyCode, KeyCode};
    use rmk_types_rynk::modifier::ModifierCombination;

    fn upgrade(action: V1KeyAction) -> (KeyAction, Vec<NativeV1Loss>) {
        let upgrade = upgrade_native_v1_key_action(action);
        (upgrade.action, upgrade.losses)
    }

    /// Every variant index one crate decodes, with its name, until both stop:
    /// the shared enums must agree on order and names, or bytes that cross
    /// would change meaning without an error.
    fn assert_same_variants<
        V1: DeserializeOwned + std::fmt::Debug,
        R: DeserializeOwned + std::fmt::Debug,
    >(
        what: &str,
        wrap: impl Fn(u32) -> Vec<u8>,
    ) {
        let mut seen = 0;
        for index in 0..1024u32 {
            let bytes = wrap(index);
            let v1 = postcard::from_bytes::<V1>(&bytes).ok();
            let rynk = postcard::from_bytes::<R>(&bytes).ok();
            assert_eq!(
                v1.as_ref().map(|value| format!("{value:?}")),
                rynk.as_ref().map(|value| format!("{value:?}")),
                "{what} variant {index}"
            );
            seen += usize::from(v1.is_some());
        }
        assert!(seen > 0, "{what} decoded nothing");
    }

    fn varint(index: u32) -> Vec<u8> {
        postcard::to_slice(&index, &mut [0u8; 5]).unwrap().to_vec()
    }

    #[test]
    fn shared_enums_match_variant_for_variant() {
        use rmk_types::action::LightAction as V1Light;
        use rmk_types::keycode::{
            ConsumerKey as V1Consumer, SpecialKey as V1Special, SystemControlKey as V1System,
        };
        use rmk_types_rynk::action::LightAction;
        use rmk_types_rynk::keycode::{ConsumerKey, SpecialKey, SystemControlKey};

        assert_same_variants::<V1HidKeyCode, HidKeyCode>("HidKeyCode", varint);
        assert_same_variants::<V1Consumer, ConsumerKey>("ConsumerKey", varint);
        assert_same_variants::<V1System, SystemControlKey>("SystemControlKey", varint);
        assert_same_variants::<V1Light, LightAction>("LightAction", varint);
        assert_same_variants::<V1Special, SpecialKey>("SpecialKey", varint);
        // KeyCode wrapper: each variant around its first key.
        assert_same_variants::<V1KeyCode, KeyCode>("KeyCode", |index| {
            [varint(index), vec![1]].concat()
        });
        // Action: each variant with one-byte payloads where it takes them.
        // KeyboardControl differs and is mapped by hand; steno keys exist only
        // in the host build of the Rynk crate and never reach a v1 action.
        const KEYBOARD_CONTROL: u32 = 17;
        const STENO: u32 = 21;
        assert_same_variants::<V1Action, Action>("Action", |index| match index {
            KEYBOARD_CONTROL | STENO => vec![0xFF],
            _ => [varint(index), vec![1, 1]].concat(),
        });
        let steno = postcard::from_bytes::<Action>(&[STENO as u8, 1]).unwrap();
        assert!(format!("{steno:?}").starts_with("Steno("));
        assert_eq!(v1_label_action(KeyAction::Single(steno)), None);
        let label = fallback_label(KeyAction::Single(steno));
        assert!(
            label.starts_with("Steno\n") && label.len() > "Steno\n".len(),
            "{label}"
        );
        assert_eq!(fallback_label(KeyAction::Transparent), "Transparent");
        for bits in 0..=u8::MAX {
            assert_eq!(
                bridge::<_, ModifierCombination>(&V1Modifiers::from_bits(bits))
                    .map(ModifierCombination::into_bits),
                Some(bits)
            );
        }
    }

    #[test]
    fn keyboard_actions_follow_the_firmware() {
        let control = |action| V1KeyAction::Single(V1Action::KeyboardControl(action));
        let rynk = |action| KeyAction::Single(Action::KeyboardControl(action));

        for (v1, expected) in [
            (V1KeyboardAction::Bootloader, KeyboardAction::Bootloader),
            (V1KeyboardAction::Reboot, KeyboardAction::Reboot),
            (V1KeyboardAction::ClearEeprom, KeyboardAction::ClearEeprom),
            (V1KeyboardAction::ComboOn, KeyboardAction::ComboOn),
            (V1KeyboardAction::ComboOff, KeyboardAction::ComboOff),
            (V1KeyboardAction::ComboToggle, KeyboardAction::ComboToggle),
            (
                V1KeyboardAction::CapsWordToggle,
                KeyboardAction::CapsWordToggle,
            ),
        ] {
            assert_eq!(upgrade(control(v1)), (rynk(expected), vec![]));
            assert_eq!(v1_label_action(rynk(expected)), Some(control(v1)));
        }
        assert_eq!(
            upgrade(control(V1KeyboardAction::OutputBluetooth)),
            (KeyAction::Single(Action::User(37)), vec![])
        );
        assert_eq!(
            upgrade(V1KeyAction::Tap(V1Action::KeyboardControl(
                V1KeyboardAction::OutputUsb
            ))),
            (KeyAction::Tap(Action::User(38)), vec![])
        );
        // Every Rynk keyboard action has a v1 one, so labels never fall back.
        for index in 0..64u8 {
            if let Ok(action) = postcard::from_bytes::<KeyboardAction>(&[index]) {
                assert!(
                    v1_action(Action::KeyboardControl(action)).is_some(),
                    "{action:?}"
                );
            }
        }
        for removed in [V1KeyboardAction::DebugToggle, V1KeyboardAction::OutputAuto] {
            assert_eq!(
                upgrade(control(removed)),
                (KeyAction::No, vec![NativeV1Loss::KeyboardAction(removed)])
            );
        }
    }

    #[test]
    fn tap_hold_reports_only_a_custom_profile() {
        let tap = V1Action::KeyWithModifier(V1HidKeyCode::Kc0, V1Modifiers::LSHIFT);
        let hold = V1Action::Modifier(V1Modifiers::LCTRL);
        let expected = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::Kc0, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::LCTRL),
            u8::MAX,
        );

        let (action, losses) = upgrade(V1KeyAction::TapHold(tap, hold, Default::default()));
        // KeyAction's PartialEq skips the profile index, so compare it apart.
        assert_eq!((action, losses), (expected, vec![]));
        assert!(matches!(action, KeyAction::TapHold(_, _, u8::MAX)));

        // Packed v1 profile: PermissiveHold, 200 ms hold timeout.
        let custom = V1MorseProfile::from(0x4000_00C8);
        assert_eq!(
            upgrade(V1KeyAction::TapHold(tap, hold, custom)),
            (expected, vec![NativeV1Loss::TapHoldProfile(custom)])
        );

        let debug = V1Action::KeyboardControl(V1KeyboardAction::DebugToggle);
        assert_eq!(
            upgrade(V1KeyAction::TapHold(debug, hold, Default::default())),
            (
                KeyAction::TapHold(
                    Action::No,
                    Action::Modifier(ModifierCombination::LCTRL),
                    u8::MAX
                ),
                vec![NativeV1Loss::KeyboardAction(V1KeyboardAction::DebugToggle)]
            )
        );
    }

    #[test]
    fn other_actions_cross_unchanged() {
        let a = V1Action::Key(V1KeyCode::Hid(V1HidKeyCode::A));
        for (v1, rynk) in [
            (V1KeyAction::No, KeyAction::No),
            (V1KeyAction::Transparent, KeyAction::Transparent),
            (V1KeyAction::Morse(2), KeyAction::Morse(2)),
            (
                V1KeyAction::Single(V1Action::User(200)),
                KeyAction::Single(Action::User(200)),
            ),
            (
                V1KeyAction::Single(a),
                KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::A))),
            ),
            (
                V1KeyAction::Single(V1Action::PersistentDefaultLayer(3)),
                KeyAction::Single(Action::PersistentDefaultLayer(3)),
            ),
        ] {
            assert_eq!(upgrade(v1), (rynk, vec![]));
            assert_eq!(v1_label_action(rynk), Some(v1));
            assert_eq!(downgrade_key_action(rynk), Some(v1));
        }
    }

    #[test]
    fn only_an_action_that_comes_back_unchanged_goes_down_to_v1() {
        // A universal symbol, as a Rynk test build exported it from a K:04.
        let symbol = KeyAction::Single(Action::User(0x82));
        assert_eq!(
            downgrade_key_action(symbol),
            Some(V1KeyAction::Single(V1Action::User(0x82)))
        );
        let tap_hold = |profile| {
            KeyAction::TapHold(
                Action::Key(KeyCode::Hid(HidKeyCode::A)),
                Action::LayerOn(2),
                profile,
            )
        };
        assert_eq!(
            downgrade_key_action(tap_hold(u8::MAX)),
            Some(V1KeyAction::TapHold(
                V1Action::Key(V1KeyCode::Hid(V1HidKeyCode::A)),
                V1Action::LayerOn(2),
                Default::default()
            ))
        );
        // v1 packs its own profile; an index into the table means nothing there.
        assert_eq!(downgrade_key_action(tap_hold(5)), None);
    }
}

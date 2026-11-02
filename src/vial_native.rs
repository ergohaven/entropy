//! Vial-only native v1/v2 codec. Rynk's pinned types and protocol are independent.
use crate::keyboard::{KeyBinding, VialMorseProfile};
use anyhow::{bail, Context, Result};
use rmk_types::action::{Action, KeyAction};
use serde::{Deserialize, Serialize};

// V2 renumbered KeyboardAction. Keep the v1 model for UI/presets and
// translate only this Vial wire boundary; removed controls fail before I/O.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum WireAction {
    No,
    Key(rmk_types::keycode::KeyCode),
    Modifier(rmk_types::modifier::ModifierCombination),
    KeyWithModifier(
        rmk_types::keycode::HidKeyCode,
        rmk_types::modifier::ModifierCombination,
    ),
    LayerOn(u8),
    LayerOnWithModifier(u8, rmk_types::modifier::ModifierCombination),
    LayerOff(u8),
    LayerToggle(u8),
    DefaultLayer(u8),
    LayerToggleOnly(u8),
    TriLayerLower,
    TriLayerUpper,
    TriggerMacro(u8),
    OneShotLayer(u8),
    OneShotModifier(rmk_types::modifier::ModifierCombination),
    OneShotKey(rmk_types::keycode::HidKeyCode),
    Light(rmk_types::action::LightAction),
    KeyboardControl(u8),
    Special(rmk_types::keycode::SpecialKey),
    User(u8),
    PersistentDefaultLayer(u8),
}
impl WireAction {
    fn from_action(action: Action) -> Result<Self> {
        use rmk_types::action::KeyboardAction as K;
        Ok(match action {
            Action::No => Self::No,
            Action::Key(a) => Self::Key(a),
            Action::Modifier(a) => Self::Modifier(a),
            Action::KeyWithModifier(a, b) => Self::KeyWithModifier(a, b),
            Action::LayerOn(a) => Self::LayerOn(a),
            Action::LayerOnWithModifier(a, b) => Self::LayerOnWithModifier(a, b),
            Action::LayerOff(a) => Self::LayerOff(a),
            Action::LayerToggle(a) => Self::LayerToggle(a),
            Action::DefaultLayer(a) => Self::DefaultLayer(a),
            Action::LayerToggleOnly(a) => Self::LayerToggleOnly(a),
            Action::TriLayerLower => Self::TriLayerLower,
            Action::TriLayerUpper => Self::TriLayerUpper,
            Action::TriggerMacro(a) => Self::TriggerMacro(a),
            Action::OneShotLayer(a) => Self::OneShotLayer(a),
            Action::OneShotModifier(a) => Self::OneShotModifier(a),
            Action::OneShotKey(a) => Self::OneShotKey(a),
            Action::Light(a) => Self::Light(a),
            Action::KeyboardControl(a) => Self::KeyboardControl(match a {
                K::Bootloader => 0,
                K::Reboot => 1,
                K::ClearEeprom => 2,
                K::ComboOn => 3,
                K::ComboOff => 4,
                K::ComboToggle => 5,
                K::CapsWordToggle => 6,
                other => bail!("Keyboard control {other:?} is unsupported by Vial v2"),
            }),
            Action::Special(a) => Self::Special(a),
            Action::User(a) => Self::User(a),
            Action::PersistentDefaultLayer(a) => Self::PersistentDefaultLayer(a),
            other => bail!("Unsupported Vial v2 action {other:?}"),
        })
    }
    fn into_action(self) -> Result<Action> {
        use rmk_types::action::KeyboardAction as K;
        Ok(match self {
            Self::No => Action::No,
            Self::Key(a) => Action::Key(a),
            Self::Modifier(a) => Action::Modifier(a),
            Self::KeyWithModifier(a, b) => Action::KeyWithModifier(a, b),
            Self::LayerOn(a) => Action::LayerOn(a),
            Self::LayerOnWithModifier(a, b) => Action::LayerOnWithModifier(a, b),
            Self::LayerOff(a) => Action::LayerOff(a),
            Self::LayerToggle(a) => Action::LayerToggle(a),
            Self::DefaultLayer(a) => Action::DefaultLayer(a),
            Self::LayerToggleOnly(a) => Action::LayerToggleOnly(a),
            Self::TriLayerLower => Action::TriLayerLower,
            Self::TriLayerUpper => Action::TriLayerUpper,
            Self::TriggerMacro(a) => Action::TriggerMacro(a),
            Self::OneShotLayer(a) => Action::OneShotLayer(a),
            Self::OneShotModifier(a) => Action::OneShotModifier(a),
            Self::OneShotKey(a) => Action::OneShotKey(a),
            Self::Light(a) => Action::Light(a),
            Self::KeyboardControl(a) => Action::KeyboardControl(match a {
                0 => K::Bootloader,
                1 => K::Reboot,
                2 => K::ClearEeprom,
                3 => K::ComboOn,
                4 => K::ComboOff,
                5 => K::ComboToggle,
                6 => K::CapsWordToggle,
                _ => bail!("Unknown Vial v2 keyboard control {a}"),
            }),
            Self::Special(a) => Action::Special(a),
            Self::User(a) => Action::User(a),
            Self::PersistentDefaultLayer(a) => Action::PersistentDefaultLayer(a),
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum WireV2 {
    No,
    Transparent,
    Single(WireAction),
    Tap(WireAction),
    TapHold(WireAction, WireAction, u8),
    Morse(u8),
}

pub(crate) fn decode(
    bytes: &[u8],
    version: u8,
    mut resolve: impl FnMut(u8) -> Result<u64>,
) -> Result<KeyBinding> {
    if version == 1 {
        let (action, rest) = postcard::take_from_bytes::<KeyAction>(bytes)?;
        if !rest.is_empty() {
            bail!("Trailing Vial native v1 payload bytes");
        }
        return Ok(action.into());
    }
    if version != 2 {
        bail!("Unsupported Vial native version {version}");
    }
    let (action, rest) = postcard::take_from_bytes::<WireV2>(bytes)?;
    if !rest.is_empty() {
        bail!("Trailing Vial native v2 payload bytes");
    }
    Ok(match action {
        WireV2::No => KeyAction::No.into(),
        WireV2::Transparent => KeyAction::Transparent.into(),
        WireV2::Single(a) => KeyAction::Single(a.into_action()?).into(),
        WireV2::Tap(a) => KeyAction::Tap(a.into_action()?).into(),
        WireV2::Morse(i) => KeyAction::Morse(i).into(),
        WireV2::TapHold(tap, hold, index) => KeyBinding::VialTapHold {
            tap: tap.into_action()?,
            hold: hold.into_action()?,
            profile: VialMorseProfile {
                raw: resolve(index)?,
                index,
            },
        },
    })
}

pub(crate) fn encode(
    binding: KeyBinding,
    version: u8,
    mut match_profile: impl FnMut(u64, Option<u8>, bool) -> Result<u8>,
) -> Result<Vec<u8>> {
    let (tap, hold, raw, index, inherited) = match binding {
        KeyBinding::Rmk(KeyAction::TapHold(tap, hold, profile)) => {
            let raw = u32::from(profile) as u64;
            (tap, hold, raw, None, raw == 0)
        }
        KeyBinding::VialTapHold { tap, hold, profile } => {
            (tap, hold, profile.raw, Some(profile.index), false)
        }
        KeyBinding::Rmk(action) if version == 1 => return Ok(packed(&action)?),
        KeyBinding::Rmk(action) if version == 2 => {
            return packed(&match action {
                KeyAction::No => WireV2::No,
                KeyAction::Transparent => WireV2::Transparent,
                KeyAction::Single(a) => WireV2::Single(WireAction::from_action(a)?),
                KeyAction::Tap(a) => WireV2::Tap(WireAction::from_action(a)?),
                KeyAction::Morse(i) => WireV2::Morse(i),
                _ => bail!("Unsupported Vial native action"),
            })
        }
        _ => bail!("Not a supported Vial native binding / version {version}"),
    };
    match version {
        1 => {
            let raw = u32::try_from(raw).context(
                "This Vial v2 profile needs a v2 device; v1 cannot represent its quick-tap bits",
            )?;
            Ok(packed(&KeyAction::TapHold(tap, hold, raw.into()))?)
        }
        2 => Ok(packed(&WireV2::TapHold(
            WireAction::from_action(tap)?,
            WireAction::from_action(hold)?,
            match_profile(raw, index, inherited)?,
        ))?),
        _ => bail!("Unsupported Vial native version {version}"),
    }
}

fn packed(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = [0; 64];
    Ok(postcard::to_slice(value, &mut bytes)?.to_vec())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProfileResolution {
    pub index: u8,
    pub exists: bool,
    pub raw: u64,
    pub default: u64,
    pub count: u16,
}
pub(crate) fn profile_response(r: &[u8; 32], index: u8) -> Result<ProfileResolution> {
    if r[..4] != [8, 0xe8, 8, 2]
        || r[4] != 0
        || r[5] != index
        || r[6] > 1
        || r[7] != 0
        || r[26..].iter().any(|&b| b != 0)
    {
        bail!(
            "Invalid Vial profile response for index {index} (status {})",
            r[4]
        );
    }
    let p = ProfileResolution {
        index,
        exists: r[6] == 1,
        raw: u64::from_le_bytes(r[8..16].try_into().unwrap()),
        default: u64::from_le_bytes(r[16..24].try_into().unwrap()),
        count: u16::from_le_bytes(r[24..26].try_into().unwrap()),
    };
    if p.count > 256
        || p.exists != (u16::from(index) < p.count)
        || (!p.exists && p.raw != p.default)
    {
        bail!("Inconsistent Vial profile table");
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;
    #[test]
    fn version_compat_actual_firmware_serializer_frames_are_lossless() {
        for line in include_str!("../tests/golden/vial/native_v2_firmware.frames").lines() {
            let (hex, expected) = line.split_once('\t').unwrap();
            let bytes: Vec<_> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let binding = decode(&bytes, 2, |_| panic!("no profiles in scalar fixture")).unwrap();
            let KeyBinding::Rmk(action) = binding else {
                panic!("unexpected binding");
            };
            assert_eq!(format!("{action:?}"), expected, "firmware bytes {hex}");
            assert_eq!(
                encode(binding, 2, |_, _, _| panic!("no profiles")).unwrap(),
                bytes
            );
        }
    }
    #[test]
    fn version_compat_profile_endpoint_validates_full_contract() {
        let mut response = [0; 32];
        response[..8].copy_from_slice(&[8, 0xe8, 8, 2, 0, 255, 1, 0]);
        response[8..16].copy_from_slice(&((1u64 << 45) | 180).to_le_bytes());
        response[16..24].copy_from_slice(&200u64.to_le_bytes());
        response[24..26].copy_from_slice(&256u16.to_le_bytes());
        assert_eq!(
            profile_response(&response, 255).unwrap().raw,
            (1u64 << 45) | 180
        );
        response[6] = 0;
        assert!(
            profile_response(&response, 255).is_err(),
            "255 is not always absent"
        );
        response[24..26].copy_from_slice(&1u16.to_le_bytes());
        response[8..16].copy_from_slice(&200u64.to_le_bytes());
        assert!(!profile_response(&response, 255).unwrap().exists);
        for byte in [0, 1, 2, 3, 4, 5, 6, 7, 26, 31] {
            let mut bad = response;
            bad[byte] ^= 128;
            assert!(profile_response(&bad, 255).is_err(), "byte {byte}");
        }
    }
    #[test]
    fn version_compat_v2_high_bits_decode_encode_and_snapshot() {
        // Build the fixture with the Vial v2 schema, not the v1 profile codec.
        let bytes = packed(&WireV2::TapHold(
            WireAction::User(180),
            WireAction::User(144),
            0,
        ))
        .unwrap();
        let raw = (1u64 << 45) | 200;
        let binding = decode(&bytes, 2, |i| {
            assert_eq!(i, 0);
            Ok(raw)
        })
        .unwrap();
        let KeyBinding::VialTapHold { profile, .. } = binding else {
            panic!("lost profile");
        };
        assert_eq!(profile.raw, raw);
        assert_eq!(profile.index, 0);
        let snapshot = serde_json::to_string(&binding).unwrap();
        let back = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(
            encode(back, 2, |p, i, inherit| {
                assert_eq!(p, raw);
                assert_eq!(i, Some(0));
                assert!(!inherit);
                Ok(0)
            })
            .unwrap(),
            bytes
        );
        assert!(encode(back, 1, |_, _, _| panic!("v1 profile lookup")).is_err());
    }
}

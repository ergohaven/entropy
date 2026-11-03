use super::*;
use rmk_types::action::{Action, KeyboardAction};

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn version_compat_explicit_unknown_capability_reply_reaches_version_validation() {
    let command = [8, 0xe8, 2, 0, 0];
    for version in [0, 3, 255] {
        let mut reply = [0; 32];
        reply[..5].copy_from_slice(&[8, 0xe8, 2, version, 1]);
        assert_eq!(matches_rmk_native_response(&command, &reply), Some(true));
    }
}

#[test]
fn version_compat_all_native_consumers_use_negotiated_codec() {
    for version in [1, 2] {
        let action = KeyAction::Single(Action::KeyboardControl(KeyboardAction::ClearEeprom));
        let mut temp = [0; 24];
        let payload = if version == 1 {
            postcard::to_slice(&action, &mut temp).unwrap().to_vec()
        } else {
            let frame = include_str!("../tests/golden/vial/native_v2_firmware.frames")
                .lines()
                .find(|l| l.ends_with("Single(KeyboardControl(ClearEeprom))"))
                .unwrap();
            unhex(frame.split_once('\t').unwrap().0)
        };
        let expected = payload.clone();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(move |request| {
            if request[1] != 0xe8 {
                return None;
            }
            let mut reply = [0; 32];
            reply[..4].copy_from_slice(&[request[0], 0xe8, request[2], version]);
            if request[2] == 2 {
                reply[4..6].copy_from_slice(&0x03ffu16.to_le_bytes());
            } else {
                assert_eq!(request[3], version, "opcode {}", request[2]);
                match request[2] {
                    3 | 5 if request[0] == 7 => {
                        assert_eq!(&request[8..8 + request[7] as usize], payload)
                    }
                    3 | 5 => {
                        reply[5] = payload.len() as u8;
                        reply[6..6 + payload.len()].copy_from_slice(&payload);
                    }
                    4 | 6 => {
                        reply[7] = payload.len() as u8;
                        reply[8..8 + payload.len()].copy_from_slice(&payload);
                    }
                    7 => {
                        reply[5] = 1;
                        reply[6] = 2;
                        reply[7] = request[4];
                    }
                    _ => panic!("unexpected opcode"),
                }
            }
            Some(Ok(reply))
        });
        hid.get_rmk_native_capabilities().unwrap();
        assert_eq!(
            hid.encode_vial_binding(hid.get_rmk_key_action(0, 0, 0).unwrap())
                .unwrap(),
            expected
        );
        hid.set_rmk_key_action(0, 0, 0, action).unwrap();
        let next = hid.get_next_rmk_native_key_action(0).unwrap().unwrap();
        assert_eq!(hid.encode_vial_binding(next.action).unwrap(), expected);
        let dynamic = hid.get_rmk_native_dynamic_action(0, 0, 0).unwrap();
        assert_eq!(hid.encode_vial_binding(dynamic).unwrap(), expected);
        hid.set_rmk_combo_output(0, action).unwrap();
        hid.set_rmk_tap_dance_action(0, 0, action).unwrap();
        let next = hid.get_next_rmk_native_dynamic_action(0).unwrap().unwrap();
        assert_eq!(hid.encode_vial_binding(next.action).unwrap(), expected);
        assert_eq!(hid.get_rmk_combo_layer(0).unwrap(), Some(2));
        hid.set_rmk_combo_layer(0, Some(2)).unwrap();
        for r in recorder.requests() {
            if r[2] != 2 {
                assert_eq!(r[3], version);
            }
        }
    }
}

#[test]
fn version_compat_actual_profile_owner_frames_roundtrip() {
    let frames: Vec<_> = include_str!("../tests/golden/vial/native_v2_profiles.frames")
        .lines()
        .map(|l| {
            let (a, b) = l.split_once('\t').unwrap();
            (unhex(a), unhex(b))
        })
        .collect();
    for (request, response) in frames {
        let response: [u8; 32] = response.try_into().unwrap();
        if request[3] == 1 {
            assert!(crate::vial_native::profile_response(&response, request[4]).is_err());
            continue;
        }
        let resolved = crate::vial_native::profile_response(&response, request[4]).unwrap();
        let payload = include_str!("../tests/golden/vial/native_v2_taphold.frames")
            .lines()
            .find(|l| l.starts_with(&format!("{}\t", request[4])))
            .unwrap();
        let payload = unhex(payload.split_once('\t').unwrap().1);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        *hid.native_capabilities.lock().unwrap() = Some(RmkNativeCapabilities {
            version: 2,
            key_actions: true,
            profile_resolution: true,
            ..Default::default()
        });
        let action_bytes = payload.clone();
        recorder.respond_by(move |r| {
            if r[2] == 8 {
                assert_eq!(r[4], response[5]);
                return Some(Ok(response));
            }
            let mut out = [0; 32];
            out[..4].copy_from_slice(&[r[0], 0xe8, r[2], 2]);
            if r[0] == 8 {
                let offset = if r[2] == 4 { 8 } else { 6 };
                out[offset - 1] = action_bytes.len() as u8;
                out[offset..offset + action_bytes.len()].copy_from_slice(&action_bytes);
            }
            Some(Ok(out))
        });
        let binding = hid.get_rmk_key_action(0, 0, 0).unwrap();
        let KeyBinding::VialTapHold { tap, profile, .. } = binding else {
            panic!("lost v2 profile");
        };
        assert!(matches!(
            tap,
            Action::KeyboardControl(KeyboardAction::ClearEeprom)
        ));
        assert_eq!(profile.raw, resolved.raw);
        assert_eq!(profile.index, request[4]);
        assert_eq!(hid.encode_vial_binding(binding).unwrap(), payload);
        hid.set_rmk_key_action(0, 0, 0, binding).unwrap();
        let scan = hid.get_next_rmk_native_key_action(0).unwrap().unwrap();
        assert_eq!(scan.action, binding);
    }
}

#[test]
fn version_compat_v1_profiles_map_only_to_exact_entries_or_real_default() {
    for count in [0u16, 1, 256] {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        *hid.native_capabilities.lock().unwrap() = Some(RmkNativeCapabilities {
            version: 2,
            key_actions: true,
            profile_resolution: true,
            ..Default::default()
        });
        recorder.respond_by(move |r| {
            assert_eq!(r[..4], [8, 0xe8, 8, 2]);
            let mut reply = [0; 32];
            let exists = u16::from(r[4]) < count;
            reply[..8].copy_from_slice(&[8, 0xe8, 8, 2, 0, r[4], u8::from(exists), 0]);
            reply[8..16].copy_from_slice(&if exists { 180u64 } else { 200 }.to_le_bytes());
            reply[16..24].copy_from_slice(&200u64.to_le_bytes());
            reply[24..26].copy_from_slice(&count.to_le_bytes());
            Some(Ok(reply))
        });
        for raw in [0u32, 180, 200, 999] {
            let binding =
                KeyAction::TapHold(Action::User(144), Action::User(145), raw.into()).into();
            let result = hid.encode_vial_binding(binding);
            let wanted = if raw == 0 { 200 } else { raw };
            if wanted == 180 && count > 0 {
                assert_eq!(result.unwrap().last(), Some(&0));
            } else if wanted == 200 && count < 256 {
                assert_eq!(result.unwrap().last(), Some(&(count as u8)));
            } else {
                assert!(result.is_err(), "unmatched {raw}, table count {count}");
            }
        }
        assert!(recorder.requests().iter().all(|r| r[0] == 8));
    }
}

#[test]
fn version_compat_morse_compound_rejected_before_clearing_entry() {
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    *hid.native_capabilities.lock().unwrap() = Some(RmkNativeCapabilities {
        version: 2,
        key_actions: true,
        profile_resolution: true,
        tap_dance_actions: true,
        ..Default::default()
    });
    recorder.respond_by(|r| {
        if r[2] != 8 {
            return None;
        }
        let mut out = [0; 32];
        out[..8].copy_from_slice(&[8, 0xe8, 8, 2, 0, r[4], 1, 0]);
        out[24] = 1;
        Some(Ok(out))
    });
    let compound = KeyBinding::VialTapHold {
        tap: Action::User(144),
        hold: Action::User(145),
        profile: crate::keyboard::VialMorseProfile { raw: 0, index: 0 },
    };
    assert!(hid
        .set_tap_dance_bindings(0, [0.into(), compound, 0.into(), 0.into()], 200)
        .is_err());
    assert!(
        recorder.requests().iter().all(|r| r[0] == 8),
        "Morse accepts only No/Single; must not clear before rejecting a compound"
    );
}

#[test]
fn version_compat_v2_mirroring_retains_profile_bits_and_index() {
    use rmk_types::modifier::ModifierCombination;
    let profile = crate::keyboard::VialMorseProfile {
        raw: (1u64 << 45) | 200,
        index: 255,
    };
    let binding = KeyBinding::VialTapHold {
        tap: Action::Modifier(ModifierCombination::from_bits(1)),
        hold: Action::Modifier(ModifierCombination::from_bits(2)),
        profile,
    };
    let KeyBinding::VialTapHold {
        tap,
        hold,
        profile: back,
    } = toggle_handed_vial_binding(binding)
    else {
        panic!("profile lost");
    };
    assert_eq!(back, profile);
    assert_eq!(tap, Action::Modifier(ModifierCombination::from_bits(16)));
    assert_eq!(hold, Action::Modifier(ModifierCombination::from_bits(32)));
}

#[test]
fn version_compat_malformed_and_unknown_payloads_fail_closed() {
    for version in [0, 1, 2, 3, 255] {
        assert!(crate::vial_native::decode(&[255, 255], version, |_| Ok(0)).is_err());
    }
    for version in [1, 2] {
        assert!(crate::vial_native::decode(&[0, 0], version, |_| Ok(0)).is_err());
    }
}

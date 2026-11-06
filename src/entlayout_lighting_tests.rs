use super::tests::module_app;
use super::*;

fn indication_app(value: u8) -> EntropyApp {
    let mut app = module_app();
    let definition = serde_json::json!({"settings":[{"name":"LEDs", "fields":[
        {"type":"integer","title":"LED brightness","qsid":316,"width":2,"max":255},
        {"type":"integer","title":"LED timeout seconds","qsid":317,"width":2,"max":600},
        {"type":"select","title":"BT profile 1 color","qsid":310,"variants":LAYER_LED_PALETTE}
    ]}]});
    app.supported_qmk_settings = vec![310, 316, 317];
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(move |r| {
        (r[..2] == [0xfe, 0x0a]).then(|| {
            let mut response = [0; 32];
            response[1] = value;
            Ok(response)
        })
    });
    app.layer_led_settings =
        EntropyApp::read_layer_led_settings(&definition, &app.supported_qmk_settings, 4, &hid);
    app
}

fn roundtrip(app: &EntropyApp) -> EntLayoutFile {
    serde_json::from_str(&serde_json::to_string(&app.entlayout_snapshot().unwrap()).unwrap())
        .unwrap()
}

#[test]
fn entlayout_lighting_indication_roundtrip_and_failed_reads() {
    let source = indication_app(16);
    let bundle = roundtrip(&source);
    let mut target = indication_app(0);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert!(failures.is_empty());
    assert_eq!(report.imported, 3);
    assert_eq!(target.layer_led_settings, source.layer_led_settings);
    let writes: Vec<_> = recorder
        .requests()
        .into_iter()
        .filter(|r| r[..2] == [0xfe, 0x0b])
        .map(|r| {
            (
                u16::from_le_bytes([r[2], r[3]]),
                u16::from_le_bytes([r[4], r[5]]),
            )
        })
        .collect();
    assert_eq!(writes, vec![(310, 16), (316, 16), (317, 16)]);
    target
        .layer_led_settings
        .brightness
        .as_mut()
        .unwrap()
        .loaded = false;
    target.layer_led_settings.timeout.as_mut().unwrap().loaded = false;
    target.layer_led_settings.bt_profile_colors[0].loaded = false;
    assert!(roundtrip(&target).data.portable_settings.is_empty());
}

#[test]
fn entlayout_lighting_indication_contract_mismatches_and_duplicates_never_write() {
    let bundle = roundtrip(&indication_app(16));
    for case in 0..5 {
        let mut target = indication_app(0);
        let mut bundle = bundle.clone();
        match case {
            0 => target.layer_led_settings.timeout_unit = LayerLedTimeoutUnit::Minutes,
            1 => target.layer_led_settings.brightness.as_mut().unwrap().max = 100,
            2 => target.layer_led_settings.bt_profile_colors[0].portable_profile = Some(2),
            3 => {
                let s = bundle.data.portable_settings[0].clone();
                bundle.data.portable_settings.push(s);
            }
            _ => target.layer_led_settings.timeout.as_mut().unwrap().width = 1,
        }
        let plan = plan_entlayout_portable_settings_import(
            &bundle.data.portable_settings,
            &target.entlayout_portable_settings_snapshot(),
            &[],
        );
        assert!(
            plan.report.incompatible + plan.report.unsupported > 0,
            "case {case}"
        );
        let affected = match case {
            0 | 4 => 317,
            1 => 316,
            _ => 310,
        };
        assert!(!plan.compatible.iter().any(|s| s.qsid == affected));
    }
}

#[test]
fn entlayout_lighting_old_backup_keeps_current_settings() {
    let source = indication_app(16);
    let mut json = serde_json::to_value(roundtrip(&source)).unwrap();
    json["data"]
        .as_object_mut()
        .unwrap()
        .remove("portable_settings");
    json["data"].as_object_mut().unwrap().remove("rgb");
    let bundle: EntLayoutFile = serde_json::from_value(json).unwrap();
    let mut target = indication_app(4);
    let before = target.layer_led_settings.clone();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.total(), 0);
    assert_eq!(target.layer_led_settings, before);
    assert!(!recorder
        .requests()
        .iter()
        .any(|r| r[0] == 7 || r[0] == 9 || r[..2] == [0xfe, 0x0b]));
}

fn rgb_app(kind: RgbSupportKind, effect: u16) -> EntropyApp {
    let mut app = module_app();
    app.module_settings.values.clear();
    app.layout.as_mut().unwrap().lighting_mode = Some(
        match kind {
            RgbSupportKind::VialRgb => "vialrgb",
            _ => "qmk_rgblight",
        }
        .into(),
    );
    app.rgb_settings = RgbSettingsState {
        loaded: true,
        supported: true,
        kind,
        effect,
        brightness: 80,
        speed: 42,
        hue: 17,
        saturation: 200,
        max_brightness: 255,
        supported_effects: vec![0, 6],
        last_enabled_effect: effect,
    };
    app
}

#[test]
fn entlayout_lighting_rgb_roundtrip_verifies_and_saves_both_protocols() {
    for kind in [RgbSupportKind::QmkRgblight, RgbSupportKind::VialRgb] {
        let source = rgb_app(kind, 6);
        let bundle = roundtrip(&source);
        let mut target = rgb_app(kind, 0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(move |r| {
            if r[0] != 8 {
                return None;
            }
            let mut response = *r;
            match r[1] {
                0x80 => response[2] = 80,
                0x81 => response[2] = 6,
                0x82 => response[2] = 42,
                0x83 => {
                    response[2] = 17;
                    response[3] = 200;
                }
                0x41 => response[2..8].copy_from_slice(&[6, 0, 42, 17, 200, 80]),
                _ => return None,
            }
            Some(Ok(response))
        });
        target.hid_device = Some(hid.into());
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.imported, 1);
        assert_eq!(
            target.entlayout_rgb_snapshot(),
            source.entlayout_rgb_snapshot()
        );
        let writes: Vec<_> = recorder
            .requests()
            .into_iter()
            .filter(|r| r[0] == 7 || r[0] == 9)
            .collect();
        assert_eq!(writes.last().unwrap()[0], 9);
        if kind == RgbSupportKind::QmkRgblight {
            assert_eq!(
                writes.iter().map(|r| r[..4].to_vec()).collect::<Vec<_>>(),
                vec![
                    vec![7, 0x81, 1, 0],
                    vec![7, 0x80, 80, 0],
                    vec![7, 0x83, 17, 200],
                    vec![7, 0x82, 42, 0],
                    vec![7, 0x81, 6, 0],
                    vec![9, 0, 0, 0]
                ]
            );
        } else {
            assert_eq!(&writes[0][..8], &[7, 0x41, 6, 0, 42, 17, 200, 80]);
        }
        // No redundant flash writes for a confirmed identical state.
        let count = recorder.requests().len();
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.imported, 1);
        assert!(!recorder.requests()[count..]
            .iter()
            .any(|r| r[0] == 7 || r[0] == 9));
    }
}

#[test]
fn entlayout_lighting_rgb_unsupported_incompatible_and_failed_readback() {
    let source = rgb_app(RgbSupportKind::VialRgb, 6);
    let bundle = roundtrip(&source);
    for case in 0..7 {
        let mut target = rgb_app(RgbSupportKind::VialRgb, 0);
        match case {
            0 => target.rgb_settings.supported = false,
            1 => target.rgb_settings.kind = RgbSupportKind::QmkRgblight,
            2 => target.rgb_settings.supported_effects = vec![0],
            3 => target.rgb_settings.max_brightness = 50,
            4 => target.rgb_settings.loaded = false,
            6 => {
                target.rgb_settings.loaded = false;
                target.rgb_settings.supported_effects = vec![0];
            }
            _ => (),
        }
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        if case >= 4 {
            recorder.respond_by(|r| (r[0] == 8).then(|| Err("RGB unavailable".into())));
        }
        target.hid_device = Some(hid.into());
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        if case >= 4 {
            assert_eq!(report.failed, 1);
            assert!(!failures.is_empty());
            assert!(target.entlayout_rgb_snapshot().is_none());
            assert!(!recorder.requests().iter().any(|r| r[0] == 9));
        } else {
            assert_eq!(report.incompatible + report.unsupported, 1);
            assert!(!recorder.requests().iter().any(|r| r[0] == 7 || r[0] == 9));
        }
    }
}

#[test]
fn entlayout_lighting_failed_indication_write_refreshes_or_omits_cache() {
    let bundle = roundtrip(&indication_app(16));
    let mut target = indication_app(0);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| {
        (r[..2] == [0xfe, 0x0a] || r[..2] == [0xfe, 0x0b]).then(|| Err("device unavailable".into()))
    });
    target.hid_device = Some(hid.into());
    let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.failed, 3);
    assert!(!failures.is_empty());
    assert!(roundtrip(&target).data.portable_settings.is_empty());
}

#[test]
fn entlayout_lighting_failed_device_reads_do_not_export_fallbacks() {
    let mut app = indication_app(0);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| (r[..2] == [0xfe, 0x0a]).then(|| Err("failed GET".into())));
    let definition = serde_json::json!({"settings":[{"name":"LEDs","fields":[
        {"type":"integer","title":"LED brightness","qsid":316,"width":2,"max":255},
        {"type":"integer","title":"LED timeout seconds","qsid":317,"width":2,"max":600},
        {"type":"select","title":"BT profile 1 color","qsid":310,"variants":LAYER_LED_PALETTE}
    ]}]});
    app.layer_led_settings =
        EntropyApp::read_layer_led_settings(&definition, &app.supported_qmk_settings, 4, &hid);
    assert!(app.entlayout_portable_settings_snapshot().is_empty());
    assert!(!app.layer_led_settings.brightness.as_ref().unwrap().loaded);
    assert!(!app.layer_led_settings.timeout.as_ref().unwrap().loaded);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| (r[0] == 8 && r[1] == 0x82).then(|| Err("failed speed GET".into())));
    app.rgb_settings = load_rgb_settings_for_mode(&hid, Some("qmk_rgblight"));
    assert!(app.rgb_settings.supported);
    assert!(app.entlayout_rgb_snapshot().is_none());
}

#[test]
fn entlayout_lighting_retry_after_failure_keeps_device_capabilities() {
    for rgb in [false, true] {
        let source = if rgb {
            rgb_app(RgbSupportKind::VialRgb, 6)
        } else {
            indication_app(16)
        };
        let bundle = roundtrip(&source);
        let mut target = if rgb {
            rgb_app(RgbSupportKind::VialRgb, 0)
        } else {
            indication_app(0)
        };
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|r| {
            (r[0] == 8 || r[..2] == [0xfe, 0x0b] || r[..2] == [0xfe, 0x0a])
                .then(|| Err("transient failure".into()))
        });
        target.hid_device = Some(hid.into());
        assert!(target.apply_entlayout(&bundle).unwrap().2.failed > 0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|r| {
            let mut response = [0; 32];
            match &r[..2] {
                [8, 0x40] => response[2..5].copy_from_slice(&[1, 0, 255]),
                [8, 0x41] => response[2..8].copy_from_slice(&[6, 0, 42, 17, 200, 80]),
                [8, 0x42] => response[2..6].copy_from_slice(&[6, 0, 255, 255]),
                _ => return None,
            }
            Some(Ok(response))
        });
        target.hid_device = Some(hid.into());
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.unsupported, 0);
        assert_eq!(report.imported, if rgb { 1 } else { 3 });
        assert!(recorder.requests().iter().any(|r| if rgb {
            r[0] == 9
        } else {
            r[..2] == [0xfe, 0x0b]
        }));
    }
}

#[test]
fn entlayout_lighting_failed_rgb_reads_are_backup_sections() {
    for channel in [0x80, 0x81, 0x82, 0x83, 0x40, 0x41, 0x42] {
        assert_eq!(
            crate::hid::hid_protocol::entlayout_section_of_read(&[8, channel]),
            Some("Rgb")
        );
    }
    assert_ne!(
        crate::hid::hid_protocol::entlayout_section_of_read(&[8, 0xe8, 3]),
        Some("Rgb")
    );
}

#[test]
fn entlayout_lighting_partial_rgb_write_reloads_actual_device_state() {
    let bundle = roundtrip(&rgb_app(RgbSupportKind::QmkRgblight, 6));
    let mut target = rgb_app(RgbSupportKind::QmkRgblight, 0);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| {
        if r[..3] == [7, 0x81, 6] {
            return Some(Err("effect write failed".into()));
        }
        let mut response = [0; 32];
        match &r[..2] {
            [8, 0x80] => response[2] = 80,
            [8, 0x81] => response[2] = 1,
            [8, 0x82] => response[2] = 42,
            [8, 0x83] => {
                response[2] = 17;
                response[3] = 200;
            }
            _ => return None,
        }
        Some(Ok(response))
    });
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(
        (
            target.rgb_settings.effect,
            target.rgb_settings.brightness,
            target.rgb_settings.speed
        ),
        (1, 80, 42)
    );
    assert_eq!(
        (target.rgb_settings.hue, target.rgb_settings.saturation),
        (17, 200)
    );
    assert!(!target.rgb_settings.loaded);
    assert!(target.entlayout_rgb_snapshot().is_none());
    assert!(!recorder.requests().iter().any(|r| r[0] == 9));
}

#[test]
fn entlayout_lighting_failed_primary_probe_does_not_export_echo_fallback() {
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| {
        if r[..2] == [8, 0x40] {
            Some(Err("primary RGB probe failed".into()))
        } else if r[0] == 8 {
            Some(Ok(*r))
        } else {
            None
        }
    });
    let mut app = rgb_app(RgbSupportKind::VialRgb, 6);
    app.rgb_settings = load_rgb_settings_for_mode(&hid, Some("vialrgb"));
    assert!(!app.rgb_settings.loaded);
    assert!(app.entlayout_rgb_snapshot().is_none());
    app.layout.as_mut().unwrap().lighting_mode = Some("vialrgb".into());
    app.hid_device = Some(hid.into());
    assert!(!app.reload_rgb_settings());
    assert!(!app.rgb_settings.loaded);
    assert!(app.entlayout_rgb_snapshot().is_none());
}

#[test]
fn entlayout_lighting_vial_effect_read_failure_can_retry_after_recovery() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let bundle = roundtrip(&rgb_app(RgbSupportKind::VialRgb, 6));
    let mut app = rgb_app(RgbSupportKind::VialRgb, 0);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    let failed = Arc::new(AtomicBool::new(true));
    let failed_response = failed.clone();
    let written = AtomicBool::new(false);
    recorder.respond_by(move |r| {
        let mut response = [0; 32];
        match &r[..2] {
            [8, 0x40] => response[2..5].copy_from_slice(&[1, 0, 255]),
            [8, 0x41] => response[2..8].copy_from_slice(&[
                if written.load(Ordering::SeqCst) { 6 } else { 0 },
                0,
                42,
                17,
                200,
                80,
            ]),
            [8, 0x42] => {
                if failed_response.load(Ordering::SeqCst) {
                    return Some(Err("effect-list read failed".into()));
                }
                response[2..6].copy_from_slice(&[6, 0, 255, 255]);
            }
            [7, 0x41] => {
                written.store(true, Ordering::SeqCst);
            }
            _ => return None,
        }
        Some(Ok(response))
    });
    app.rgb_settings = load_rgb_settings_for_mode(&hid, Some("vialrgb"));
    assert!(!app.rgb_settings.loaded);
    assert_eq!(app.rgb_settings.supported_effects, vec![0]);
    app.hid_device = Some(hid.into());
    let (errors, _, report) = app.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.incompatible, 0);
    assert!(!errors.is_empty());
    assert!(!recorder.requests().iter().any(|r| r[0] == 7 || r[0] == 9));
    failed.store(false, Ordering::SeqCst);
    let (errors, _, report) = app.apply_entlayout(&bundle).unwrap();
    assert!(errors.is_empty());
    assert_eq!(report.imported, 1);
    assert!(app.rgb_settings.loaded);
    assert_eq!(app.rgb_settings.effect, 6);
    assert!(recorder.requests().iter().any(|r| r[0] == 9));
}

#[test]
fn entlayout_lighting_unconfirmed_matching_ram_still_saves_on_retry() {
    let bundle = roundtrip(&rgb_app(RgbSupportKind::QmkRgblight, 6));
    let mut target = rgb_app(RgbSupportKind::QmkRgblight, 6);
    target.rgb_settings.loaded = false;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|r| {
        let mut response = [0; 32];
        match &r[..2] {
            [8, 0x80] => response[2] = 80,
            [8, 0x81] => response[2] = 6,
            [8, 0x82] => response[2] = 42,
            [8, 0x83] => {
                response[2] = 17;
                response[3] = 200;
            }
            _ => return None,
        }
        Some(Ok(response))
    });
    target.hid_device = Some(hid.into());
    let (errors, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert!(errors.is_empty());
    assert_eq!(report.imported, 1);
    assert!(recorder.requests().iter().any(|r| r[0] == 9));
}

#[test]
fn entlayout_lighting_rgblight_restores_hsv_when_disabled_or_animated() {
    use std::sync::{Arc, Mutex};
    for source_effect in [0, 6] {
        for target_effect in [0, 6] {
            let bundle = roundtrip(&rgb_app(RgbSupportKind::QmkRgblight, source_effect));
            let mut target = rgb_app(RgbSupportKind::QmkRgblight, target_effect);
            target.rgb_settings.brightness = 1;
            let state = Arc::new(Mutex::new([target_effect as u8, 1, 0, 0, 0]));
            let device_state = state.clone();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.respond_by(move |r| {
                let mut state = device_state.lock().unwrap();
                let mut reply = [0; 32];
                match &r[..2] {
                    [7, 0x81] => state[0] = r[2],
                    [7, 0x80] if state[0] == 1 => state[1] = r[2],
                    [7, 0x83] if state[0] == 1 => {
                        state[3] = r[2];
                        state[4] = r[3];
                    }
                    [7, 0x82] => state[2] = r[2],
                    [7, 0x80 | 0x83] => {}
                    [8, 0x81] => reply[2] = state[0],
                    [8, 0x80] => reply[2] = state[1],
                    [8, 0x82] => reply[2] = state[2],
                    [8, 0x83] => {
                        reply[2] = state[3];
                        reply[3] = state[4];
                    }
                    _ => return None,
                }
                Some(Ok(reply))
            });
            target.hid_device = Some(hid.into());
            let (errors, _, report) = target.apply_entlayout(&bundle).unwrap();
            assert!(errors.is_empty(), "{errors:?}");
            assert_eq!(report.failed, 0);
            assert_eq!(
                *state.lock().unwrap(),
                [source_effect as u8, 80, 42, 17, 200]
            );
            assert!(recorder.requests().iter().any(|r| r[0] == 9));
        }
    }
}

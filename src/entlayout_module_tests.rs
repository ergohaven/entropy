use super::tests::module_app;
use super::*;

fn phenom_settings_app(rmk: bool) -> EntropyApp {
    // Settings-only extracts from the pinned definitions in the filenames.
    let definition: serde_json::Value = serde_json::from_str(if rmk {
        include_str!("../tests/fixtures/phenom-rmk-settings-513718e9.json")
    } else {
        include_str!("../tests/fixtures/phenom-qmk-settings-f6d3d4d5.json")
    })
    .unwrap();
    let mut app = module_app();
    app.supported_qmk_settings = definition["settings"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["fields"].as_array().unwrap())
        .map(|field| field["qsid"].as_u64().unwrap() as u16)
        .collect();
    app.module_settings =
        EntropyApp::module_settings_from_definition(&definition, &app.supported_qmk_settings);
    for field in &app.module_settings.fields {
        app.module_settings.values.insert(field.qsid, field.min);
    }
    app
}

#[test]
fn entlayout_phenom_migrates_changed_module_qsids_by_contract() {
    let mut source = phenom_settings_app(false);
    let mappings = [
        (149, 327),
        (150, 328),
        (151, 329),
        (152, 330),
        (327, 151),
        (328, 152),
    ];
    source
        .module_settings
        .values
        .extend(mappings.map(|(from, _)| (from, 1)));
    let mut bundle = roundtrip(&source);
    bundle
        .data
        .portable_settings
        .retain(|setting| mappings.iter().any(|(from, _)| setting.qsid == *from));
    let mut target = phenom_settings_app(true);
    target.module_settings.values.extend([(149, 3), (150, 2)]);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.imported, 6);
    assert_eq!(report.incompatible, 0);
    let sets: Vec<_> = recorder
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
    assert_eq!(sets, mappings.map(|(_, to)| (to, 1)));
    for (_, to) in mappings {
        assert_eq!(target.module_settings.values[&to], 1);
    }
    assert_eq!(target.module_settings.values[&149], 3);
    assert_eq!(target.module_settings.values[&150], 2);
}

#[test]
fn entlayout_phenom_migrates_timeout_duration() {
    for (from, to) in [(0, 4), (1, 9), (2, 10), (3, 11), (4, 12), (5, 13)] {
        let mut source = phenom_settings_app(false);
        source.module_settings.values.insert(324, from);
        let mut bundle = roundtrip(&source);
        bundle
            .data
            .portable_settings
            .retain(|setting| setting.qsid == 324);
        let mut target = phenom_settings_app(true);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid.into());
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.imported, 1, "old timeout index {from}");
        assert_eq!(target.module_settings.values[&324], to);
        assert!(recorder
            .requests()
            .iter()
            .any(|r| r[..6] == [0xfe, 0x0b, 0x44, 1, to as u8, 0]));
    }
}

#[test]
fn entlayout_phenom_manual_choices_survive_appended_auto_variant() {
    for qsid in [149, 150] {
        for value in 0..4 {
            let mut source = phenom_settings_app(true);
            source.module_settings.values.insert(qsid, value);
            let mut bundle = roundtrip(&source);
            bundle
                .data
                .portable_settings
                .retain(|setting| setting.qsid == qsid);
            let mut target = phenom_settings_app(true);
            for field in &mut target.module_settings.fields {
                if field.qsid == qsid {
                    field.variants.push("Auto".into());
                    field.max = 4;
                }
            }
            for group in &mut target.module_settings.groups {
                for field in &mut group.fields {
                    if field.qsid == qsid {
                        field.variants.push("Auto".into());
                        field.max = 4;
                    }
                }
            }
            target.module_settings.values.insert(qsid, 4);
            let (hid, _) = crate::hid::HidDevice::test_device();
            target.hid_device = Some(hid.into());
            let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
            assert_eq!(report.imported, 1, "module {qsid} value {value}");
            assert_eq!(target.module_settings.values[&qsid], value);
        }
    }
}

#[test]
fn entlayout_phenom_migration_rejects_changed_contracts_and_aliases() {
    let source = phenom_settings_app(false).entlayout_portable_settings_snapshot();
    let target = phenom_settings_app(true).entlayout_portable_settings_snapshot();
    let original = source.iter().find(|setting| setting.qsid == 149).unwrap();
    let mut aliases = vec![original.clone(), original.clone()];
    aliases[1].qsid = 327;
    let plan = plan_entlayout_portable_settings_import(&aliases, &target, &[]);
    assert!(plan.compatible.is_empty());
    assert_eq!(plan.report.incompatible, 2);
    for (key, value) in [
        ("group", serde_json::json!("Other modules")),
        ("side", serde_json::json!("right")),
        ("title", serde_json::json!("Different meaning")),
        ("bit", serde_json::json!(1)),
        ("width", serde_json::json!(2)),
        ("min", serde_json::json!(1)),
        ("max", serde_json::json!(1)),
    ] {
        let mut altered = serde_json::to_value(original).unwrap();
        altered["module"][0][key] = value;
        let altered = serde_json::from_value(altered).unwrap();
        let plan = plan_entlayout_portable_settings_import(&[altered], &target, &[]);
        assert!(plan.compatible.is_empty(), "altered {key}");
        assert_eq!(plan.report.incompatible, 1);
    }
    // A new 50 ms preset has no representation in the old six-value contract.
    let timeout = target.iter().find(|setting| setting.qsid == 324).unwrap().clone();
    let plan = plan_entlayout_portable_settings_import(&[timeout], &source, &[]);
    assert!(plan.compatible.is_empty());
    assert_eq!(plan.report.incompatible, 1);
}

#[test]
fn entlayout_phenom_migration_preserves_destination_bits_and_confirmation() {
    let mut source = phenom_settings_app(false).entlayout_portable_settings_snapshot()
        .into_iter().find(|setting| setting.qsid == 149).unwrap();
    source.value = 0x81;
    let mut target = phenom_settings_app(true).entlayout_portable_settings_snapshot();
    target.iter_mut().find(|setting| setting.qsid == 327).unwrap().value = 0x40;
    let plan = plan_entlayout_portable_settings_import(&[source.clone()], &target, &[]);
    assert_eq!(plan.compatible.len(), 1);
    assert_eq!((plan.compatible[0].qsid, plan.compatible[0].value), (327, 0x41));
    target.iter_mut().find(|setting| setting.qsid == 327).unwrap().value = 0x41;
    let plan = plan_entlayout_portable_settings_import(&[source.clone()], &target, &[]);
    assert!(plan.compatible.is_empty());
    assert_eq!(plan.report.imported, 1);
    let plan = plan_entlayout_portable_settings_import(&[source], &target, &[327]);
    assert_eq!(plan.compatible.len(), 1, "confirm the destination register, not source 149");
}

#[test]
fn entlayout_phenom_relocated_settings_survive_return_to_qmk() {
    let mappings = [(327, 149), (328, 150), (329, 151), (330, 152), (151, 327), (152, 328)];
    let mut source = phenom_settings_app(true).entlayout_portable_settings_snapshot();
    source.retain(|setting| mappings.iter().any(|(from, _)| *from == setting.qsid));
    for setting in &mut source { setting.value = 1; }
    let target = phenom_settings_app(false).entlayout_portable_settings_snapshot();
    let plan = plan_entlayout_portable_settings_import(&source, &target, &[]);
    assert_eq!(plan.compatible.len(), 6);
    for setting in plan.compatible {
        assert!(mappings.iter().any(|(_, to)| *to == setting.qsid));
        assert_eq!(setting.value, 1);
    }
}

fn roundtrip(app: &EntropyApp) -> EntLayoutFile {
    serde_json::from_str(&serde_json::to_string(&app.entlayout_snapshot().unwrap()).unwrap())
        .unwrap()
}

#[test]
fn entlayout_module_contract_mutations_never_write() {
    let mut source = module_app();
    source.module_settings.values.insert(140, 1);
    let mut json = serde_json::to_value(roundtrip(&source)).unwrap();
    json["data"]["portable_settings"]
        .as_array_mut()
        .unwrap()
        .retain(|s| s["qsid"] == 140);
    for (key, value) in [
        ("group", serde_json::json!("Right modules")),
        ("side", serde_json::json!("right")),
        ("title", serde_json::json!("Different meaning")),
        ("kind", serde_json::json!("unknown")),
        ("width", serde_json::json!(2)),
        ("bit", serde_json::json!(1)),
        ("min", serde_json::json!(1)),
        ("max", serde_json::json!(1)),
        ("variants", serde_json::json!(["Off", "On"])),
    ] {
        let mut altered = json.clone();
        altered["data"]["portable_settings"][0]["module"][0][key] = value;
        let bundle: EntLayoutFile = serde_json::from_value(altered).unwrap();
        let mut target = module_app();
        let before = target.module_settings.values.clone();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid.into());
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.incompatible, 1, "contract mutation {key}");
        assert_eq!(target.module_settings.values, before);
        assert!(
            !recorder.requests().iter().any(|r| r[..2] == [0xfe, 0x0b]),
            "no SET for {key}"
        );
    }
}

#[test]
fn entlayout_module_unloaded_unsupported_and_out_of_range_skip() {
    let mut source = module_app();
    source
        .module_settings
        .values
        .extend([(140, 1), (141, 1), (143, 99)]);
    let bundle = roundtrip(&source);
    assert!(!bundle.data.portable_settings.iter().any(|s| s.qsid == 143));
    let mut target = module_app();
    target.module_settings.values.remove(&140);
    target.supported_qmk_settings.retain(|id| *id != 141);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.unsupported, 2);
    assert!(!recorder.requests().iter().any(|r| r[..2] == [0xfe, 0x0b]));
}

#[test]
fn entlayout_module_old_files_retain_module_cache_and_standard_portable() {
    let mut app = module_app();
    app.module_settings.values.extend([(140, 1), (141, 1)]);
    app.supported_qmk_settings.push(7);
    app.tap_hold_settings.set_qsid_loaded(7);
    app.tap_hold_settings.tapping_term = 250;
    let mut json = serde_json::to_value(roundtrip(&app)).unwrap();
    json["data"]["portable_settings"]
        .as_array_mut()
        .unwrap()
        .retain(|s| s["module"].is_null());
    let before = app.module_settings.values.clone();
    for version in 1..=7 {
        json["version"] = serde_json::json!(version);
        let bundle: EntLayoutFile = serde_json::from_value(json.clone()).unwrap();
        app.tap_hold_settings.tapping_term = 200;
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        let (_, _, report) = app.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(app.tap_hold_settings.tapping_term, 250);
        assert_eq!(app.module_settings.values, before);
        assert!(recorder
            .requests()
            .iter()
            .filter(|r| r[..2] == [0xfe, 0x0b])
            .all(|r| r[2..4] == [7, 0]));
    }
    json["data"]
        .as_object_mut()
        .unwrap()
        .remove("portable_settings");
    let bundle: EntLayoutFile = serde_json::from_value(json).unwrap();
    let (_, _, report) = app.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.total(), 0);
    assert_eq!(app.module_settings.values, before);
}

#[test]
fn entlayout_module_transport_failure_keeps_committed_cache() {
    let mut source = module_app();
    source.module_settings.values.insert(140, 1);
    let bundle = roundtrip(&source);
    let mut target = module_app();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(|request| {
        (request[..2] == [0xfe, 0x0b]).then(|| Err("scripted SET failure".to_owned()))
    });
    target.hid_device = Some(hid.into());
    let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.failed, 1);
    assert!(!failures.is_empty());
    assert_eq!(target.module_settings.values.get(&140), Some(&0));
}

#[test]
fn entlayout_module_synthetic_packed_boolean_and_u16_roundtrip_once_per_qsid() {
    // Deliberately synthetic: two booleans share a 16-bit packed register;
    // an integer and ordered select exercise definitions not in the K:04 fixture.
    let definition = serde_json::json!({"settings":[{"name":"Left modules","fields":[
        {"type":"boolean","title":"Visible","qsid":500,"width":2,"bit":9,"layoutOption":0},
        {"type":"boolean","title":"Sticky","qsid":500,"width":2,"bit":2},
        {"type":"integer","title":"Gain","qsid":501,"width":2,"min":10,"max":2000},
        {"type":"select","title":"Mode","qsid":502,"variants":["A","B","C"]}
    ]}]});
    let mut source = module_app();
    source.supported_qmk_settings = vec![500, 501, 502];
    source.module_settings =
        EntropyApp::module_settings_from_definition(&definition, &source.supported_qmk_settings);
    source
        .module_settings
        .values
        .extend([(500, 0x8204), (501, 1234), (502, 2)]);
    source.layout.as_mut().unwrap().layout_options = vec![crate::keyboard::LayoutOption {
        label: "Managed module visibility".to_owned(),
        choices: Vec::new(),
    }];
    source.layout_options_value = Some(0);
    let bundle = roundtrip(&source);
    let mut target = module_app();
    target.supported_qmk_settings = source.supported_qmk_settings.clone();
    target.module_settings =
        EntropyApp::module_settings_from_definition(&definition, &target.supported_qmk_settings);
    target
        .module_settings
        .values
        .extend([(500, 0x4000), (501, 10), (502, 0)]);
    target.layout.as_mut().unwrap().layout_options =
        source.layout.as_ref().unwrap().layout_options.clone();
    target.layout_options_value = Some(0);
    target.current_device_name = "Synthetic RMK".to_owned();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.imported, 3);
    assert_eq!(
        bundle.data.portable_settings[0].value, 0x8204,
        "export retains original raw word"
    );
    assert_eq!(
        target.module_settings.values[&500], 0x4204,
        "restore only known bits; preserve target-reserved bits"
    );
    assert_eq!(target.module_settings.values[&501], 1234);
    assert_eq!(target.module_settings.values[&502], 2);
    assert_eq!(
        target.layout_options_value,
        Some(1),
        "derived managed layout must use committed module word, including RMK early return"
    );
    let sets = recorder
        .requests()
        .into_iter()
        .filter(|r| r[..2] == [0xfe, 0x0b])
        .map(|r| r[..6].to_vec())
        .collect::<Vec<_>>();
    assert_eq!(
        sets,
        vec![
            vec![0xfe, 0x0b, 0xf4, 1, 4, 0x42],
            vec![0xfe, 0x0b, 0xf5, 1, 0xd2, 4],
            vec![0xfe, 0x0b, 0xf6, 1, 2, 0]
        ]
    );
    // Diagnostic GET is separate from the production writer's policy.
    assert_eq!(
        target
            .hid_device
            .as_ref()
            .unwrap()
            .get_qmk_setting_u16(500)
            .unwrap(),
        0x4204
    );
    let before = recorder
        .requests()
        .iter()
        .filter(|r| r[..2] == [0xfe, 0x0b])
        .count();
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.imported, 3);
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|r| r[..2] == [0xfe, 0x0b])
            .count(),
        before,
        "raw reserved differences alone must not cause another SET"
    );
}

#[test]
fn entlayout_module_all_real_k04_loaded_fields_roundtrip() {
    let definition: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
    let mut source = module_app();
    source.supported_qmk_settings = definition["settings"]
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .flat_map(|g| g["fields"].as_array().unwrap())
        .map(|f| f["qsid"].as_u64().unwrap() as u16)
        .collect();
    source.module_settings =
        EntropyApp::module_settings_from_definition(&definition, &source.supported_qmk_settings);
    let mut target = module_app();
    target.supported_qmk_settings = source.supported_qmk_settings.clone();
    target.module_settings =
        EntropyApp::module_settings_from_definition(&definition, &target.supported_qmk_settings);
    for field in &source.module_settings.fields {
        let current = if field.kind == ModuleSettingKind::Integer {
            field.min
        } else {
            0
        };
        source
            .module_settings
            .values
            .insert(field.qsid, current + 1);
        target.module_settings.values.insert(field.qsid, current);
    }
    let bundle = roundtrip(&source);
    assert_eq!(
        bundle.data.portable_settings.len(),
        source.module_settings.values.len()
    );
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    target.hid_device = Some(hid.into());
    let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
    assert_eq!(report.imported, source.module_settings.values.len());
    assert_eq!(target.module_settings.values, source.module_settings.values);
    let sets = recorder
        .requests()
        .into_iter()
        .filter(|r| r[..2] == [0xfe, 0x0b])
        .collect::<Vec<_>>();
    assert_eq!(sets.len(), source.module_settings.values.len());
    for request in sets {
        let qsid = u16::from_le_bytes([request[2], request[3]]);
        assert_eq!(
            u16::from_le_bytes([request[4], request[5]]),
            source.module_settings.values[&qsid]
        );
    }
}

#[test]
fn entlayout_phenom_led_minutes_migrate_to_exact_seconds_and_back() {
    let presets = [0, 60, 120, 300, 600, 900, 1800, 3600];
    let minutes = |value| EntPortableSetting {
        id: "led_timeout:minutes".into(), qsid: 317,
        width: EntPortableSettingWidth::U8, value,
        variants: ["max:7", "Never", "1 min", "2 min", "5 min", "10 min", "15 min", "30 min", "60 min"]
            .map(String::from).to_vec(), module: None,
    };
    let seconds = |value, max| EntPortableSetting {
        id: "led_timeout:seconds".into(), qsid: 317,
        width: if max == 255 { EntPortableSettingWidth::U8 } else { EntPortableSettingWidth::U16 },
        value, variants: vec![format!("max:{max}")], module: None,
    };
    for (index, value) in presets.into_iter().enumerate() {
        let source = minutes(index as u16);
        let target = seconds(42, 3600);
        let plan = plan_entlayout_portable_settings_import(&[source.clone()], &[target.clone()], &[]);
        assert_eq!(plan.compatible.len(), 1, "preset {index}");
        assert_eq!(plan.compatible[0].value, value);
        assert_eq!(plan.compatible[0].width, EntPortableSettingWidth::U16);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        write_entlayout_portable_setting(&hid.into(), &plan.compatible[0]).unwrap();
        assert!(recorder.requests().iter().any(|r| r[..6] == [0xfe, 0x0b, 61, 1, value as u8, (value >> 8) as u8]));
        let plan = plan_entlayout_portable_settings_import(&[seconds(value, 3600)], &[minutes(7)], &[]);
        if index == 7 { assert!(plan.compatible.is_empty()); assert_eq!(plan.report.imported, 1); }
        else { assert_eq!(plan.compatible[0].value, index as u16); }
        let old = plan_entlayout_portable_settings_import(&[source], &[seconds(42, 255)], &[]);
        assert_eq!(old.compatible.len(), usize::from(value <= 255));
    }
    let plan = plan_entlayout_portable_settings_import(&[seconds(601, 3600)], &[minutes(0)], &[]);
    assert_eq!(plan.report.incompatible, 1, "never round an explicit duration");
    for value in [0, 42, 255] {
        let plan = plan_entlayout_portable_settings_import(&[seconds(value,255)], &[seconds(600,3600)], &[]);
        assert_eq!(plan.compatible[0].value, value, "existing explicit RMK seconds");
        let plan = plan_entlayout_portable_settings_import(&[seconds(value,3600)], &[seconds(1,255)], &[]);
        assert_eq!(plan.compatible[0].value, value, "representable downgrade");
    }
    assert_eq!(plan_entlayout_portable_settings_import(&[seconds(600,3600)], &[seconds(1,255)], &[]).report.incompatible, 1);
    let mut bad = minutes(4);
    bad.variants.swap(2, 3);
    assert_eq!(plan_entlayout_portable_settings_import(&[bad], &[seconds(42,3600)], &[]).report.incompatible, 1);
}

#[test]
fn entlayout_phenom_led_migration_uses_parsed_firmware_contracts() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(
        include_str!("../tests/fixtures/phenom-led-timeout-contracts.json")
    ).unwrap();
    let snapshot = |index: usize, value: u16| {
        let mut app = module_app();
        app.supported_qmk_settings = vec![317];
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(move |request| {
            (request[..2] == [0xfe, 0x0a]).then(|| {
                let mut response = [0; 32];
                response[1..3].copy_from_slice(&value.to_le_bytes());
                Ok(response)
            })
        });
        app.layer_led_settings = EntropyApp::read_layer_led_settings(
            &cases[index]["definition"], &[317], 4, &hid,
        );
        app.entlayout_portable_settings_snapshot()
    };
    let wide = snapshot(2, 1);
    assert_eq!(wide.len(), 1);
    assert_eq!(wide[0].id, "led_timeout:seconds");
    assert_eq!(wide[0].width, EntPortableSettingWidth::U16);
    let qmk = snapshot(0, 4);
    assert_eq!(qmk[0].id, "led_timeout:minutes");
    let plan = plan_entlayout_portable_settings_import(&qmk, &wide, &[]);
    assert_eq!(plan.compatible[0].value, 600);
    let old = snapshot(1, 42);
    assert_eq!(old[0].id, "led_timeout:seconds");
    assert_eq!(plan_entlayout_portable_settings_import(&old, &wide, &[]).compatible[0].value, 42);
}

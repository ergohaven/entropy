use super::tests::module_app;
use super::*;

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

// Production encoder/decoder; only the OS HID boundary is scripted.
#[cfg(target_os = "linux")]
#[test]
fn sm62_failed_live_qmk_query_cannot_certify_default_labels_or_preimport_backup() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut source, session, source_recorder) = scripted_export_device_app(|_| None, true);
    resident_export_peer(&source_recorder, "");
    let json = export_entlayout(&mut source, 0, &session, deadline(), &AtomicBool::default()).unwrap();
    let mut input: serde_json::Value = serde_json::from_str(&json).unwrap();
    input["version"] = 7.into();
    for field in ["application_layouts", "application_categories", "pictograms"] {
        input.as_object_mut().unwrap().remove(field);
    }
    let input_path = storage.path().join("valid-input.entlayout");
    std::fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();

    let (mut app, _, _) = scripted_export_device_app(|_| None, true);
    app.headless = false;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    resident_export_peer(&recorder, "qmk-query");
    app.connect_hid = Some(hid);
    let ctx = egui::Context::default();
    app.start_connect(0);
    let mut connection = ExportConnection::default();
    let end = deadline();
    loop {
        app.poll_vial_hid_task(&ctx);
        app.poll_connect(&ctx);
        if connection.step(&mut app, &ctx).unwrap() { break; }
        assert!(Instant::now() < end, "connect never completed: {}", app.status_msg);
        std::thread::yield_now();
    }
    app.deferred_full_layout_action = None; // consumed by this modeled GUI connect
    assert!(recorder.requests().iter().any(|p| p[..2] == [0xfe, 0x09]));
    assert!(!app.supported_qmk_settings.contains(&200));
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    let profile = settings.layouts.get_mut("default").unwrap();
    profile.layers = EntropyApp::base_application_layers(app.layout.as_ref().unwrap()).to_vec();
    profile.layer_names = app.layer_names.clone();
    profile.layer_names[0] = "Draft".into();
    let key = app.application_layout_device_key().unwrap();
    app.app_settings.application_layouts.insert(key, settings);
    let cursor = recorder.requests().len();
    assert!(app.entlayout_export_json().unwrap().is_err(),
        "failed live QUERY must not turn an absent cached QSID into host-only metadata");
    let destination = storage.path().join("valuable.entlayout");
    std::fs::write(&destination, b"previous complete backup").unwrap();
    app.write_entlayout_export(&destination);
    assert_eq!(std::fs::read(&destination).unwrap(), b"previous complete backup");
    app.vial_unlocked = Some(true);
    app.begin_entlayout_import(input_path);
    let end = deadline();
    while app.import_pending() || app.vial_hid_task_active() {
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.1);
        assert!(Instant::now() < end, "unknown-capability import never terminated");
        std::thread::yield_now();
    }
    assert!(app.import_report_open);
    assert!(recorder.requests()[cursor..].iter().all(|p|
        !matches!(p[0], 0x05 | 0x13 | 0x0f)
            && p[..2] != [0xfe, 0x0b]), "unknown Default labels allowed a device SET");
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_complete_export_keeps_nonresident_author_profiles_without_publishing() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
    resident_export_peer(&recorder, "");
    let json = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    settings.layouts = serde_json::from_value(baseline["application_layouts"].clone()).unwrap();
    let mut authored = settings.layouts["default"].clone();
    authored.id = "local-author-only".into();
    authored.name = "Local author only".into();
    authored.layers[15][61] = 6;
    settings.layouts.insert(authored.id.clone(), authored.clone());
    let key = app.application_layout_device_key().unwrap();
    app.app_settings.application_layouts.insert(key, settings);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let json = app.entlayout_export_json().unwrap().expect("complete backup must preserve nonresident author data");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["application_layouts"].as_object().unwrap().len(), 4);
    assert_eq!(value["application_layouts"]["local-author-only"], serde_json::to_value(authored).unwrap());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_resident_visual_full_cannot_be_silently_downgraded() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
    resident_export_peer(&recorder, "visual");
    let result = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default());
    assert!(result.is_err(), "a FULL visual ID absent from the portable schema must not be silently dropped");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("previous.entlayout");
    std::fs::write(&path, b"previous full backup").unwrap();
    app.write_entlayout_export(&path);
    assert_eq!(std::fs::read(&path).unwrap(), b"previous full backup");
    assert!(recorder.requests().iter().any(|p| p[..3] == [0xef, 9, 6]));
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_headless_cancel_during_resident_read_cannot_return_success() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
    let cancelled = std::sync::Arc::new(AtomicBool::default());
    let signal = cancelled.clone();
    resident_export_peer_observing(&recorder, "", move |p| {
        if p[..3] == [0xef, 9, 6] { signal.store(true, Ordering::Relaxed); }
    });
    let result = export_entlayout(&mut app, 0, &session, deadline(), &cancelled);
    assert!(cancelled.load(Ordering::Relaxed), "fixture never reached resident READ");
    assert!(matches!(result, Err(ExportError::Failed(ref error)) if error.contains("cancelled")),
        "cancelled resident export must not return a successful backup");
    let reads = recorder.requests().iter().filter(|p| p[..3] == [0xef, 9, 6]).count();
    assert_eq!(reads, 1, "cancel must be checked before the next resident request");
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_resident_export_refuses_loss_of_authored_profiles_and_default_drafts() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for case in ["resident-draft", "default-draft", "supported-default-name-conflict"] {
        let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
        resident_export_peer(&recorder, "");
        // Run the ordinary read-only preparation before introducing author data.
        let baseline = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();
        let baseline: serde_json::Value = serde_json::from_str(&baseline).unwrap();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        settings.layouts = serde_json::from_value(baseline["application_layouts"].clone()).unwrap();
        if case == "resident-draft" {
            settings.layouts.retain(|_, p| p.name != "device-editor");
        }
        let mut profile = settings.layouts["default"].clone();
        profile.id = match case {
            "nonresident" => "user-only-profile",
            "resident-draft" => "device-editor",
            _ => "default",
        }.into();
        profile.name = "valuable author draft".into();
        profile.layers = vec![vec![6; 62]; 16];
        if case == "supported-default-name-conflict" {
            profile.layers = settings.layouts["default"].layers.clone();
            profile.layer_names[0] = "valuable unsaved native name".into();
            app.supported_qmk_settings.push(200);
        }
        settings.layouts.insert(profile.id.clone(), profile);
        let key = app.application_layout_device_key().unwrap();
        app.app_settings.application_layouts.insert(key, settings);
        let before = serde_json::to_value(&app.app_settings.application_layouts).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("valuable.entlayout");
        std::fs::write(&path, b"previous complete backup").unwrap();
        // Until both physical and author versions can be represented losslessly,
        // explicit refusal is preferable to a falsely complete success.
        let error = app.entlayout_export_json().unwrap().expect_err(case);
        assert!(format!("{error:#}").contains("author"), "{case}: {error:#}");
        app.write_entlayout_export(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"previous complete backup", "{case}");
        assert_eq!(serde_json::to_value(&app.app_settings.application_layouts).unwrap(), before);
        assert!(recorder.requests().iter().all(|p| !is_write(p)), "{case}");
    }
}

#[cfg(target_os = "linux")]
fn cold_resident_full(id: &str, code: u16) -> Vec<u8> {
    cold_resident_full_with_visual(id, code, false)
}

#[cfg(target_os = "linux")]
fn cold_resident_full_with_visual(id: &str, code: u16, visual: bool) -> Vec<u8> {
    let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
    let mut profile = settings.layouts["default"].clone();
    profile.id = id.into(); profile.name = id.into();
    profile.layers = vec![vec![code; 62]; 16];
    settings.layouts.insert(id.into(), profile.clone());
    let mut snapshot = settings.snapshot_for_layout(&profile);
    if visual { snapshot.visuals[15][61] = 7; }
    let packets = crate::qmk_hid_host::test_application_layout_transmission_packets(&snapshot);
    assert_eq!(packets.len(), 210);
    let mut raw = [packets[0].as_slice(), packets[209].as_slice()].concat();
    for p in &packets[1..209] {
        match p[0] {
            0xe2 => raw.extend_from_slice(&p[5..5 + usize::from(p[4]) * 2]),
            0xe7 => raw.extend_from_slice(&p[5..5 + usize::from(p[4])]),
            0xe9 => raw.extend_from_slice(&p[4..10 + usize::from(p[5])]),
            0xe4 => raw.extend_from_slice(&p[3..4 + usize::from(p[3])]),
            _ => panic!("unexpected FULL frame"),
        }
    }
    raw
}

#[cfg(target_os = "linux")]
fn resident_export_peer(recorder: &crate::hid::TestHidRecorder, failure: &'static str) {
    resident_export_peer_observing(recorder, failure, |_| {});
}

#[cfg(target_os = "linux")]
fn resident_export_peer_observing(
    recorder: &crate::hid::TestHidRecorder,
    failure: &'static str,
    mut observe: impl FnMut(&[u8; 32]) + Send + 'static,
) {
    let records = [(7u8, cold_resident_full_with_visual("device-editor", 4, failure == "visual")),
        (99u8, cold_resident_full("device-terminal", 5))];
    let mut enum_counts = std::collections::BTreeMap::<u8, usize>::new();
    scripted_keyboard_answering_geometry(recorder, move |p| {
        observe(p);
        if failure == "qmk-query" && p[..2] == [0xfe, 0x09] {
            return Some(Err("injected live QMK settings query failure".into()));
        }
        if failure == "qmk-query" && p[..4] == [0xfe, 0x0a, 200, 0] {
            let mut r = [0; 32];
            r[1..7].copy_from_slice(b"Native");
            return Some(Ok(r));
        }
        if p[..3] == [0xeb, 8, 0xa5] && p[3] == 2 {
            let mut r = *p;
            r[2] = 0x5a;
            r[8..15].copy_from_slice(&[2, 100, 0xd0, 0x11, 16, 62, 22]);
            r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
            r[20..24].copy_from_slice(&15u32.to_le_bytes());
            r[24] = 2;
            return Some(Ok(r));
        }
        if p[..2] != [0xef, 9] { return None; }
        assert!(matches!(p[2], 3 | 6), "export attempted a resident mutation: {p:?}");
        let record = records.iter().find(|(slot, _)| *slot == p[3]);
        let mut r = *p; r[2] |= 0x80;
        if p[2] == 3 {
            *enum_counts.entry(p[3]).or_default() += 1;
            r[8..].fill(0);
            r[3] = if record.is_some() { 0 } else { 1 };
            if let Some((_, raw)) = record {
                r[8..28].copy_from_slice(&raw[43..63]);
                r[28..].copy_from_slice(&(!raw.iter().fold(!0u32, |mut crc, b| { crc ^= u32::from(*b); for _ in 0..8 { crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 }; } crc })).to_le_bytes());
                if failure == "generation" && enum_counts[&p[3]] > 1 { r[8] ^= 1; }
            }
        } else {
            if failure == "read" { return Some(Err("injected resident READ failure".into())); }
            let raw = &record.unwrap().1;
            let offset = usize::from(u16::from_le_bytes(p[8..10].try_into().unwrap()));
            let n = (raw.len() - offset).min(usize::from(p[10]));
            r[10] = n as u8; r[11..].fill(0);
            r[12..12+n].copy_from_slice(&raw[offset..offset+n]);
            if failure == "crc" && offset == 0 { r[12] ^= 1; }
        }
        Some(Ok(r))
    }, None, true);
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_cold_resident_fulls_export_all_profiles_without_host_cache_or_mutations() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
    resident_export_peer(&recorder, "");
    assert!(app.app_settings.application_layouts.is_empty());
    let json = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let profiles = value["application_layouts"].as_object().unwrap();
    assert_eq!(profiles.len(), 3, "cold export silently omitted device FULL records");
    for (name, code) in [("device-editor", 4), ("device-terminal", 5)] {
        let profile = profiles.values().find(|p| p["name"] == name).unwrap();
        assert_eq!(profile["layers"][15][61], code);
    }
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
    assert!(!crate::app::app_storage::app_settings_path().exists());
    // Ordinary GUI writer must use the same preparation, not its old host-only snapshot.
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("backup.entlayout");
    click_ordinary_export(&mut app, &path, &egui::Context::default());
    let gui: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(gui["application_layouts"], value["application_layouts"]);
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
}

#[cfg(target_os = "linux")]
#[test]
fn sm62_resident_read_crc_and_generation_failures_preserve_export_destination() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for failure in ["read", "crc", "generation"] {
        let (mut app, session, recorder) = scripted_export_device_app(|_| None, true);
        resident_export_peer(&recorder, failure);
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("backup.entlayout");
        std::fs::write(&path, b"valuable previous complete backup").unwrap();
        assert!(export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).is_err(), "{failure}");
        app.write_entlayout_export(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"valuable previous complete backup", "{failure}");
        assert!(app.entlayout_export_json().unwrap().is_err(), "{failure}");
        assert!(recorder.requests().iter().all(|p| !is_write(p)), "{failure}");
    }
}

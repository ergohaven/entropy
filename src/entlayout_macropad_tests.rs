//! Real layout/menu/file-dispatch code; only native picker and HID are replaced.
//! Each test owns an isolated output root, including settings and auto-backups.

#[test]
fn gui_export_recovers_uncertain_names_at_ready_entry() {
    gui_export_uncertain_names("ready", true);
}

#[test]
fn gui_export_uncertain_names_read_error_stays_fail_closed() {
    gui_export_uncertain_names("ready", false);
}

#[test]
fn gui_export_uncertain_names_waits_for_borrowed_hid() {
    gui_export_uncertain_names("borrowed", true);
}

#[test]
fn gui_export_uncertain_names_recovers_after_real_full_load() {
    gui_export_uncertain_names("cold", true);
}

fn gui_export_uncertain_names(mode: &str, recovered: bool) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.current_device_name = "Macropad".into();
    app.app_settings.language = crate::i18n::Language::English;
    app.deferred_device_load = DeferredDeviceLoadState::complete(8);
    for profile in app.application_layout_settings_mut().unwrap().layouts.values_mut() {
        profile.seed_unset_keycodes([[4; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
            crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT]);
    }
    app.app_settings.application_layouts.insert("unrelated-device".into(), DeviceApplicationLayouts::default());
    let fail_get = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let fail = fail_get.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut gate = Some(release_rx);
    let mut name = String::new();
    let held_opcode = if mode == "borrowed" { 0xfe } else { 0x12 };
    recorder.respond_by(move |r| {
        if (r[0] == held_opcode && (r[0] != 0xfe || r[1] == 0x08)) && gate.is_some() {
            entered_tx.send(()).unwrap();
            gate.take().unwrap().recv_timeout(std::time::Duration::from_secs(3)).unwrap();
            return Some(Ok([0; 32]));
        }
        if r[..4] == [0xfe, 0x0b, 200, 0] {
            let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
            name = String::from_utf8(r[4..4+end].to_vec()).unwrap();
            Some(Ok([0; 32]))
        } else if r[..4] == [0xfe, 0x0a, 200, 0] {
            if fail.load(std::sync::atomic::Ordering::SeqCst) { return Some(Ok([0xff; 32])); }
            let mut response = [0; 32];
            response[1..1+name.len()].copy_from_slice(name.as_bytes());
            Some(Ok(response))
        } else if r[0] == 0x12 { Some(Ok([0; 32])) } else { None }
    });
    let ctx = egui::Context::default();
    // Finish ordinary first-frame binding/catalog initialization before taking
    // preservation snapshots; export is not responsible for those migrations.
    app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
    app.rename_application_layout_layer(DEFAULT_APPLICATION_LAYOUT_ID, 0, "Draft".into());
    app.poll_vial_hid_task(&ctx);
    assert!(app.defer_exit_until_hid_write_returns(&ctx));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.hid_write_lifecycle_busy() {
        app.poll_vial_hid_task(&ctx);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    app.finish_deferred_exit_after_hid_write(&ctx);
    assert!(!app.exit_after_hid_write, "failed readback blocked exit drain");
    assert!(app.selected_layer_names_uncertain());
    assert!(app.entlayout_snapshot().is_none());
    assert!(app.write_entlayout_auto_backup().is_err());
    save_app_settings(&app.app_settings);
    let settings = serde_json::to_value(&app.app_settings).unwrap();
    let persisted = std::fs::read(app_settings_path()).unwrap();
    let path = storage.path().join("recovered.entlayout");
    std::fs::write(&path, b"existing destination").unwrap();
    let reads = recorder.requests().iter().filter(|r| r[..2] == [0xfe, 0x0a]).count();
    fail_get.store(!recovered, std::sync::atomic::Ordering::SeqCst);
    if mode == "cold" {
        app.deferred_device_load = DeferredDeviceLoadState::staged(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(vec![]),
            definition_fingerprint: 42, layer_count: 8, rows: 5, cols: 3, encoder_count: 1,
            macro_count: 0, macro_memory_bytes: Some(0), tap_dance_count: 0, combo_count: 0,
            key_override_count: 0, alt_repeat_count: 0, modules_supported: false, touchpad_supported: false,
            bluetooth_supported: false, layer_leds_supported: false, rgb_supported: false, lighting_mode: None,
            supports_rmk_native_key_actions: false, supports_universal_symbols: false,
            supports_universal_russian_letters: false, supports_rmk_native_combo_output: false,
            supports_rmk_native_tap_dance_actions: false, supports_rmk_combo_layers: false,
        });
    } else if mode == "borrowed" {
        assert_eq!(app.start_vial_hid_operation(&ctx, crate::app::vial_hid_task::VialHidOperation::Lock),
            crate::app::vial_hid_task::VialHidTaskStart::Started);
    }
    crate::app::file_dialog::set_test_dialog_result(Some(path.clone()));
    app.request_entlayout_export_after_full_load();
    if mode != "ready" {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
            if entered_rx.try_recv().is_ok() { break; }
            assert!(std::time::Instant::now() < deadline, "deferred read never started: {}", app.status_msg);
            std::thread::yield_now();
        }
        assert!(app.hid_device.is_none());
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
        // Release before assertions so a failing regression cannot strand a worker.
        release_tx.send(()).unwrap();
        assert!(app.deferred_full_layout_action_pending(), "export discarded while HID was borrowed: {}", app.status_msg);
        assert!(app.pending_file_dialog.is_none());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.deferred_full_layout_action_pending() || app.vial_hid_task_active() {
            app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
            assert!(std::time::Instant::now() < deadline, "export never finished: {}", app.status_msg);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    assert!(recorder.requests().iter().filter(|r| r[..2] == [0xfe, 0x0a]).count() > reads,
        "normal export never reached read-only recovery: {}", app.status_msg);
    if recovered {
        assert!(app.pending_file_dialog.is_some(), "recovery did not reach picker: {}", app.status_msg);
        assert!(!app.selected_layer_names_uncertain());
        assert_eq!(app.layer_names[0], "Draft");
        app.poll_file_dialog(&ctx);
        let bundle: EntLayoutFile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        app.validate_entlayout_file(&bundle).unwrap();
        assert_eq!(bundle.data.layer_names[0], "Draft");
    } else {
        assert!(app.pending_file_dialog.is_none());
        assert!(app.status_msg.contains("Layer name not saved"), "{}", app.status_msg);
        assert!(app.selected_layer_names_uncertain());
        assert!(app.entlayout_snapshot().is_none());
        assert!(app.write_entlayout_auto_backup().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing destination");
    }
    assert_eq!(recorder.requests().iter().filter(|r| r[..2] == [0xfe, 0x0b]).count(), 1, "automatic SET retry");
    assert!(!recorder.requests().iter().any(|r| matches!(r[0], 0x05 | 0x07 | 0x0f)), "export mutated device data");
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), settings);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), persisted);
}

#[test]
fn uncertain_layer_name_blocks_unchecked_snapshot_and_auto_backup() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let ctx = egui::Context::default();
    app.deferred_device_load = DeferredDeviceLoadState::complete(16);
    recorder.respond_by(|request| {
        if request[..2] == [0xfe, 0x0b] { Some(Ok([0; 32])) }
        else if request[..2] == [0xfe, 0x0a] { Some(Ok([0xff; 32])) }
        else { None }
    });
    app.rename_application_layout_layer(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID, 0, "Draft".into());
    app.poll_vial_hid_task(&ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.hid_write_lifecycle_busy() {
        app.poll_vial_hid_task(&ctx);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(app.selected_layer_names_uncertain());
    assert!(app.entlayout_snapshot().is_none(), "unchecked snapshot treated uncertainty as current");
    assert!(app.write_entlayout_auto_backup().is_err());
}

#[test]
fn macropad_layer_name_get_failure_rolls_back_host_publication() {
    assert_layer_name_failure_rolls_back(false);
}

#[test]
fn macropad_layer_name_set_failure_rolls_back_host_publication() {
    assert_layer_name_failure_rolls_back(true);
}

fn assert_layer_name_failure_rolls_back(fail_set: bool) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    app.current_device_name = "M4CR0Pad v3".into();
    let bundle = app.entlayout_snapshot().unwrap();
    let path = storage.path().join("names.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .layers[0][0] = 99;
    save_app_settings(&app.app_settings);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let persisted = std::fs::read(app_settings_path()).unwrap();
    let names_path = layer_names_path(&app.current_device_name);
    std::fs::create_dir_all(names_path.parent().unwrap()).unwrap();
    std::fs::write(&names_path, b"old sidecar").unwrap();
    app.supported_qmk_settings = vec![200];
    let mut keys = std::collections::BTreeMap::new();
    recorder.respond_by(move |request| {
        if request[0] == 0xfe
            && matches!(request[1], 0x0a | 0x0b)
            && request[2..4] == 200u16.to_le_bytes()
        {
            if request[1] == if fail_set { 0x0b } else { 0x0a } {
                return Some(Err("injected layer-name failure".into()));
            }
            let mut response = *request;
            response[4..].fill(0);
            return Some(Ok(response));
        }
        match request[0] {
            0x05 => {
                keys.insert(
                    (request[1], request[2], request[3]),
                    [request[4], request[5]],
                );
                Some(Ok([0; 32]))
            }
            0x04 => {
                let mut response = [0; 32];
                if let Some(code) = keys.get(&(request[1], request[2], request[3])) {
                    response[4..6].copy_from_slice(code);
                }
                Some(Ok(response))
            }
            _ => None,
        }
    });
    let result = app.import_entlayout_from_path(&path);
    assert!(
        result.is_err(),
        "supported layer-name failure must not report complete: {result:?}"
    );
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), persisted);
    assert_eq!(std::fs::read(names_path).unwrap(), b"old sidecar");
    assert!(!macro_metadata_path(&app.current_device_name).exists());
}
#[test]
fn macropad_cold_json_export_discovers_and_reads_pictograms() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.display_settings.pictograms.supported = None;
    let mut library = PictogramLibrary::blank();
    library.set_colored(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES], [1, 2, 3]);
    let mut responses = std::collections::VecDeque::from(
        crate::app::pictograms::test_pictogram_read_responses(&library),
    );
    recorder.respond_by(move |request| {
        matches!(request[0], 0xc0 | 0xca | 0xcb)
            .then(|| Ok(responses.pop_front().expect("unexpected pictogram request")))
    });
    let json = app.entlayout_export_json().unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value["pictograms"][0]["bitmap"],
        serde_json::json!(vec![0x55; PICTOGRAM_BYTES])
    );
    assert_eq!(app.display_settings.pictograms.supported, Some(true));
    assert!(app.display_settings.pictograms.loaded);
    assert!(recorder.requests().iter().any(|request| request[0] == 0xc0));
}

#[test]
fn macropad_ordinary_import_retries_unconfirmed_capability_after_transport_recovers() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let bundle = app.entlayout_snapshot().unwrap();
    let input = storage.path().join("retry.entlayout");
    write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
    let original_input = std::fs::read(&input).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    save_app_settings(&app.app_settings);
    let saved = std::fs::read(app_settings_path()).unwrap();
    let info = app.device_about_info.as_mut().unwrap();
    info.supports_application_layouts = false;
    info.application_layout_probe_failed = true;
    let recovered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let transport = recovered.clone();
    let mut keys = std::collections::BTreeMap::new();
    recorder.respond_by(move |request| {
        let mut response = [0; 32];
        match request[0] {
            0xe6 => {
                assert_eq!(
                    &request[..5],
                    &[
                        0xe6,
                        crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                        0xa5,
                        0,
                        0
                    ]
                );
                assert!(request[5..].iter().all(|byte| *byte == 0));
                if !transport.load(std::sync::atomic::Ordering::SeqCst) {
                    return Some(Err("injected capability read failure".into()));
                }
                response[..3].copy_from_slice(&[
                    0xe6,
                    crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                    0x5a,
                ]);
            }
            0x05 => {
                keys.insert(
                    (request[1], request[2], request[3]),
                    [request[4], request[5]],
                );
            }
            0x04 => {
                if let Some(code) = keys.get(&(request[1], request[2], request[3])) {
                    response[4..6].copy_from_slice(code);
                }
            }
            _ => return None,
        }
        Some(Ok(response))
    });
    assert!(app.import_entlayout_from_path(&input).is_err());
    assert!(
        recorder.requests().iter().any(|r| r[0] == 0xe6),
        "ordinary import must reach read-only retry"
    );
    assert!(recorder.requests().iter().all(|r| r[0] == 0xe6));
    assert!(
        app.device_about_info
            .as_ref()
            .unwrap()
            .application_layout_probe_failed
    );
    assert!(app.application_layout_device_key().is_none());
    assert!(!storage.path().join("entropy/backups").exists());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), saved);
    assert_eq!(std::fs::read(&input).unwrap(), original_input);
    recovered.store(true, std::sync::atomic::Ordering::SeqCst);
    let report = app.import_entlayout_from_path(&input).unwrap();
    assert!(report.contains("Layout import complete"));
    assert!(
        !app.device_about_info
            .as_ref()
            .unwrap()
            .application_layout_probe_failed
    );
    assert!(app.application_layout_device_key().is_some());
    assert_eq!(
        app.application_layout_settings().unwrap().layouts,
        bundle.application_layouts.unwrap()
    );
    assert!(recorder.requests().iter().any(|r| r[0] == 0x05));
}

#[test]
fn macropad_ordinary_import_malformed_sources_never_probe_or_read_macro_capacity() {
    for (malformed, unconfirmed) in ["geometry", "profile", "resource", "keymap", "macro"]
        .into_iter()
        .flat_map(|malformed| [false, true].map(|unconfirmed| (malformed, unconfirmed)))
    {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, application) = fixture();
        app.keycode_picker.macro_count = 1;
        app.keycode_picker.macro_texts = vec!["x".into()];
        let mut bundle = app.entlayout_snapshot().unwrap();
        bundle.data.macros.bytecode_base64 = vec![BASE64_STANDARD.encode(b"x")];
        match malformed {
            "geometry" => bundle.data.source_layout.as_mut().unwrap().keys[0].w = 0.0,
            "profile" => {
                bundle
                    .application_layouts
                    .as_mut()
                    .unwrap()
                    .get_mut(&application)
                    .unwrap()
                    .layers
                    .pop();
            }
            "resource" => {
                bundle
                    .application_layouts
                    .as_mut()
                    .unwrap()
                    .get_mut(&application)
                    .unwrap()
                    .layers[0][0] = 0x77ff
            }
            "keymap" => {
                bundle.data.keymap[0].pop();
            }
            "macro" => bundle.data.macros.bytecode_base64 = vec!["invalid base64!".into()],
            _ => unreachable!(),
        }
        let input = storage.path().join("malformed.entlayout");
        write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
        let original = std::fs::read(&input).unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        let info = app.device_about_info.as_mut().unwrap();
        info.supports_application_layouts = !unconfirmed;
        info.application_layout_probe_failed = unconfirmed;
        assert!(
            app.import_entlayout_from_path(&input).is_err(),
            "{malformed}"
        );
        assert!(
            recorder.requests().is_empty(),
            "{malformed}: {:?}",
            recorder.requests()
        );
        assert!(!storage.path().join("entropy/backups").exists());
        assert!(!app_settings_path().exists());
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert_eq!(std::fs::read(input).unwrap(), original);
    }
}

#[test]
fn macropad_ordinary_import_does_not_retry_explicit_unsupported_or_wrong_identity() {
    for mismatch in ["unsupported", "vendor", "product", "path", "non-macropad"] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        let bundle = app.entlayout_snapshot().unwrap();
        let input = storage.path().join("refused.entlayout");
        write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
        let info = app.device_about_info.as_mut().unwrap();
        info.supports_application_layouts = false;
        info.application_layout_probe_failed = mismatch != "unsupported";
        match mismatch {
            "vendor" => info.vendor_id ^= 1,
            "product" => info.product_id ^= 1,
            "path" => info.path.push_str("-other"),
            "non-macropad" => {
                let mut device = app.device_manager.devices()[0].clone();
                device.product_id = 0x0071;
                device.name = "K:04".into();
                app.device_manager.replace_devices(vec![device]);
            }
            _ => {}
        }
        assert!(app.import_entlayout_from_path(&input).is_err());
        assert!(recorder.requests().is_empty(), "{mismatch}");
        assert!(app.application_layout_device_key().is_none());
        assert!(!storage.path().join("entropy/backups").exists());
        assert!(!app_settings_path().exists());
    }
}

#[test]
fn macropad_ordinary_import_retry_rejection_never_grants_capability() {
    for version in [
        crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
        255,
    ] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        let bundle = app.entlayout_snapshot().unwrap();
        let input = storage.path().join("rejected-probe.entlayout");
        write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        let info = app.device_about_info.as_mut().unwrap();
        info.supports_application_layouts = false;
        info.application_layout_probe_failed = true;
        recorder.respond_by(move |request| {
            (request[0] == 0xe6).then(|| {
                let mut response = [0; 32];
                response[..3].copy_from_slice(&[
                    0xe6,
                    version,
                    if version == 255 { 0x5a } else { 0xa5 },
                ]);
                Ok(response)
            })
        });
        assert!(app.import_entlayout_from_path(&input).is_err());
        assert_eq!(recorder.requests().len(), 1);
        assert_eq!(recorder.requests()[0][0], 0xe6);
        assert!(
            !app.device_about_info
                .as_ref()
                .unwrap()
                .application_layout_probe_failed
        );
        assert!(app.application_layout_device_key().is_none());
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert!(!storage.path().join("entropy/backups").exists());
        assert!(!app_settings_path().exists());
        assert!(app.import_entlayout_from_path(&input).is_err());
        assert_eq!(
            recorder.requests().len(),
            1,
            "explicit unsupported must not retry"
        );
    }
}

#[test]
fn macropad_cold_import_discovers_pictograms_before_validation_and_backup() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let mut library = PictogramLibrary::blank();
    library.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.pictograms = Some(vec![EntLayoutPictogram {
        tap_dance: false,
        slot: 0,
        bitmap: vec![0x55; PICTOGRAM_BYTES],
        color: library.color(PictogramKind::Macro, 0).unwrap(),
    }]);
    let path = storage.path().join("fresh.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    app.display_settings.pictograms.supported = None;
    let mut responses = crate::app::pictograms::test_pictogram_read_responses(&library);
    responses.extend(crate::app::pictograms::test_pictogram_read_responses(
        &library,
    ));
    recorder.respond_with(responses);
    let report = app.import_entlayout_from_path(&path).unwrap();
    assert!(report.contains("Layout import complete"));
    assert_eq!(app.display_settings.pictograms.supported, Some(true));
    assert_eq!(app.display_settings.pictograms.library, library);
    let backup = std::fs::read_dir(storage.path().join("entropy/backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(backup).unwrap()).unwrap();
    assert_eq!(
        value["pictograms"][0]["bitmap"],
        serde_json::json!(vec![0x55; PICTOGRAM_BYTES])
    );
}

#[test]
fn macropad_json_export_rejects_unportable_application_resources() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    app.display_settings.pictograms.supported = Some(false);
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .layers[15][14] = 0x77ff;
    assert!(
        app.entlayout_export_json().unwrap().is_err(),
        "headless serializer must enforce the same strict portability checks as the GUI writer"
    );
}

#[test]
fn macropad_cold_export_explicit_unsupported_is_not_a_transport_error() {
    for format_rejection in [false, true] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        app.display_settings.pictograms.supported = None;
        recorder.respond_by(move |request| {
            (request[0] == 0xc0).then(|| {
                let mut response = [0; 32];
                if format_rejection {
                    response[0] = 0xc0;
                    response[2] = 255;
                }
                Ok(response)
            })
        });
        let path = storage.path().join("unsupported.entlayout");
        app.write_entlayout_export(&path);
        assert!(path.exists(), "{}", app.status_msg);
        assert_eq!(app.display_settings.pictograms.supported, Some(false));
        assert!(!app.display_settings.pictograms.loaded);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(value.get("pictograms").is_none());
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| request[0] == 0xc0)
                .count(),
            1
        );
    }
}

#[test]
fn macropad_cold_query_and_read_errors_abort_export_and_import_without_publication() {
    for failure in [0xc0, 0xca, 0xcb] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        let mut library = PictogramLibrary::blank();
        library.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
        let bundle = app.entlayout_snapshot().unwrap();
        let input = storage.path().join("input.entlayout");
        write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
        save_app_settings(&app.app_settings);
        let settings = std::fs::read(app_settings_path()).unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        app.display_settings.pictograms.supported = None;
        let read_responses = crate::app::pictograms::test_pictogram_read_responses(&library);
        let mut responses = std::collections::VecDeque::from(read_responses.clone());
        recorder.respond_by(move |request| {
            if request[0] == failure {
                return Some(Err("injected pictogram failure".into()));
            }
            matches!(request[0], 0xc0 | 0xca | 0xcb).then(|| Ok(responses.pop_front().unwrap()))
        });
        let output = storage.path().join("existing.entlayout");
        std::fs::write(&output, b"existing backup").unwrap();
        app.write_entlayout_export(&output);
        assert_eq!(std::fs::read(&output).unwrap(), b"existing backup");
        assert!(app.status_msg.contains("failed"));
        assert_eq!(app.display_settings.pictograms.supported, None);
        assert!(!app.display_settings.pictograms.loaded);
        // Reset the stateful read sequence for an independent normal import.
        let mut responses = std::collections::VecDeque::from(read_responses);
        recorder.respond_by(move |request| {
            if request[0] == failure {
                return Some(Err("injected pictogram failure".into()));
            }
            matches!(request[0], 0xc0 | 0xca | 0xcb).then(|| Ok(responses.pop_front().unwrap()))
        });
        assert!(app.import_entlayout_from_path(&input).is_err());
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert_eq!(std::fs::read(app_settings_path()).unwrap(), settings);
        assert!(!storage.path().join("entropy/backups").exists());
        assert!(recorder
            .requests()
            .iter()
            .all(|request| matches!(request[0], 0xc0 | 0xca | 0xcb)));
    }
}

#[test]
fn macropad_pictogram_busy_preparation_rejects_without_hid_or_export_write() {
    for saving in [false, true] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        app.display_settings.pictograms.loading = !saving;
        app.display_settings.pictograms.saving = saving;
        let path = storage.path().join("busy.entlayout");
        app.write_entlayout_export(&path);
        assert!(!path.exists());
        assert!(recorder.requests().is_empty());
        assert!(app.entlayout_export_json().unwrap().is_err());
    }
}

#[test]
fn macropad_cold_export_without_prior_host_profiles_still_includes_device_pictograms() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.app_settings.application_layouts.clear();
    app.display_settings.pictograms.supported = None;
    let mut library = PictogramLibrary::blank();
    library.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    recorder.respond_with(crate::app::pictograms::test_pictogram_read_responses(
        &library,
    ));
    let value: serde_json::Value =
        serde_json::from_str(&app.entlayout_export_json().unwrap().unwrap()).unwrap();
    assert_eq!(value["version"], 9);
    assert!(value["application_layouts"][DEFAULT_APPLICATION_LAYOUT_ID].is_object());
    assert_eq!(value["data"]["layer_names"][0], "Firmware 0");
    assert_eq!(
        value["application_layouts"][DEFAULT_APPLICATION_LAYOUT_ID]["layer_names"][0],
        "Firmware 0"
    );
    assert_eq!(
        value["pictograms"][0]["bitmap"],
        serde_json::json!(vec![0x55; PICTOGRAM_BYTES])
    );
    assert!(!app_settings_path().exists());
}

#[test]
fn legacy_layer_name_failure_remains_an_ordinary_firmware_issue() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.version = ENTLAYOUT_VERSION;
    bundle.application_layouts = None;
    bundle.pictograms = None;
    app.supported_qmk_settings = vec![200];
    recorder.respond_by(|request| {
        (request[0] == 0xfe && request[1] == 0x0a && request[2] == 200)
            .then(|| Err("injected name read failure".into()))
    });
    let path = storage.path().join("legacy.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    let report = app.import_entlayout_from_path(&path).unwrap();
    assert!(report.contains("Firmware issues:"));
    assert!(report.contains("layer names (layers 0)"));
}

#[test]
fn macropad_cold_query_status_error_retains_unknown_and_allows_read_retry() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.display_settings.pictograms.supported = None;
    recorder.respond_by(|request| {
        (request[0] == 0xc0).then(|| {
            let mut response = [0; 32];
            response[0] = 0xc0;
            response[1] = 1;
            Ok(response)
        })
    });
    assert!(app.entlayout_export_json().unwrap().is_err());
    assert_eq!(app.display_settings.pictograms.supported, None);
    assert!(!app.display_settings.pictograms.loaded);
    recorder.respond_by(|_| None);
    let library = PictogramLibrary::blank();
    recorder.respond_with(crate::app::pictograms::test_pictogram_read_responses(
        &library,
    ));
    assert!(app.entlayout_export_json().unwrap().is_ok());
    assert_eq!(app.display_settings.pictograms.supported, Some(true));
    assert!(app.display_settings.pictograms.loaded);
}

use super::*;
use crate::application_layouts::{
    DetectedApplication, DeviceApplicationLayouts, DEFAULT_APPLICATION_LAYOUT_ID,
};

fn fixture() -> (EntropyApp, crate::hid::TestHidRecorder, String) {
    let mut app = EntropyApp::new_inert_for_test();
    // Most cases model firmware with explicitly unsupported custom icon storage.
    // Cold-connection regressions override this to None and script production reads.
    app.display_settings.pictograms.supported = Some(false);
    let device = Device {
        name: "M4CR0Pad v3".into(),
        vendor_id: 0xE126,
        product_id: 0x0042,
        manufacturer: "Ergohaven".into(),
        serial_number: "layout-test".into(),
        bus_type: "Usb".into(),
        path: "inert-entlayout-macropad".into(),
        instance_token: "layout-test".into(),
        firmware: FirmwareProtocol::Vial,
    };
    app.device_about_info = Some(DeviceAboutInfo {
        supports_application_layouts: true,
        vendor_id: device.vendor_id,
        product_id: device.product_id,
        path: device.path.clone(),
        ..Default::default()
    });
    app.device_manager.replace_devices(vec![device]);
    app.selected_device = Some(0);
    app.current_keyboard_id = Some(42);
    let mut layout = KeyboardLayout::from_vial_json(&serde_json::json!({
        "name": "M4CR0Pad v3", "matrix": {"rows": 5, "cols": 3},
        "layouts": {"keymap": [["1,0", "1,1", "0,2", "0,0\n\n\n\n\n\n\n\n\ne"]]}
    }))
    .unwrap();
    let mut other_direction = layout.encoders[0].clone();
    other_direction.direction = 1 - other_direction.direction;
    other_direction.x += 1.0;
    layout.encoders.push(other_direction);
    layout.layers = vec![vec![4.into(); layout.keys.len()]; 8];
    layout.encoder_layers = vec![vec![5; layout.encoders.len()]; 8];
    app.layout = Some(layout);
    app.layer_count = 8;
    app.layer_names = (0..8).map(|i| format!("Firmware {i}")).collect();
    app.keycode_picker.macro_texts.clear();
    app.vial_unlocked = Some(true);
    app.default_layout_device_reconciled = true;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    // Synthetic firmware memory, not a bypass of production writeback validation.
    let mut keys = std::collections::BTreeMap::new();
    recorder.respond_by(move |request| match request[0] {
        0x05 => {
            keys.insert(
                (request[1], request[2], request[3]),
                [request[4], request[5]],
            );
            Some(Ok([0; 32]))
        }
        0x04 => {
            let mut response = [0; 32];
            if let Some(code) = keys.get(&(request[1], request[2], request[3])) {
                response[4..6].copy_from_slice(code);
            }
            Some(Ok(response))
        }
        _ => None,
    });
    app.shared_hid_output = hid.shared_output();
    app.hid_device = Some(hid);
    let key = app.application_layout_device_key().unwrap();
    let mut settings = DeviceApplicationLayouts::default();
    let application = settings.create_for_application(&DetectedApplication {
        executable: "entropy".into(),
        display_name: "Entropy".into(),
        ..Default::default()
    });
    settings
        .layouts
        .get_mut(&application)
        .unwrap()
        .set_keycode(0, 0, 7);
    settings
        .layouts
        .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap()
        .set_keycode(0, 0, 6);
    settings
        .layouts
        .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap()
        .layer_names[0] = "My Default".into();
    app.app_settings.application_layouts.insert(key, settings);
    for id in [DEFAULT_APPLICATION_LAYOUT_ID, application.as_str()] {
        app.application_layout_settings_mut()
            .unwrap()
            .layouts
            .get_mut(id)
            .unwrap()
            .set_encoder_stack(
                0,
                vec![
                    crate::application_layouts::EncoderStackAction {
                        name: "Volume".into(),
                        counter_clockwise: 0x0104,
                        clockwise: 0x0105,
                    },
                    crate::application_layouts::EncoderStackAction {
                        name: "Scroll".into(),
                        counter_clockwise: 0x0204,
                        clockwise: 0x0205,
                    },
                ],
            );
    }
    (app, recorder, application)
}

#[test]
fn macropad_unified_export_contains_every_profile_independent_of_selection() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let expected = app.application_layout_settings().unwrap().layouts.clone();
    for selection in [DEFAULT_APPLICATION_LAYOUT_ID, application.as_str()] {
        app.application_layout_settings_mut()
            .unwrap()
            .editor_layout_id = selection.into();
        let value: serde_json::Value =
            serde_json::from_str(&app.entlayout_export_json().unwrap().unwrap()).unwrap();
        assert_eq!(value["version"], 9);
        assert_eq!(
            value["application_layouts"],
            serde_json::to_value(&expected).unwrap()
        );
    }
}

#[test]
fn macropad_unified_import_restores_profiles_matching_and_persisted_device_only() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let key = app.application_layout_device_key().unwrap();
    app.app_settings
        .application_layouts
        .insert("other-keyboard".into(), DeviceApplicationLayouts::default());
    let other_before = app.app_settings.application_layouts["other-keyboard"].clone();
    let source = app.application_layout_settings().unwrap().layouts.clone();
    let path = storage.path().join("all.entlayout");
    app.write_entlayout_export(&path);
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .remove(&application);
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap()
        .layers[15][14] = 42;
    let report = app.import_entlayout_from_path(&path).unwrap();
    assert!(report.contains("Application layouts: 2"), "{report}");
    assert!(
        !report.contains("Text Expander"),
        "global text expansion settings were not imported"
    );
    let reloaded: AppSettings = load_app_settings();
    for (id, expected) in source {
        let actual = &reloaded.application_layouts[&key].layouts[&id];
        assert_eq!(actual.layers, expected.layers);
        assert_eq!(actual.layer_names, expected.layer_names);
        assert_eq!(actual.encoder_stacks, expected.encoder_stacks);
        assert_eq!(actual.executable, expected.executable);
        assert_eq!(
            actual.application_identities,
            expected.application_identities
        );
        assert_eq!(actual.name, expected.name);
    }
    assert_eq!(reloaded.application_layouts["other-keyboard"], other_before);
}

#[test]
fn macropad_unified_invalid_profile_rejected_before_backup_persistence_or_hid() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let path = storage.path().join("invalid.entlayout");
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle
        .application_layouts
        .as_mut()
        .unwrap()
        .get_mut(&application)
        .unwrap()
        .layers
        .pop();
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert!(recorder.requests().is_empty());
    assert!(!storage.path().join("entropy/backups").exists());
    assert!(!app_settings_path().exists());
}

#[test]
fn macropad_unified_persistence_failure_keeps_memory_and_hid_untouched() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let path = storage.path().join("all.entlayout");
    app.write_entlayout_export(&path);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    std::fs::create_dir_all(app_settings_path()).unwrap();
    let error = app.import_entlayout_from_path(&path).unwrap_err();
    assert!(error
        .to_string()
        .contains("Failed to save Macropad layouts"));
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert!(recorder.requests().is_empty());
}

#[test]
fn macropad_unified_shared_macros_and_tap_dance_round_trip_every_profile() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.macro_texts = vec![b"macro body".to_vec()];
    app.keycode_picker.tap_dance_entries = vec![crate::keycode_picker::TapDanceEntry::default()];
    for profile in app
        .application_layout_settings_mut()
        .unwrap()
        .layouts
        .values_mut()
    {
        for (layer, codes) in profile.layers.iter_mut().enumerate() {
            codes[0] = 0x7700;
            codes[14] = 0x5700;
            profile.layer_names[layer] = format!("Resource {layer}");
        }
        profile.encoder_stacks[0][0].clockwise = 0x7700;
        profile.encoder_stacks[0][1].counter_clockwise = 0x5700;
    }
    let expected = app.application_layout_settings().unwrap().layouts.clone();
    let path = storage.path().join("resources.entlayout");
    app.write_entlayout_export(&path);
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .remove(&application);
    app.import_entlayout_from_path(&path).unwrap();
    let persisted: AppSettings =
        serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    let key = app.application_layout_device_key().unwrap();
    assert_eq!(persisted.application_layouts[&key].layouts, expected);
    assert_eq!(app.keycode_picker.macro_texts, vec![b"macro body".to_vec()]);
}

#[test]
fn macropad_unified_export_rejects_invalid_resource_without_overwriting_destination() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let path = storage.path().join("all.entlayout");
    std::fs::write(&path, b"keep").unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .layers[15][14] = 0x77ff;
    app.write_entlayout_export(&path);
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
}

#[test]
fn macropad_unified_geometry_and_shared_dependencies_are_validated_before_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let original = app.entlayout_snapshot().unwrap();
    let path = storage.path().join("invalid.entlayout");
    for variant in 0..7 {
        let mut bundle = original.clone();
        match variant {
            0 => bundle.keyboard.keys += 1,
            1 => bundle.keyboard.layers += 1,
            2 => bundle
                .application_layouts
                .as_mut()
                .unwrap()
                .get_mut(&application)
                .unwrap()
                .layer_names
                .pop()
                .map(|_| ())
                .unwrap(),
            3 => bundle
                .application_layouts
                .as_mut()
                .unwrap()
                .get_mut(&application)
                .unwrap()
                .encoder_stacks[15]
                .push(crate::application_layouts::EncoderStackAction {
                    name: "Invalid".into(),
                    counter_clockwise: 4,
                    clockwise: 5,
                }),
            4 => {
                bundle
                    .application_layouts
                    .as_mut()
                    .unwrap()
                    .get_mut(&application)
                    .unwrap()
                    .layers[15][14] = 0x77ff
            }
            5 => {
                bundle
                    .application_layouts
                    .as_mut()
                    .unwrap()
                    .get_mut(&application)
                    .unwrap()
                    .executable = "different".into()
            }
            _ => bundle.data.source_layout.as_mut().unwrap().keys[0].w = 0.0,
        }
        write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        assert!(
            app.import_entlayout_from_path(&path).is_err(),
            "variant {variant}"
        );
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert!(recorder.requests().is_empty());
        assert!(!storage.path().join("entropy/backups").exists());
        assert!(!app_settings_path().exists());
    }
}

#[test]
fn macropad_unified_remaps_custom_resources_in_every_key_and_encoder_stack() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    let custom = |name: &str| crate::keyboard::CustomKeycode {
        name: name.into(),
        label: name.into(),
        title: name.into(),
    };
    app.layout.as_mut().unwrap().custom_keycodes = vec![custom("First"), custom("Second")];
    for profile in app
        .application_layout_settings_mut()
        .unwrap()
        .layouts
        .values_mut()
    {
        for layer in &mut profile.layers {
            layer.fill(0x7e00);
        }
        for stack in &mut profile.encoder_stacks {
            *stack = vec![
                crate::application_layouts::EncoderStackAction {
                    name: "Resource".into(),
                    clockwise: 0x7e00,
                    counter_clockwise: 0x7e01
                };
                2
            ];
        }
    }
    let path = storage.path().join("custom.entlayout");
    app.write_entlayout_export(&path);
    app.layout.as_mut().unwrap().custom_keycodes.swap(0, 1);
    app.import_entlayout_from_path(&path).unwrap();
    let persisted: AppSettings =
        serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    for profile in persisted.application_layouts[&app.application_layout_device_key().unwrap()]
        .layouts
        .values()
    {
        assert!(profile.layers.iter().flatten().all(|code| *code == 0x7e01));
        assert!(profile
            .encoder_stacks
            .iter()
            .flatten()
            .all(|action| action.clockwise == 0x7e01 && action.counter_clockwise == 0x7e00));
    }
}

#[test]
fn macropad_unified_export_includes_confirmed_shared_pictograms() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    app.display_settings.pictograms.library.set_colored(
        PictogramKind::Macro,
        0,
        &[0x55; PICTOGRAM_BYTES],
        [1, 2, 3],
    );
    let value: serde_json::Value =
        serde_json::from_str(&app.entlayout_export_json().unwrap().unwrap()).unwrap();
    assert_eq!(
        value["pictograms"][0]["bitmap"],
        serde_json::json!(vec![0x55; PICTOGRAM_BYTES])
    );
    assert_eq!(
        value["pictograms"][0]["color"],
        serde_json::json!([1, 2, 3])
    );
}

#[test]
fn macropad_unified_import_restores_pictograms_with_real_hid_transfer_and_readback() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.layout.as_mut().unwrap().layers[0][0] = 6.into();
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    app.display_settings.pictograms.library.set_colored(
        PictogramKind::Macro,
        0,
        &[0x55; PICTOGRAM_BYTES],
        [1, 2, 3],
    );
    let expected = app.display_settings.pictograms.library.clone();
    let path = storage.path().join("pictograms.entlayout");
    app.write_entlayout_export(&path);
    app.display_settings.pictograms.library = PictogramLibrary::blank();
    let mut responses = crate::app::pictograms::test_pictogram_upload_responses(true, 0);
    responses.extend(crate::app::pictograms::test_pictogram_read_responses(
        &expected,
    ));
    recorder.respond_with(responses);
    app.import_entlayout_from_path(&path).unwrap();
    assert_eq!(app.display_settings.pictograms.library, expected);
    assert!(recorder.requests().iter().any(|request| request[0] == 0xc9));
}

// Stateful wire boundary: only COMMIT changes firmware; reads are generated
// from committed bytes, never from the import's staged host library.
fn assert_pictogram_import_cache_recovery(failure: Option<&'static str>) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.pictograms = Some(
        (0..2)
            .map(|slot| EntLayoutPictogram {
                tap_dance: false,
                slot,
                bitmap: vec![0x55 + slot as u8; PICTOGRAM_BYTES],
                color: [1, 2, 3],
            })
            .collect(),
    );
    let input = storage.path().join("two-icons.entlayout");
    write_entlayout_file(&input, &bundle, app.app_settings.language).unwrap();
    save_app_settings(&app.app_settings);
    let settings = std::fs::read(app_settings_path()).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    if failure == Some("late") {
        app.supported_qmk_settings = vec![200];
    }
    let committed = std::sync::Arc::new(std::sync::Mutex::new(PictogramLibrary::blank()));
    let firmware = committed.clone();
    let mut commits = 0;
    let mut failed = false;
    let mut slot = 0;
    let mut record = Vec::new();
    let mut reads = std::collections::VecDeque::new();
    let mut keys = std::collections::BTreeMap::new();
    recorder.respond_by(move |request| {
        let mut reply = [0; 32];
        reply[0] = request[0];
        match request[0] {
            0xc0 => {
                if !failed
                    && ((failure == Some("second-query") && commits == 1)
                        || (failure == Some("readback") && commits == 2))
                {
                    failed = true;
                    return Some(Err("injected post-commit query failure".into()));
                }
                let readback = if !failed && failure == Some("mismatch") && commits == 2 {
                    failed = true;
                    PictogramLibrary::blank()
                } else {
                    firmware.lock().unwrap().clone()
                };
                reads = crate::app::pictograms::test_pictogram_read_responses(&readback).into();
                reply = reads.pop_front().unwrap();
                reply[17] = crate::app::pictograms::test_pictogram_upload_responses(true, 0)[0][17];
            }
            0xca | 0xcb => reply = reads.pop_front().expect("unexpected read"),
            0xc7 => {
                assert_eq!(request[1], 0);
                slot = u16::from_le_bytes([request[2], request[3]]) as usize;
                record.clear();
            }
            0xc8 => {
                assert_eq!(
                    u16::from_le_bytes([request[1], request[2]]) as usize * 29,
                    record.len()
                );
                record.extend_from_slice(&request[3..]);
            }
            0xc9 => {
                firmware.lock().unwrap().set_colored(
                    PictogramKind::Macro,
                    slot,
                    &record[4..4 + PICTOGRAM_BYTES],
                    record[..3].try_into().unwrap(),
                );
                commits += 1;
            }
            0xfe if failure == Some("late") && request[1] == 0x0a => {
                return Some(Err("injected late layer-name failure".into()));
            }
            0x05 => {
                keys.insert(
                    (request[1], request[2], request[3]),
                    [request[4], request[5]],
                );
            }
            0x04 => {
                if let Some(code) = keys.get(&(request[1], request[2], request[3])) {
                    reply[4..6].copy_from_slice(code);
                }
            }
            _ => return None,
        }
        Some(Ok(reply))
    });
    let result = app.import_entlayout_from_path(&input);
    assert_eq!(result.is_err(), failure.is_some(), "{result:?}");
    let actual = committed.lock().unwrap().clone();
    assert!(
        actual.bitmap(PictogramKind::Macro, 0).is_some(),
        "first COMMIT must reach firmware"
    );
    if failure.is_some() {
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert_eq!(std::fs::read(app_settings_path()).unwrap(), settings);
        assert!(
            !app.display_settings.pictograms.loaded,
            "uncertain import must invalidate confirmed cache"
        );
        assert_eq!(
            app.display_settings.pictograms.library,
            PictogramLibrary::blank(),
            "never publish unverified staged bytes"
        );
    } else {
        assert!(app.display_settings.pictograms.loaded);
        assert_eq!(app.display_settings.pictograms.library, actual);
    }
    let prior_requests = recorder.requests().len();
    let output = storage.path().join("after-import.entlayout");
    app.write_entlayout_export(&output);
    let exported: EntLayoutFile = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    let icons = exported.pictograms.unwrap();
    assert_eq!(
        icons.len(),
        if failure == Some("second-query") {
            1
        } else {
            2
        }
    );
    for icon in icons {
        assert_eq!(
            actual.bitmap(PictogramKind::Macro, icon.slot),
            Some(icon.bitmap.as_slice())
        );
        assert_eq!(
            actual.color(PictogramKind::Macro, icon.slot),
            Some(icon.color)
        );
    }
    assert_eq!(
        recorder.requests()[prior_requests..]
            .iter()
            .any(|r| r[0] == 0xc0),
        failure.is_some(),
        "ordinary export must refresh uncertain firmware and reuse confirmed success"
    );
}

#[test]
fn macropad_partial_pictogram_import_refreshes_export_after_second_query_failure() {
    assert_pictogram_import_cache_recovery(Some("second-query"));
}

#[test]
fn macropad_pictogram_import_refreshes_export_after_readback_failure() {
    assert_pictogram_import_cache_recovery(Some("readback"));
}

#[test]
fn macropad_pictogram_import_refreshes_export_after_readback_mismatch() {
    assert_pictogram_import_cache_recovery(Some("mismatch"));
}

#[test]
fn macropad_pictogram_import_refreshes_export_after_late_firmware_failure() {
    assert_pictogram_import_cache_recovery(Some("late"));
}

#[test]
fn macropad_pictogram_import_success_retains_confirmed_cache() {
    assert_pictogram_import_cache_recovery(None);
}

#[test]
fn macropad_unified_invalid_pictogram_rejected_before_backup_or_hid() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.pictograms = Some(vec![EntLayoutPictogram {
        tap_dance: false,
        slot: 999,
        bitmap: vec![1],
        color: [1, 2, 3],
    }]);
    let path = storage.path().join("invalid-icon.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert!(recorder.requests().is_empty());
    assert!(!storage.path().join("entropy/backups").exists());
    assert!(!app_settings_path().exists());
}

#[test]
fn macropad_unified_pictogram_transfer_failure_preserves_persisted_profiles() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    app.display_settings
        .pictograms
        .library
        .set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    let path = storage.path().join("icon.entlayout");
    app.write_entlayout_export(&path);
    app.display_settings.pictograms.library = PictogramLibrary::blank();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .layers[0][0] = 99;
    save_app_settings(&app.app_settings);
    let bytes = std::fs::read(app_settings_path()).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    recorder.respond_with(vec![[0; 32]]);
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), bytes);
}

#[test]
fn macropad_unified_ambiguous_application_identity_is_rejected_without_mutation() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let mut bundle = app.entlayout_snapshot().unwrap();
    let mut profile = bundle
        .application_layouts
        .as_mut()
        .unwrap()
        .remove(&application)
        .unwrap();
    profile.id = "different-id".into();
    bundle
        .application_layouts
        .as_mut()
        .unwrap()
        .insert(profile.id.clone(), profile);
    let path = storage.path().join("ambiguous.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    assert!(app
        .import_entlayout_from_path(&path)
        .unwrap_err()
        .to_string()
        .contains("Ambiguous"));
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert!(recorder.requests().is_empty());
    assert!(!app_settings_path().exists());
}

#[test]
fn macropad_unified_export_reads_supported_pictograms_before_writing() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let mut library = PictogramLibrary::blank();
    library.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = false;
    recorder.respond_with(crate::app::pictograms::test_pictogram_read_responses(
        &library,
    ));
    let path = storage.path().join("loaded.entlayout");
    app.write_entlayout_export(&path);
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        value["pictograms"][0]["bitmap"],
        serde_json::json!(vec![0x55; PICTOGRAM_BYTES])
    );
}

#[test]
fn macropad_unified_rejects_unrestorable_shared_tap_dance_dependency_before_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    app.keycode_picker.tap_dance_entries = vec![crate::keycode_picker::TapDanceEntry::default()];
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.data.tap_dance.entries[0].on_tap = 0x77ff;
    bundle
        .application_layouts
        .as_mut()
        .unwrap()
        .get_mut(&application)
        .unwrap()
        .layers[15][14] = 0x5700;
    let path = storage.path().join("invalid-shared.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert!(recorder.requests().is_empty());
    assert!(!storage.path().join("entropy/backups").exists());
}

#[test]
fn macropad_unified_duplicate_profile_identity_is_rejected_before_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let bundle = app.entlayout_snapshot().unwrap();
    let mut json = serde_json::to_string(&bundle).unwrap();
    let profile = &bundle.application_layouts.as_ref().unwrap()[&application];
    let member = format!(
        "{}:{}",
        serde_json::to_string(&application).unwrap(),
        serde_json::to_string(profile).unwrap()
    );
    json = json.replacen(&member, &format!("{member},{member}"), 1);
    let path = storage.path().join("duplicate-profile.entlayout");
    std::fs::write(&path, json).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert!(recorder.requests().is_empty());
    assert!(!storage.path().join("entropy/backups").exists());
}

#[test]
fn macropad_unified_import_preserves_unrelated_global_pictogram_library_bytes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    let icons = vec![SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: "Unrelated disk library".into(),
        color: [1, 2, 3],
        bitmap: vec![0x55; PICTOGRAM_BYTES],
    }];
    write_pictogram_file(&pictogram_library_path(), &icons).unwrap();
    let bytes = std::fs::read(pictogram_library_path()).unwrap();
    let path = storage.path().join("all.entlayout");
    app.write_entlayout_export(&path);
    app.import_entlayout_from_path(&path).unwrap();
    assert_eq!(std::fs::read(pictogram_library_path()).unwrap(), bytes);
}

#[test]
fn macropad_unified_import_reads_cold_target_library_without_erasing_other_slots() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.layout.as_mut().unwrap().layers[0][0] = 6.into();
    app.display_settings.pictograms.supported = Some(true);
    app.display_settings.pictograms.loaded = true;
    app.display_settings
        .pictograms
        .library
        .set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    let path = storage.path().join("cold-target.entlayout");
    app.write_entlayout_export(&path);
    let mut target = PictogramLibrary::blank();
    target.set(PictogramKind::TapDance, 1, &[0xaa; PICTOGRAM_BYTES]);
    let mut expected = target.clone();
    expected.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
    app.display_settings.pictograms.library = PictogramLibrary::blank();
    app.display_settings.pictograms.loaded = false;
    let mut responses = crate::app::pictograms::test_pictogram_read_responses(&target);
    responses.extend(crate::app::pictograms::test_pictogram_upload_responses(
        true, 0,
    ));
    responses.extend(crate::app::pictograms::test_pictogram_read_responses(
        &expected,
    ));
    recorder.respond_with(responses);
    app.import_entlayout_from_path(&path).unwrap();
    assert_eq!(app.display_settings.pictograms.library, expected);
}

#[test]
fn macropad_unified_unknown_custom_action_is_rejected_on_keys_and_encoder_stacks() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let path = storage.path().join("unknown-custom.entlayout");
    for encoder in [false, true] {
        let mut bundle = app.entlayout_snapshot().unwrap();
        let profile = bundle
            .application_layouts
            .as_mut()
            .unwrap()
            .get_mut(&application)
            .unwrap();
        if encoder {
            profile.encoder_stacks[0][0].clockwise = 0x7eff;
        } else {
            profile.layers[15][14] = 0x7eff;
        }
        write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
        assert!(app.import_entlayout_from_path(&path).is_err());
        assert!(recorder.requests().is_empty());
        assert!(!storage.path().join("entropy/backups").exists());
    }
}

#[test]
fn macropad_application_navigation_round_trip_reloads_all_keys() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let path = _storage.path().join("navigation.entlayout");
    let source = if let Ok(private_path) = std::env::var("ENTROPY_PROFILE_REPLAY") {
        assert!(
            app.import_application_layout_from_path(Path::new(&private_path), &application)
                .is_err(),
            "the obsolete input must not be accepted as a layout"
        );
        let bytes = std::fs::read(private_path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Replay only the captured content, not its obsolete file envelope.
        serde_json::from_value(value["profile"].clone()).unwrap()
    } else {
        let profile = app
            .application_layout_settings_mut()
            .unwrap()
            .layouts
            .get_mut(&application)
            .unwrap();
        profile.layers[0][..5].copy_from_slice(&[4, 0x00A9, 0x00F0, 0x7E05, 0x7E06]);
        let source = profile.clone();
        source
    };
    let source: crate::application_layouts::ApplicationLayout = source;
    write_application_entlayout(&path, source.clone(), app.app_settings.language).unwrap();
    let before = app.application_layout_settings().unwrap().clone();
    app.import_application_layout_from_path(&path, &application)
        .unwrap();
    // Restore through the production writer and reader. All 16x15 controls
    // and encoder stacks must survive disk reload.
    let modern = _storage.path().join("navigation.entlayout");
    app.write_application_layout_export(&modern, &application)
        .unwrap();
    let profile = app
        .application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap();
    profile.layers[0].fill(42);
    profile.layer_names[0] = "Changed".into();
    profile.encoder_stacks[0].clear();
    app.import_application_layout_from_path(&modern, &application)
        .unwrap();
    let restored: AppSettings =
        serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    let key = app.application_layout_device_key().unwrap();
    let target = &restored.application_layouts[&key].layouts[&application];
    assert_eq!(target.layers, source.layers);
    assert_eq!(target.layer_names, source.layer_names);
    assert_eq!(target.encoder_stacks, source.encoder_stacks);
    assert_eq!(target.id, before.layouts[&application].id);
    assert_eq!(
        restored.application_layouts[&key].layouts[DEFAULT_APPLICATION_LAYOUT_ID],
        before.layouts[DEFAULT_APPLICATION_LAYOUT_ID]
    );
    assert_eq!(
        restored.application_layouts[&key].active_layout_id,
        before.active_layout_id
    );
    assert!(recorder.requests().is_empty());
}

#[test]
fn macropad_application_entlayout_envelope_is_versioned_and_not_a_device_layout() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let path = _storage.path().join("application.entlayout");
    app.write_application_layout_export(&path, &application)
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["format"], "entropy.layout");
    assert_eq!(value["version"], 8);
    assert_eq!(value["scope"], "application");
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let error = app
        .import_entlayout_from_path(&path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("single application"), "{error}");
    assert!(error.contains("Export layout"), "{error}");
    assert!(!error.contains("Import into application"), "{error}");
    app.app_settings.language = crate::i18n::Language::Russian;
    let russian_error = app
        .import_entlayout_from_path(&path)
        .unwrap_err()
        .to_string();
    assert!(
        russian_error.contains("только одну раскладку приложения"),
        "{russian_error}"
    );
    assert!(
        russian_error.contains("«Экспорт раскладки»"),
        "{russian_error}"
    );
    assert!(
        !russian_error.contains("Импорт в приложение"),
        "{russian_error}"
    );
    app.app_settings.language = crate::i18n::Language::English;
    assert!(recorder.requests().is_empty());
    assert!(!app_settings_path().exists());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .set_keycode(0, 0, 42);
    app.import_application_layout_from_path(&path, &application)
        .unwrap();
    assert_eq!(
        app.application_layout_settings().unwrap().layouts[&application].layers[0][0],
        7
    );
    let mut invalid = value.clone();
    invalid["version"] = serde_json::json!(9);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(app
        .import_application_layout_from_path(&path, &application)
        .is_err());
    assert!(recorder.requests().is_empty());
    // Characterize unchanged Default/device schema and explicit target boundary.
    let mut device = app.entlayout_snapshot().unwrap();
    device.version = 7;
    device.application_layouts = None;
    let device_path = _storage.path().join("default.entlayout");
    write_entlayout_file(&device_path, &device, app.app_settings.language).unwrap();
    let device_value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&device_path).unwrap()).unwrap();
    assert_eq!(device_value["format"], "entropy.layout");
    assert_eq!(device_value["version"], 7);
    assert!(device_value.get("scope").is_none());
    let before = serde_json::to_value(&app.app_settings).unwrap();
    assert!(app
        .import_application_layout_from_path(&device_path, &application)
        .is_err());
    for (format, version, scope) in [
        ("entropy.layout", 7, Some("application")),
        ("entropy.layout", 8, Some("unknown")),
        ("entropy.layout", 8, None),
        ("entropy.application-layout", 2, None),
        ("entropy.application-layout", 1, Some("application")),
    ] {
        let mut malformed = value.clone();
        malformed["format"] = serde_json::json!(format);
        malformed["version"] = serde_json::json!(version);
        if let Some(scope) = scope {
            malformed["scope"] = serde_json::json!(scope);
        } else {
            malformed.as_object_mut().unwrap().remove("scope");
        }
        std::fs::write(&path, serde_json::to_vec(&malformed).unwrap()).unwrap();
        assert!(app
            .import_application_layout_from_path(&path, &application)
            .is_err());
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    }
}

#[test]
fn macropad_application_native_picker_only_offers_entlayout() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    for export in [true, false] {
        crate::app::file_dialog::set_test_dialog_result(None);
        app.application_layout_file_dialog(application.clone(), export);
        let options = crate::app::file_dialog::test_dialog_options();
        assert!(options.contains(".entlayout"), "{options}");
        assert!(options.contains("\"entlayout\""), "{options}");
        assert!(!options.contains("entapp"), "{options}");
        app.poll_file_dialog(&egui::Context::default());
    }
}

#[test]
fn macropad_application_rejects_legacy_format_and_other_extensions_without_changes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let profile = app.application_layout_settings().unwrap().layouts[&application].clone();
    for extension in ["entlayout", "entapp"] {
        let path = storage.path().join(format!("legacy.{extension}"));
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "format": "entropy.application-layout", "version": 1, "profile": profile
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(app
            .import_application_layout_from_path(&path, &application)
            .is_err());
    }
    let modern = storage.path().join("modern.entlayout");
    app.write_application_layout_export(&modern, &application)
        .unwrap();
    for extension in ["entapp", "json"] {
        let path = storage.path().join(format!("modern.{extension}"));
        std::fs::copy(&modern, &path).unwrap();
        assert!(app
            .import_application_layout_from_path(&path, &application)
            .is_err());
        assert!(app
            .write_application_layout_export(&path, &application)
            .is_err());
    }
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert!(!app_settings_path().exists());
    assert!(recorder.requests().is_empty());
}

#[test]
fn macropad_entlayout_duplicate_fields_rejected_before_any_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let device = serde_json::to_value(app.entlayout_snapshot().unwrap()).unwrap();
    let path = storage.path().join("duplicate.entlayout");
    app.write_application_layout_export(&path, &application)
        .unwrap();
    let profile: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let backups = app_settings_path().parent().unwrap().join("backups");
    for (value, field, contents, application_scope) in [
        (&device, "version", &device["version"], false),
        (&device, "data", &device["data"], false),
        (&device, "keymap", &device["data"]["keymap"], false),
        (&profile, "version", &profile["version"], true),
        (&profile, "profile", &profile["profile"], true),
        (&profile, "layers", &profile["profile"]["layers"], true),
    ] {
        let json = serde_json::to_string(value).unwrap();
        let member = format!("\"{field}\":{}", serde_json::to_string(contents).unwrap());
        let duplicate = json.replacen(&member, &format!("{member},{member}"), 1);
        assert_ne!(duplicate, json);
        std::fs::write(&path, duplicate).unwrap();
        let error = if application_scope {
            app.import_application_layout_from_path(&path, &application)
                .unwrap_err()
        } else {
            app.import_entlayout_from_path(&path).unwrap_err()
        };
        assert!(
            format!("{error:#}").contains("duplicate field"),
            "{field}: {error:#}"
        );
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
        assert!(!app_settings_path().exists());
        assert!(!backups.exists());
        assert!(recorder.requests().is_empty());
    }
}

#[test]
fn macropad_application_file_round_trip_targets_only_explicit_profile() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .set_encoder_stack(
            0,
            vec![
                crate::application_layouts::EncoderStackAction {
                    name: "Zoom".into(),
                    counter_clockwise: 0x0104,
                    clockwise: 0x0105,
                },
                crate::application_layouts::EncoderStackAction {
                    name: "Scroll".into(),
                    counter_clockwise: 0x0204,
                    clockwise: 0x0205,
                },
            ],
        );
    let before = app.application_layout_settings().unwrap().clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("application.entlayout");
    app.write_application_layout_export(&path, &application)
        .unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .set_keycode(0, 0, 42);
    app.import_application_layout_from_path(&path, &application)
        .unwrap();
    let settings = app.application_layout_settings().unwrap();
    assert_eq!(
        settings.layouts[&application].layers,
        before.layouts[&application].layers
    );
    assert_eq!(
        settings.layouts[&application].encoder_stacks,
        before.layouts[&application].encoder_stacks
    );
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID],
        before.layouts[DEFAULT_APPLICATION_LAYOUT_ID]
    );
    assert_eq!(settings.active_layout_id, before.active_layout_id);
    assert!(
        recorder.requests().is_empty(),
        "profile import must not rewrite shared firmware state"
    );
}

#[test]
fn macropad_application_persistence_failure_keeps_memory_library_and_existing_destination() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let input = _storage.path().join("application.entlayout");
    app.write_application_layout_export(&input, &application)
        .unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&application)
        .unwrap()
        .set_keycode(0, 0, 42);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    write_pictogram_file(
        &pictogram_library_path(),
        &app.app_settings.saved_pictograms,
    )
    .unwrap();
    let library_before = std::fs::read(pictogram_library_path()).unwrap();
    // A directory at the destination makes atomic persist fail even as root.
    let destination = app_settings_path();
    std::fs::create_dir(&destination).unwrap();
    let sentinel = destination.join("keep");
    std::fs::write(&sentinel, b"untouched").unwrap();
    let error = app
        .import_application_layout_from_path(&input, &application)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Failed to save application layout"),
        "{error:#}"
    );
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(
        std::fs::read(pictogram_library_path()).unwrap(),
        library_before
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"untouched");
    assert!(recorder.requests().is_empty());
    // PersistError owns the temporary file until the reported error is dropped.
    drop(error);
    assert!(std::fs::read_dir(destination.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")));
}

#[test]
fn macropad_application_menu_has_only_unified_layout_actions() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    app.application_layout_settings_mut()
        .unwrap()
        .editor_layout_id = application;
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..2 {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.draw_layout_device_dropdown(
                    ui,
                    &ctx,
                    crate::i18n::Language::English,
                    Some(egui::Rect::from_min_size(
                        egui::pos2(400.0, 0.0),
                        egui::vec2(80.0, 30.0),
                    )),
                    true,
                    false,
                    false,
                );
            },
        );
        texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
    }
    assert!(
        !texts.iter().any(|text| text == "Export application: Entropy"),
        "{texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|text| text == "Import into application: Entropy"),
        "{texts:?}"
    );
}

#[test]
fn macropad_unified_menu_clicks_complete_picker_and_file_dispatch() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for export in [true, false] {
        exercise_unified_menu_lifecycle(export, true, 0);
        exercise_unified_menu_lifecycle(export, false, 0);
    }
}

#[test]
fn macropad_unified_import_cold_load_failure_can_retry_from_nondefault_menu() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for failing_read in [0x12, 0x0e] {
        exercise_unified_menu_lifecycle(false, true, failing_read);
        exercise_unified_menu_lifecycle(false, false, failing_read);
    }
}

fn exercise_unified_menu_lifecycle(export: bool, nondefault: bool, failing_read: u8) {
    let cold_failure = failing_read != 0;
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (mut app, recorder, application) = fixture();
    let selection = if nondefault {
        application.clone()
    } else {
        DEFAULT_APPLICATION_LAYOUT_ID.into()
    };
    app.application_layout_settings_mut()
        .unwrap()
        .editor_layout_id = selection.clone();
    // Fully specified saved profiles: runtime inheritance normalization must
    // not be mistaken for import corruption in the persisted comparison.
    for profile in app
        .application_layout_settings_mut()
        .unwrap()
        .layouts
        .values_mut()
    {
        profile.seed_unset_keycodes(
            [[4; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
                crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT],
        );
    }
    if failing_read == 0x0e {
        app.keycode_picker.macro_count = 1;
        app.keycode_picker.macro_texts = vec![b"m".to_vec()];
        app.keycode_picker.macro_names = vec!["Shared macro".into()];
        for profile in app
            .application_layout_settings_mut()
            .unwrap()
            .layouts
            .values_mut()
        {
            profile.set_keycode(0, 1, 0x7700);
        }
    }
    let source = app.application_layout_settings().unwrap().layouts.clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clicked.entlayout");
    if !export {
        app.write_entlayout_export(&path);
        for (id, profile) in &mut app.application_layout_settings_mut().unwrap().layouts {
            profile.set_keycode(0, 0, 88);
            profile.layer_names[0] = "damaged name".into();
            profile.encoder_stacks.clear();
            if id != DEFAULT_APPLICATION_LAYOUT_ID {
                profile.application_identities.clear();
            }
        }
    }
    if cold_failure {
        app.deferred_device_load = DeferredDeviceLoadState::staged(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(vec![]),
            definition_fingerprint: 42,
            layer_count: 8,
            rows: 5,
            cols: 3,
            encoder_count: 1,
            macro_count: if failing_read == 0x0e { 1 } else { 0 },
            macro_memory_bytes: Some(8),
            tap_dance_count: 0,
            combo_count: 0,
            key_override_count: 0,
            alt_repeat_count: 0,
            modules_supported: false,
            touchpad_supported: false,
            bluetooth_supported: false,
            layer_leds_supported: false,
            rgb_supported: false,
            lighting_mode: None,
            supports_rmk_native_key_actions: false,
            supports_universal_symbols: false,
            supports_universal_russian_letters: false,
            supports_rmk_native_combo_output: false,
            supports_rmk_native_tap_dance_actions: false,
            supports_rmk_combo_layers: false,
        });
        let mut fail_once = true;
        let mut keys = std::collections::BTreeMap::new();
        let mut macros = [0u8; 8];
        macros[..3].copy_from_slice(b"old");
        recorder.respond_by(move |request| match request[0] {
            opcode if opcode == failing_read && fail_once => {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .expect("release cold read");
                fail_once = false;
                Some(Err("injected cold layout read failure".into()))
            }
            0x12 => Some(Ok([0; 32])),
            0x0d => {
                let mut response = [0; 32];
                response[2] = 8;
                Some(Ok(response))
            }
            0x0e => {
                let mut response = [0; 32];
                let offset = u16::from_be_bytes([request[1], request[2]]) as usize;
                let len = request[3] as usize;
                response[4..4 + len].copy_from_slice(&macros[offset..offset + len]);
                Some(Ok(response))
            }
            0x0f => {
                let offset = u16::from_be_bytes([request[1], request[2]]) as usize;
                let len = request[3] as usize;
                macros[offset..offset + len].copy_from_slice(&request[4..4 + len]);
                Some(Ok([0; 32]))
            }
            0x05 => {
                keys.insert(
                    (request[1], request[2], request[3]),
                    [request[4], request[5]],
                );
                Some(Ok([0; 32]))
            }
            0x04 => {
                let mut response = [0; 32];
                if let Some(code) = keys.get(&(request[1], request[2], request[3])) {
                    response[4..6].copy_from_slice(code);
                }
                Some(Ok(response))
            }
            _ => None,
        });
    }
    let ctx = egui::Context::default();
    let draw = |app: &mut EntropyApp, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.draw_layout_device_dropdown(
                    ui,
                    &ctx,
                    crate::i18n::Language::English,
                    Some(egui::Rect::from_min_size(
                        egui::pos2(400.0, 0.0),
                        egui::vec2(80.0, 30.0),
                    )),
                    true,
                    false,
                    false,
                );
            },
        )
    };
    let _ = draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let label = if export {
        "Export layout"
    } else {
        "Import layout"
    };
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    crate::app::file_dialog::set_test_dialog_result(Some(path.clone()));
    for pressed in [true, false] {
        let _ = draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    if cold_failure {
        // Keep the real HID worker in flight across a native background tick.
        // Fast local replies used to hide accidental discovery of the fake device.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.tick_layout_import_lifecycle_for_test(&ctx, 0.1);
            if entered_rx.try_recv().is_ok() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cold read did not start: {}",
                app.status_msg
            );
            std::thread::yield_now();
        }
        assert!(app.hid_device.is_none());
        let generation = app.connection_generation;
        app.tick_layout_import_lifecycle_for_test(&ctx, 0.1);
        release_tx.send(()).unwrap();
        assert!(matches!(app.device_scan_state, DeviceScanState::Idle), "synthetic lifecycle must not start real HID discovery while the worker owns the device");
        assert_eq!(app.connection_generation, generation);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline && app.deferred_full_layout_action_pending() {
            app.tick_layout_import_lifecycle_for_test(&ctx, 0.1);
            std::thread::yield_now();
        }
        assert!(
            !app.deferred_full_layout_action_pending(),
            "failed cold load leaves ordinary import waiting forever: {}",
            app.status_msg
        );
        assert!(app.status_msg.contains("failed"), "{}", app.status_msg);
        assert!(app.pending_file_dialog.is_none());
        assert!(!app.import_pending());
        assert!(!recorder.requests().iter().any(|request| request[0] == 0x05));
        // Retry using the same ordinary menu, not an importer or readiness override.
        let _ = draw(&mut app, vec![]);
        for pressed in [true, false] {
            let _ = draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline && app.pending_file_dialog.is_none() {
            app.tick_layout_import_lifecycle_for_test(&ctx, 0.2);
            std::thread::yield_now();
        }
    }
    let expected = if export {
        crate::app::file_dialog::FileDialogAction::ExportEntlayout
    } else {
        crate::app::file_dialog::FileDialogAction::ImportEntlayout
    };
    assert_eq!(
        app.pending_file_dialog.as_ref().map(|pending| &pending.0),
        Some(&expected),
        "read={failing_read:#x}, nondefault={nondefault}, action={:?}, busy={}, gaps={:?}, status={}",
        app.deferred_full_layout_action, app.hid_write_lifecycle_busy(), app.entlayout_export_gaps(), app.status_msg
    );
    // Keep NONDEFAULT selected across completion and production lifecycle dispatch.
    app.application_layout_settings_mut()
        .unwrap()
        .active_layout_id = application.clone();
    app.poll_file_dialog(&ctx);
    assert!(app.pending_file_dialog.is_none());
    if !export {
        assert!(app.import_pending());
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.1);
        assert!(!app.import_pending(), "{}", app.status_msg);
        assert!(app.import_report_open, "{}", app.status_msg);
        let key = app.application_layout_device_key().unwrap();
        assert_eq!(
            load_app_settings().application_layouts[&key].layouts[&application].layers[0][0],
            7
        );
    }
    assert!(path.exists());
    assert_eq!(
        app.application_layout_settings().unwrap().layouts[&application].layers[0][0],
        7
    );
    if export {
        assert!(recorder.requests().is_empty());
    } else {
        let key = app.application_layout_device_key().unwrap();
        let restored = load_app_settings();
        if failing_read == 0x0e {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while app.hid_device.is_none() && std::time::Instant::now() < deadline {
                app.poll_vial_hid_task(&ctx);
                std::thread::yield_now();
            }
            assert_eq!(app.keycode_picker.macro_texts, vec![b"m".to_vec()]);
            let buffer = app
                .hid_device
                .as_ref()
                .unwrap()
                .get_macro_buffer(8, 1)
                .unwrap();
            assert_eq!(
                crate::hid::HidDevice::parse_macros(&buffer, 1),
                vec![b"m".to_vec()]
            );
            assert_eq!(app.keycode_picker.macro_names[0], "Shared macro");
            assert!(
                std::fs::read_to_string(macro_metadata_path(&app.current_device_name))
                    .unwrap()
                    .contains("Shared macro")
            );
        }
        for (id, expected) in source {
            let actual = &restored.application_layouts[&key].layouts[&id];
            assert_eq!(actual.layers, expected.layers);
            assert_eq!(actual.layer_names, expected.layer_names);
            assert_eq!(actual.encoder_stacks, expected.encoder_stacks);
            assert_eq!(actual.executable, expected.executable);
            assert_eq!(
                actual.application_identities,
                expected.application_identities
            );
            assert_eq!(actual.name, expected.name);
        }
    }
}

#[test]
fn macropad_application_picker_dispatch_keeps_captured_target_when_selection_changes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    use crate::app::file_dialog::FileDialogAction;
    let (mut app, recorder, application) = fixture();
    let other = app
        .application_layout_settings_mut()
        .unwrap()
        .create_for_application(&DetectedApplication {
            executable: "other".into(),
            display_name: "Other".into(),
            ..Default::default()
        });
    let original = app.application_layout_settings().unwrap().clone();
    app.keycode_picker.macro_texts = vec![b"shared macro".to_vec()];
    app.keycode_picker.macro_names = vec!["Shared macro".into()];
    app.combo_entries = vec![ComboEntry {
        keys: [4, 5, 0, 0],
        output: 6.into(),
        layer: None,
    }];
    app.app_settings.saved_pictograms.push(SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: "Shared artwork".into(),
        color: [1, 2, 3],
        bitmap: vec![0x55; PICTOGRAM_BYTES],
    });
    let shared_before = serde_json::to_value(&app.app_settings.saved_pictograms).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("captured.entlayout");
    let ctx = egui::Context::default();
    for export in [true, false] {
        if !export {
            app.application_layout_settings_mut()
                .unwrap()
                .layouts
                .get_mut(&other)
                .unwrap()
                .set_keycode(0, 0, 99);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let action = if export {
            FileDialogAction::ExportApplicationLayout(application.clone())
        } else {
            FileDialogAction::ImportApplicationLayout(other.clone())
        };
        app.pending_file_dialog = Some((action, app.connection_generation, rx));
        app.application_layout_settings_mut()
            .unwrap()
            .editor_layout_id = DEFAULT_APPLICATION_LAYOUT_ID.into();
        tx.send(Some(path.clone())).unwrap();
        app.poll_file_dialog(&ctx);
        assert!(app.pending_file_dialog.is_none());
        assert!(!app.status_msg.contains("failed"), "{}", app.status_msg);
    }
    let settings = app.application_layout_settings().unwrap();
    assert_eq!(
        settings.layouts[&other].layers,
        original.layouts[&application].layers
    );
    assert_eq!(
        settings.layouts[&other].encoder_stacks,
        original.layouts[&application].encoder_stacks
    );
    assert_eq!(settings.layouts[&other].executable, "other");
    assert_eq!(settings.layouts[&other].id, other);
    assert_eq!(
        settings.layouts[&application],
        original.layouts[&application]
    );
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID],
        original.layouts[DEFAULT_APPLICATION_LAYOUT_ID]
    );
    assert_eq!(
        serde_json::to_value(&app.app_settings.saved_pictograms).unwrap(),
        shared_before
    );
    let persisted: AppSettings =
        serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    assert_eq!(
        app.keycode_picker.macro_texts,
        vec![b"shared macro".to_vec()]
    );
    assert_eq!(app.keycode_picker.macro_names, vec!["Shared macro"]);
    assert_eq!(app.combo_entries[0].keys, [4, 5, 0, 0]);
    assert_eq!(
        persisted.application_layouts,
        app.app_settings.application_layouts
    );
    assert!(recorder.requests().is_empty());
}

#[test]
fn macropad_application_export_rejects_unimportable_keys_and_encoder_actions_without_writing() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let path = _storage.path().join("unsupported.entlayout");
    std::fs::write(&path, b"existing export").unwrap();
    for code in [
        0x2000, 0x5700, 0x5F00, 0x7700, 0x7E00, 0x7E04, 0x7E07, 0x7EFF,
    ] {
        for encoder in [false, true] {
            let profile = app
                .application_layout_settings_mut()
                .unwrap()
                .layouts
                .get_mut(&application)
                .unwrap();
            profile.set_keycode(0, 0, if encoder { 4 } else { code });
            profile.encoder_stacks[0][0].clockwise = if encoder { code } else { 0x00A9 };
            let error = app
                .write_application_layout_export(&path, &application)
                .unwrap_err()
                .to_string();
            assert!(error.contains("Safe resource remapping"), "{error}");
            assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
        }
    }
}

#[test]
fn macropad_application_import_rejects_shared_resource_ids_without_partial_changes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resource.entlayout");
    for code in [0x7700, 0x5700, 0x5f00, 0x7e00] {
        app.application_layout_settings_mut()
            .unwrap()
            .layouts
            .get_mut(&application)
            .unwrap()
            .set_keycode(0, 0, code);
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "format": "entropy.layout", "version": 8, "scope": "application",
                "profile": app.application_layout_settings().unwrap().layouts[&application]
            }))
            .unwrap(),
        )
        .unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        let error = app
            .import_application_layout_from_path(&path, &application)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Safe resource remapping"), "{error}");
        assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    }
    assert!(recorder.requests().is_empty());
}

#[test]
fn macropad_application_picker_rejects_stale_connection_and_deleted_target() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stale.entlayout");
    let (tx, rx) = std::sync::mpsc::channel();
    app.pending_file_dialog = Some((
        crate::app::file_dialog::FileDialogAction::ExportApplicationLayout(application.clone()),
        app.connection_generation,
        rx,
    ));
    app.connection_generation += 1;
    tx.send(Some(path.clone())).unwrap();
    app.poll_file_dialog(&egui::Context::default());
    assert!(!path.exists());
    app.write_application_layout_export(&path, &application)
        .unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .remove(&application);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    assert!(app
        .import_application_layout_from_path(&path, &application)
        .is_err());
    assert!(app
        .import_application_layout_from_path(&path, DEFAULT_APPLICATION_LAYOUT_ID)
        .is_err());
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
}

#[test]
fn macropad_default_universal_reordered_mapping_imports_encoders_losslessly_and_keeps_stacks() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let stacks = app.application_layout_settings().unwrap().layouts[DEFAULT_APPLICATION_LAYOUT_ID]
        .encoder_stacks
        .clone();
    let other = app.application_layout_settings().unwrap().layouts[&application].clone();
    let mut bundle = app.entlayout_snapshot().unwrap();
    // Historical ordinary device file remains Default-only.
    bundle.version = 7;
    bundle.application_layouts = None;
    bundle.keyboard.layout_hash = "reordered source".into();
    bundle.data.source_layout.as_mut().unwrap().keys.swap(0, 1);
    for layer in &mut bundle.data.keymap {
        layer.swap(0, 1);
    }
    for layer in &mut bundle.data.native_keymap {
        layer.swap(0, 1);
    }
    bundle.data.keymap[0][0] = 11;
    bundle.data.keymap[0][1] = 12;
    bundle.data.encoder_keymap[0][0] = 0x0104;
    bundle.data.encoder_keymap[0][1] = 0x0205;
    let mapping = app.entlayout_import_mapping(&bundle).unwrap();
    assert!(!mapping.exact_layout);
    assert_eq!(mapping.key_mapping[0], Some(1));
    assert_eq!(mapping.key_mapping[1], Some(0));
    app.apply_entlayout(&bundle).unwrap();
    let layout = app.layout.as_ref().unwrap();
    assert_eq!(layout.get_keycode(0, 0), 12);
    assert_eq!(layout.get_keycode(0, 1), 11);
    assert_eq!(layout.get_encoder_keycode(0, 0), 0x0104);
    assert_eq!(layout.get_encoder_keycode(0, 1), 0x0205);
    let settings = app.application_layout_settings().unwrap();
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].encoder_stacks,
        stacks
    );
    assert_eq!(settings.layouts[&application], other);
    let rendered = app.application_layout_rendered_copy(layout);
    assert_eq!(rendered.get_keycode(0, 0), 12);
    assert_eq!(rendered.get_encoder_keycode(0, 0), 0x0104);
    assert_eq!(rendered.get_encoder_keycode(0, 1), 0x0205);
}

#[test]
fn macropad_default_export_uses_default_storage_not_device_or_active_application() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let settings = app.application_layout_settings_mut().unwrap();
    settings.active_layout_id = application.clone();
    settings.editor_layout_id = application;
    let before = serde_json::to_value(app.application_layout_settings().unwrap()).unwrap();
    let bundle = app.entlayout_snapshot().unwrap();
    assert_eq!(
        bundle.data.keymap[0][0], 6,
        "Default export must include pending/local Default edits"
    );
    assert_eq!(bundle.data.layer_names[0], "My Default");
    assert_eq!(
        bundle.keyboard.layers, 8,
        "Default overlay must not invent device layers"
    );
    assert_eq!(bundle.data.layer_names.len(), bundle.keyboard.layers);
    assert_eq!(
        bundle.data.keymap[0][1], 4,
        "unset profile control must inherit firmware, not export 0xffff"
    );
    assert_eq!(bundle.data.encoder_keymap[0][0], 5);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exported-default.entlayout");
    let (tx, rx) = std::sync::mpsc::channel();
    app.pending_file_dialog = Some((
        crate::app::file_dialog::FileDialogAction::ExportEntlayout,
        app.connection_generation,
        rx,
    ));
    tx.send(Some(path.clone())).unwrap();
    app.poll_file_dialog(&egui::Context::default());
    let written: EntLayoutFile =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(written.data.keymap, bundle.data.keymap);
    assert_eq!(written.data.layer_names, bundle.data.layer_names);
    assert_eq!(
        serde_json::to_value(app.application_layout_settings().unwrap()).unwrap(),
        before
    );
}

#[test]
fn macropad_default_import_updates_rendered_default_and_preserves_application_profiles() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let profile_before =
        serde_json::to_value(&app.application_layout_settings().unwrap().layouts[&application])
            .unwrap();
    let mut bundle = app.entlayout_snapshot().unwrap();
    // Historical ordinary device file remains Default-only.
    bundle.version = 7;
    bundle.application_layouts = None;
    bundle.data.keymap[0][0] = 9;
    bundle.data.layer_names[0] = "Restored Default".into();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    let ctx = egui::Context::default();
    let (tx, rx) = std::sync::mpsc::channel();
    app.pending_file_dialog = Some((
        crate::app::file_dialog::FileDialogAction::ImportEntlayout,
        app.connection_generation,
        rx,
    ));
    tx.send(Some(path)).unwrap();
    app.poll_file_dialog(&ctx);
    let (path, generation) = app
        .pending_entlayout_import_path
        .take()
        .expect("picker result queues import");
    assert_eq!(generation, app.connection_generation);
    let before_import = app.entlayout_snapshot().unwrap();
    app.import_entlayout_from_path(&path).unwrap();
    let backup_dir = _storage.path().join("entropy/backups");
    let backups: Vec<_> = std::fs::read_dir(&backup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1, "import must persist its real auto-backup");
    let backup: EntLayoutFile =
        serde_json::from_slice(&std::fs::read(&backups[0]).unwrap()).unwrap();
    assert_eq!(backup.data.keymap, before_import.data.keymap);
    let persisted: AppSettings =
        serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&persisted.application_layouts).unwrap(),
        serde_json::to_value(&app.app_settings.application_layouts).unwrap()
    );
    assert_eq!(
        read_pictogram_file(&pictogram_library_path()).unwrap(),
        app.app_settings.saved_pictograms
    );
    assert!(
        recorder
            .requests()
            .iter()
            .any(|r| r[..6] == [0x05, 0, 1, 0, 0, 9]),
        "real keymap write was not reached"
    );
    assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 9);
    let rendered = app.application_layout_rendered_copy(app.layout.as_ref().unwrap());
    assert_eq!(
        rendered.get_keycode(0, 0),
        9,
        "import is invisible while cached Default still wins"
    );
    let settings = app.application_layout_settings().unwrap();
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names[0],
        "Restored Default"
    );
    assert_eq!(
        serde_json::to_value(&settings.layouts[&application]).unwrap(),
        profile_before
    );
}

#[test]
fn macropad_import_while_application_active_preserves_profiles_and_unmapped_default_controls() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    {
        let settings = app.application_layout_settings_mut().unwrap();
        settings.active_layout_id = application.clone();
        settings.editor_layout_id = application.clone();
        settings
            .layouts
            .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .set_keycode(0, 1, 10);
    }
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.macro_texts = vec![b"macro body".to_vec()];
    app.keycode_picker.macro_names = vec!["Macro name".into()];
    app.combo_entries = vec![ComboEntry {
        keys: [4, 5, 0, 0],
        output: 6.into(),
        layer: None,
    }];
    app.combo_names = vec!["Combo name".into()];
    app.app_settings.saved_pictograms.push(SavedPictogram {
        origin: PictogramOrigin::Legacy,
        name: "Keep artwork".into(),
        color: [1, 2, 3],
        bitmap: vec![0x55; PICTOGRAM_BYTES],
    });
    let icons_before = serde_json::to_value(&app.app_settings.saved_pictograms).unwrap();
    let app_before =
        serde_json::to_value(&app.application_layout_settings().unwrap().layouts[&application])
            .unwrap();
    let mut bundle = app.entlayout_snapshot().unwrap();
    // Historical ordinary device file remains Default-only.
    bundle.version = 7;
    bundle.application_layouts = None;
    bundle.data.keymap[0][0] = 11;
    let mut mapping = app.entlayout_import_mapping(&bundle).unwrap();
    mapping.key_mapping[1] = None;
    app.apply_entlayout(&bundle).unwrap();
    // Exercise a partial universal mapping separately: only key 0 is imported.
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap()
        .set_keycode(0, 1, 12);
    app.apply_entlayout_local_state(&bundle, &mapping).unwrap();
    let settings = app.application_layout_settings().unwrap();
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layers[0][0],
        11
    );
    assert_eq!(
        settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layers[0][1],
        12
    );
    assert_eq!(settings.active_layout_id, application);
    assert_eq!(settings.editor_layout_id, application);
    assert_eq!(
        serde_json::to_value(&settings.layouts[&application]).unwrap(),
        app_before
    );
    assert_eq!(app.keycode_picker.macro_texts, vec![b"macro body".to_vec()]);
    assert_eq!(app.keycode_picker.macro_names, vec!["Macro name"]);
    assert_eq!(app.combo_entries[0].keys, [4, 5, 0, 0]);
    assert_eq!(
        serde_json::to_value(&app.app_settings.saved_pictograms).unwrap(),
        icons_before
    );
    let persisted: AppSettings =
        serde_json::from_str(&std::fs::read_to_string(app_settings_path()).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&persisted.application_layouts).unwrap(),
        serde_json::to_value(&app.app_settings.application_layouts).unwrap()
    );
}

#[test]
fn k04_standard_layout_round_trip_does_not_touch_saved_macropad_profiles() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    let mut keyboard = app.device_manager.devices()[0].clone();
    keyboard.name = "K:04".into();
    keyboard.product_id = 0x0004;
    app.device_manager.replace_devices(vec![keyboard]);
    let before = serde_json::to_value(&app.app_settings.application_layouts).unwrap();
    assert!(!app.application_layouts_supported());
    let bundle = app.entlayout_snapshot().unwrap();
    assert_eq!(bundle.keyboard.layers, 8);
    assert_eq!(bundle.data.keymap[0][0], 4);
    assert_eq!(bundle.data.layer_names[0], "Firmware 0");
    app.apply_entlayout(&bundle).unwrap();
    assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 4);
    assert_eq!(
        serde_json::to_value(&app.app_settings.application_layouts).unwrap(),
        before
    );
}

#[test]
fn macropad_menu_clicks_queue_standard_import_export_without_application_gating() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for (label_key, expected) in [
        ("Import layout", DeferredFullLayoutAction::ImportEntlayout),
        ("Export layout", DeferredFullLayoutAction::ExportEntlayout),
    ] {
        let (mut app, _, application) = fixture();
        app.application_layout_settings_mut()
            .unwrap()
            .active_layout_id = application;
        // Force the real action to queue instead of invoking an OS file picker.
        app.deferred_device_load = DeferredDeviceLoadState::complete(8);
        app.deferred_device_load
            .set_layer_status(0, DeferredLoadStatus::NotLoaded);
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(80.0, 30.0));
        let draw = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.draw_layout_device_dropdown(
                        ui,
                        &ctx,
                        crate::i18n::Language::English,
                        Some(rect),
                        true,
                        false,
                        false,
                    )
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let label = crate::i18n::tr_catalog(crate::i18n::Language::English, label_key);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("menu label missing: {label}"));
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.deferred_full_layout_action, Some(expected));
    }
}

#[test]
fn reviewed_macro_slot_overflow_rejected_on_import_and_export() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.macro_texts = vec![b"a".to_vec(), b"b".to_vec()];
    let bundle = app.entlayout_snapshot().unwrap();
    let path = storage.path().join("overflow.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert!(recorder.requests().is_empty());
    assert!(!storage.path().join("entropy/backups").exists());
    std::fs::write(&path, b"keep").unwrap();
    app.write_entlayout_export(&path);
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
}

#[test]
fn reviewed_macro_capacity_counts_terminators_before_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.keycode_picker.macro_count = 2;
    app.keycode_picker.macro_texts = vec![b"abc".to_vec(), b"de".to_vec()];
    recorder.respond_by(|request| {
        if request[0] == 0x0d {
            let mut r = [0; 32];
            r[2] = 6;
            Some(Ok(r))
        } else {
            None
        }
    });
    let bundle = app.entlayout_snapshot().unwrap();
    let path = storage.path().join("overflow.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert!(!storage.path().join("entropy/backups").exists());
    assert!(recorder.requests().iter().all(|r| r[0] == 0x0d));
    std::fs::write(&path, b"keep").unwrap();
    app.write_entlayout_export(&path);
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
}

#[test]
fn reviewed_changed_macro_upload_requires_readback() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.macro_texts = vec![b"old".to_vec()];
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.data.macros.bytecode_base64 = vec![BASE64_STANDARD.encode(b"changed")];
    bundle.data.keymap = app
        .layout
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.iter().map(|b| b.vial_keycode()).collect())
        .collect();
    let memory = std::sync::Arc::new(std::sync::Mutex::new(vec![0u8; 64]));
    let firmware = memory.clone();
    recorder.respond_by(move |request| {
        let mut response = [0; 32];
        match request[0] {
            0x0d => {
                response[2] = 64;
                Some(Ok(response))
            }
            0x0f => {
                let offset = u16::from_be_bytes([request[1], request[2]]) as usize;
                let len = request[3] as usize;
                firmware.lock().unwrap()[offset..offset + len]
                    .copy_from_slice(&request[4..4 + len]);
                Some(Ok(response))
            }
            0x0e => {
                let offset = u16::from_be_bytes([request[1], request[2]]) as usize;
                let len = request[3] as usize;
                response[4..4 + len]
                    .copy_from_slice(&firmware.lock().unwrap()[offset..offset + len]);
                Some(Ok(response))
            }
            _ => None,
        }
    });
    let path = storage.path().join("changed.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    app.import_entlayout_from_path(&path).unwrap();
    assert_eq!(&memory.lock().unwrap()[..8], b"changed\0");
    assert!(recorder.requests().iter().any(|r| r[0] == 0x0f));
    assert!(
        recorder.requests().iter().any(|r| r[0] == 0x0e),
        "upload was never read back"
    );
}

#[test]
fn reviewed_source_dependencies_checked_in_firmware_and_shared_resources() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.tap_dance_entries = vec![crate::keycode_picker::TapDanceEntry::default()];
    app.keycode_picker.macro_texts.clear();
    app.keycode_picker.macro_names.clear();
    assert!(app
        .validate_entlayout_file(&app.entlayout_snapshot().unwrap())
        .is_ok());
    for variant in 0..4 {
        let mut bundle = app.entlayout_snapshot().unwrap();
        match variant {
            0 => bundle.data.tap_dance.entries[0].on_tap = 0x7700,
            1 => bundle.data.keymap[0][0] = 0x7700,
            2 => bundle.data.keymap[0][0] = 0x7eff,
            _ => bundle.data.encoder_keymap[0][0] = 0x7eff,
        }
        assert!(
            app.validate_entlayout_file(&bundle).is_err(),
            "accepted variant {variant}"
        );
    }
}

#[test]
fn reviewed_merged_application_identity_uses_runtime_normalization() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (app, _, application) = fixture();
    let mut bundle = app.entlayout_snapshot().unwrap();
    let profiles = bundle.application_layouts.as_mut().unwrap();
    let mut duplicate = profiles[&application].clone();
    duplicate.id = "duplicate".into();
    duplicate.title_contains = " WINDOW ".into();
    profiles.get_mut(&application).unwrap().title_contains = "window".into();
    profiles.insert(duplicate.id.clone(), duplicate);
    assert!(app.validate_entlayout_file(&bundle).is_err());
}

#[test]
fn reviewed_each_sidecar_failure_prevents_publication_and_preserves_reload() {
    for variant in 0..6 {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, recorder, _) = fixture();
        let paths = [
            macro_metadata_path(&app.current_device_name),
            combo_names_path(&app.current_device_name),
            combo_colors_path(&app.current_device_name),
            tap_dance_names_path(&app.current_device_name),
            key_override_names_path(&app.current_device_name),
            alt_repeat_names_path(&app.current_device_name),
        ];
        save_app_settings(&app.app_settings);
        let before = std::fs::read(app_settings_path()).unwrap();
        for (index, path) in paths.iter().enumerate() {
            if index == variant {
                std::fs::create_dir_all(path).unwrap();
            } else {
                std::fs::write(path, b"[]").unwrap();
            }
        }
        let path = storage.path().join("sidecars.entlayout");
        let bundle = app.entlayout_snapshot().unwrap();
        write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
        assert!(
            app.import_entlayout_from_path(&path).is_err(),
            "sidecar {variant} silently lost"
        );
        assert!(
            recorder.requests().iter().all(|r| r[0] == 0x0d),
            "{:?}",
            recorder.requests()
        );
        assert_eq!(std::fs::read(app_settings_path()).unwrap(), before);
        for (index, path) in paths.iter().enumerate() {
            if index != variant {
                assert_eq!(std::fs::read(path).unwrap(), b"[]");
            }
        }
        assert_eq!(
            serde_json::to_value(load_app_settings()).unwrap(),
            serde_json::from_slice::<serde_json::Value>(&before).unwrap()
        );
    }
}

#[test]
fn reviewed_unified_export_excludes_global_text_expansion() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    app.app_settings.text_expander_app_blacklist = "private-global-app".into();
    let exported = app.entlayout_export_json().unwrap().unwrap();
    assert!(!exported.contains("private-global-app"));
}

#[test]
fn reviewed_category_definitions_round_trip_without_rebinding_target_category() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut source, _, application) = fixture();
    let category = source
        .application_layout_settings_mut()
        .unwrap()
        .create_category("Source category")
        .unwrap();
    source
        .application_layout_settings_mut()
        .unwrap()
        .set_layout_category_id(&application, &category);
    let path = storage.path().join("category.entlayout");
    source.write_entlayout_export(&path);
    let (mut target, _, _) = fixture();
    let unrelated = target
        .application_layout_settings_mut()
        .unwrap()
        .create_category("Unrelated category")
        .unwrap();
    assert_eq!(category, unrelated);
    target.import_entlayout_from_path(&path).unwrap();
    let reloaded = load_app_settings();
    let settings = &reloaded.application_layouts[&target.application_layout_device_key().unwrap()];
    let imported = settings.category_id_for_layout(&settings.layouts[&application]);
    assert_ne!(imported, unrelated);
    assert_eq!(settings.category_names[&imported], "Source category");
    assert_eq!(settings.category_names[&unrelated], "Unrelated category");
}

#[test]
fn reviewed_native_resource_dependencies_cannot_bypass_source_limits() {
    use rmk_types::action::{Action, KeyAction};
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (app, _, _) = fixture();
    for action in [
        KeyAction::Single(Action::TriggerMacro(0)),
        KeyAction::Morse(0),
        KeyAction::Single(Action::LayerOn(15)),
    ] {
        let mut bundle = app.entlayout_snapshot().unwrap();
        bundle.data.native_keymap[0][0] = encode_native_key_action(action);
        assert!(
            app.validate_entlayout_file(&bundle).is_err(),
            "accepted native {action:?}"
        );
    }
}

#[test]
fn reviewed_sidecars_reload_every_shared_metadata_field() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, _) = fixture();
    app.keycode_picker.macro_count = 1;
    app.keycode_picker.tap_dance_entries = vec![crate::keycode_picker::TapDanceEntry::default()];
    app.combo_entries = vec![ComboEntry::default()];
    app.key_override_entries = vec![KeyOverrideEntry::default()];
    app.alt_repeat_entries = vec![AltRepeatKeyEntry::default()];
    let mut bundle = app.entlayout_snapshot().unwrap();
    bundle.data.macros.names = vec!["macro name".into()];
    bundle.data.macros.descriptions = vec!["macro description".into()];
    bundle.data.combos.names = vec!["combo name".into()];
    bundle.data.combos.colors = vec![0x123456];
    bundle.data.tap_dance.names = vec!["tap name".into()];
    bundle.data.key_overrides.names = vec!["override name".into()];
    bundle.data.alt_repeat.names = vec!["repeat name".into()];
    let path = storage.path().join("metadata.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    app.import_entlayout_from_path(&path).unwrap();
    let name = &app.current_device_name;
    assert_eq!(load_macro_metadata(name).names, vec!["macro name"]);
    assert_eq!(
        load_macro_metadata(name).descriptions,
        vec!["macro description"]
    );
    assert_eq!(load_combo_names(name), vec!["combo name"]);
    assert_eq!(load_combo_colors(name), vec![0x123456]);
    assert_eq!(load_tap_dance_names(name), vec!["tap name"]);
    assert_eq!(load_key_override_names(name), vec!["override name"]);
    assert_eq!(load_alt_repeat_names(name), vec!["repeat name"]);
}

#[test]
fn reviewed_sidecars_rollback_on_hardware_failure() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture();
    let paths = [
        macro_metadata_path(&app.current_device_name),
        combo_names_path(&app.current_device_name),
        combo_colors_path(&app.current_device_name),
        tap_dance_names_path(&app.current_device_name),
        key_override_names_path(&app.current_device_name),
        alt_repeat_names_path(&app.current_device_name),
    ];
    for path in &paths {
        std::fs::write(path, b"[]").unwrap();
    }
    save_app_settings(&app.app_settings);
    let before = std::fs::read(app_settings_path()).unwrap();
    let bundle = app.entlayout_snapshot().unwrap();
    let path = storage.path().join("failure.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    recorder.respond_by(|request| {
        if request[0] == 0x05 {
            Some(Err("injected transfer failure".into()))
        } else {
            None
        }
    });
    assert!(app.import_entlayout_from_path(&path).is_err());
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), before);
    for path in &paths {
        assert_eq!(std::fs::read(path).unwrap(), b"[]");
    }
}

#[test]
fn reviewed_duplicate_and_missing_category_definitions_rejected_before_writes() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, application) = fixture();
    let category = app
        .application_layout_settings_mut()
        .unwrap()
        .create_category("Category")
        .unwrap();
    app.application_layout_settings_mut()
        .unwrap()
        .set_layout_category_id(&application, &category);
    let original = app.entlayout_snapshot().unwrap();
    for variant in 0..3 {
        let mut bundle = original.clone();
        match variant {
            0 => bundle
                .application_categories
                .push(bundle.application_categories[0].clone()),
            1 => bundle.application_categories.clear(),
            _ => bundle.application_categories[0].1.clear(),
        }
        let path = storage.path().join("category-invalid.entlayout");
        write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
        assert!(app.import_entlayout_from_path(&path).is_err());
        assert!(recorder.requests().is_empty());
        assert!(!storage.path().join("entropy/backups").exists());
    }
}

#[test]
fn reviewed_sidecar_partial_persist_failure_rolls_back_existing_and_new_files() {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let existing = storage.path().join("existing.json");
    let new = storage.path().join("new.json");
    let blocked = storage.path().join("blocked.json");
    std::fs::write(&existing, b"[\"original\"]").unwrap();
    {
        let mut sidecars = EntLayoutSidecars::default();
        for path in [&existing, &new, &blocked] {
            sidecars.stage(path.clone(), &vec!["imported"]).unwrap();
        }
        // Fail a late rename after earlier entries have already been committed.
        std::fs::create_dir(&blocked).unwrap();
        assert!(sidecars.persist().is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"[\"imported\"]");
    }
    assert_eq!(std::fs::read(&existing).unwrap(), b"[\"original\"]");
    assert!(!new.exists());
    assert!(blocked.is_dir());
}

#[test]
fn reviewed_final_merged_rules_check_alias_overlap_for_existing_ids() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = fixture();
    let other = app
        .application_layout_settings_mut()
        .unwrap()
        .create_for_application(&DetectedApplication {
            executable: "other-app".into(),
            display_name: "Other".into(),
            ..Default::default()
        });
    app.application_layout_settings_mut()
        .unwrap()
        .layouts
        .get_mut(&other)
        .unwrap()
        .application_identities = vec!["entropy".into()];
    let mut bundle = app.entlayout_snapshot().unwrap();
    // Other is not imported, but remains part of the resulting device set.
    bundle.application_layouts.as_mut().unwrap().remove(&other);
    assert!(bundle
        .application_layouts
        .as_ref()
        .unwrap()
        .contains_key(&application));
    assert!(app.validate_entlayout_file(&bundle).is_err());
}

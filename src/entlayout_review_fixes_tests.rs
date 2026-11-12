#[test]
fn complete_import_names_accepted_set_ack_error_blocks_next_menu_export() {
    for sm62 in [false, true] {
        review_names_failure(sm62, 0);
    }
}

#[test]
fn complete_import_names_ignored_set_is_rejected_before_publication() {
    for sm62 in [false, true] {
        review_names_failure(sm62, 1);
    }
}

#[test]
fn complete_import_names_early_success_later_failure_keeps_uncertainty() {
    for sm62 in [false, true] {
        review_names_failure(sm62, 2);
    }
}

fn review_names_failure(sm62: bool, fault: u8) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture_for(sm62);
    app.current_device_name = if sm62 { "SM62" } else { "M4CR0Pad v3" }.into();
    app.app_settings.language = crate::i18n::Language::English;
    app.deferred_device_load = DeferredDeviceLoadState::complete(app.layer_count);
    app.supported_qmk_settings = vec![200, 201];
    let ctx = egui::Context::default();
    app.tick_layout_import_lifecycle_for_test(&ctx, 1.);
    let mut bundle = app.entlayout_snapshot_with_profiles(None).unwrap();
    bundle.data.keymap = app
        .layout
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.iter().map(|b| b.vial_keycode()).collect())
        .collect();
    bundle.data.encoder_keymap = app.layout.as_ref().unwrap().encoder_layers.clone();
    // Empty names must be verified too, not silently skipped.
    bundle.data.layer_names[0] = String::new();
    bundle
        .application_layouts
        .as_mut()
        .unwrap()
        .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap()
        .layer_names[0] = String::new();
    let names = std::sync::Arc::new(std::sync::Mutex::new(vec![
        "old zero".to_owned(),
        "old one".to_owned(),
    ]));
    let physical = names.clone();
    let reads_fail = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let failing = reads_fail.clone();
    recorder.respond_as_legacy(move |r| {
        if r[0] != 0xfe || !matches!(r[1], 0x0a | 0x0b) {
            return None;
        }
        let q = u16::from_le_bytes([r[2], r[3]]);
        if !(200..=201).contains(&q) {
            return None;
        }
        let layer = (q - 200) as usize;
        if r[1] == 0x0b {
            if fault != 1 {
                let end = r[4..].iter().position(|v| *v == 0).unwrap_or(28);
                physical.lock().unwrap()[layer] =
                    String::from_utf8(r[4..4 + end].to_vec()).unwrap();
            }
            if fault == 0 || (fault == 2 && layer == 1) {
                failing.store(true, std::sync::atomic::Ordering::SeqCst);
                return Some(Err("name SET accepted but ack failed".into()));
            }
            return Some(Ok([0; 32]));
        }
        if failing.load(std::sync::atomic::Ordering::SeqCst) {
            return Some(Err("name GET unavailable".into()));
        }
        let mut response = [0; 32];
        let current = physical.lock().unwrap()[layer].clone();
        response[1..1 + current.len()].copy_from_slice(current.as_bytes());
        Some(Ok(response))
    });
    save_app_settings(&app.app_settings);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let persisted = std::fs::read(app_settings_path()).unwrap();
    let sidecar = layer_names_path(&app.current_device_name);
    std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    std::fs::write(&sidecar, b"unchanged sidecar").unwrap();
    let path = storage.path().join("names.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    review_menu(&mut app, &ctx, "Import layout", &path);
    if fault != 1 {
        assert_eq!(names.lock().unwrap()[0], "");
    }
    assert!(app.import_report_open);
    assert!(
        app.status_msg.contains("failed"),
        "ignored SET incorrectly published host profiles: {}",
        app.status_msg
    );
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), persisted);
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"unchanged sidecar");
    assert!(
        app.selected_layer_names_uncertain(),
        "accepted or unverified name SET lost captured-target uncertainty"
    );
    app.import_report_open = false;
    reads_fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let export = storage.path().join("next.entlayout");
    std::fs::write(&export, b"preserve destination").unwrap();
    review_menu(&mut app, &ctx, "Export layout", &export);
    assert_eq!(std::fs::read(&export).unwrap(), b"preserve destination");
    let sets = recorder
        .requests()
        .iter()
        .filter(|r| r[..2] == [0xfe, 0x0b])
        .count();
    reads_fail.store(false, std::sync::atomic::Ordering::SeqCst);
    review_menu(&mut app, &ctx, "Export layout", &export);
    let recovered: EntLayoutFile =
        serde_json::from_slice(&std::fs::read(&export).unwrap()).unwrap();
    // Complete exports intentionally retain Default profile drafts. Recovery
    // confirms physical names without publishing the aborted import's profiles.
    assert_eq!(app.layer_names[0], names.lock().unwrap()[0]);
    assert_eq!(
        recovered.application_layouts.as_ref().unwrap()[DEFAULT_APPLICATION_LAYOUT_ID].layer_names
            [0],
        app.application_layout_settings().unwrap().layouts[DEFAULT_APPLICATION_LAYOUT_ID]
            .layer_names[0]
    );
    assert!(!app.selected_layer_names_uncertain());
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|r| r[..2] == [0xfe, 0x0b])
            .count(),
        sets,
        "export retried SET"
    );
}

fn review_menu(app: &mut EntropyApp, ctx: &egui::Context, label: &str, path: &std::path::Path) {
    let draw = |app: &mut EntropyApp, events| {
        crate::app::discard_texture_deltas(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 900.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.draw_layout_device_dropdown(
                    ui,
                    ctx,
                    crate::i18n::Language::English,
                    Some(egui::Rect::from_min_size(
                        egui::pos2(400., 0.),
                        egui::vec2(80., 30.),
                    )),
                    true,
                    false,
                    false,
                )
            },
        ))
    };
    draw(app, vec![]);
    let out = draw(app, vec![]);
    let pos = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing menu {label}: {}", app.status_msg));
    crate::app::file_dialog::set_test_dialog_result(Some(path.to_path_buf()));
    for pressed in [true, false] {
        draw(
            app,
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
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(30);
    loop {
        app.tick_layout_import_lifecycle_for_test(
            ctx,
            2. + started.elapsed().as_secs_f64(),
        );
        app.poll_file_dialog(ctx);
        if !app.deferred_full_layout_action_pending()
            && app.pending_file_dialog.is_none()
            && !app.import_pending()
            && !app.vial_hid_task_active()
            && app.gui_entlayout_preparation.is_none()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "menu lifecycle stalled: {}",
            app.status_msg
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn complete_import_touchpad_accepted_set_failed_get_blocks_next_menu_export() {
    review_portable_failure(121, false);
}
#[test]
fn complete_import_module_accepted_set_failed_get_blocks_next_menu_export() {
    review_portable_failure(230, false);
}
#[test]
fn complete_import_behavior_accepted_set_failed_get_blocks_next_menu_export() {
    review_portable_failure(3, false);
}
#[test]
fn complete_import_staged_touchpad_failed_get_cannot_confirm_defaults() {
    review_portable_failure(121, true);
}

#[test]
fn complete_import_staged_module_export_schedules_failed_read_recovery() {
    review_portable_failure(230, true);
}

fn review_portable_failure(qsid: u16, staged: bool) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, recorder, _) = fixture_for(true);
    app.current_device_name = "SM62".into();
    app.app_settings.language = crate::i18n::Language::English;
    app.deferred_device_load = DeferredDeviceLoadState::complete(16);
    app.supported_qmk_settings = vec![qsid];
    if qsid == 3 {
        app.auto_shift_options = AutoShiftOptionsState::from_bits(1);
    }
    if qsid == 230 {
        app.module_settings.supported = true;
        app.module_settings.values.insert(qsid, 1);
        app.module_settings.groups = vec![ModuleSettingsGroup {
            title: "Left Modules".into(),
            kind: ModuleSettingsGroupKind::Left,
            fields: vec![ModuleSettingField {
                title: "Sensitivity".into(),
                qsid,
                kind: ModuleSettingKind::Integer,
                bit: 0,
                layout_option: None,
                width: 1,
                min: 0,
                max: 255,
                variants: vec![],
            }],
        }];
    }
    if staged {
        let definition = if qsid == 230 {
            app.supported_qmk_settings = vec![230];
            let json = serde_json::json!({"settings":[{"name":"Left Modules","fields":[{
                "title":"Sensitivity","type":"integer","qsid":230,"min":0,"max":255
            }]}]});
            app.module_settings =
                EntropyApp::module_settings_from_definition(&json, &app.supported_qmk_settings);
            assert!(app.module_settings.supported);
            assert_eq!(app.module_settings.fields.len(), 1);
            app.module_settings.values.insert(qsid, 1);
            json
        } else {
            app.supported_qmk_settings = vec![120, 121, 122, 123, 124];
            serde_json::json!({"settings":[{"name":"Touchpad","fields":[{"qsid":120},{"qsid":121},{"qsid":122},{"qsid":123},{"qsid":124}]}]})
        };
        app.deferred_device_load = DeferredDeviceLoadState::staged(DeferredDeviceLoadContext { supports_universal_numero: false,
            json: std::sync::Arc::new(definition),
            supported_qmk_settings: std::sync::Arc::new(app.supported_qmk_settings.clone()),
            definition_fingerprint: 42,
            layer_count: 16,
            rows: 10,
            cols: 7,
            encoder_count: 0,
            macro_count: 0,
            macro_memory_bytes: Some(0),
            tap_dance_count: 0,
            combo_count: 0,
            key_override_count: 0,
            alt_repeat_count: 0,
            modules_supported: qsid == 230,
            touchpad_supported: qsid == 121,
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
        for l in 0..16 {
            app.deferred_device_load
                .set_layer_status(l, DeferredLoadStatus::Loaded);
        }
        app.deferred_device_load.set_section_status(
            if qsid == 230 {
                DeferredLoadSection::Modules
            } else {
                DeferredLoadSection::Touchpad
            },
            DeferredLoadStatus::Loaded,
        );
    }
    app.touchpad_settings.loaded_qsids.mark(121);
    app.touchpad_settings.sniper_sens = 1;
    let ctx = egui::Context::default();
    app.tick_layout_import_lifecycle_for_test(&ctx, 1.);
    let mut bundle = app.entlayout_snapshot_with_profiles(None).unwrap();
    bundle.data.keymap = app
        .layout
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.iter().map(|b| b.vial_keycode()).collect())
        .collect();
    bundle.data.encoder_keymap = app.layout.as_ref().unwrap().encoder_layers.clone();
    assert_eq!(bundle.data.portable_settings.len(), 1);
    bundle.data.portable_settings[0].value = 2;
    let physical = std::sync::Arc::new(std::sync::Mutex::new(1u8));
    let memory = physical.clone();
    let get_failure = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let fail = get_failure.clone();
    recorder.respond_as_legacy(move |r| {
        if r[..4] == [0xfe, 0x0b, qsid as u8, (qsid >> 8) as u8] {
            *memory.lock().unwrap() = r[4];
            return Some(if qsid == 3 {
                Err("accepted SET ack error".into())
            } else {
                Ok([0; 32])
            });
        }
        if r[..4] == [0xfe, 0x0a, qsid as u8, (qsid >> 8) as u8] {
            if fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Some(Err("accepted SET, GET unavailable".into()));
            }
            let mut response = [0; 32];
            response[1] = *memory.lock().unwrap();
            return Some(Ok(response));
        }
        if r[..2] == [0xfe, 0x0a] && (120..=124).contains(&u16::from_le_bytes([r[2], r[3]])) {
            return Some(Ok([0; 32]));
        }
        None
    });
    save_app_settings(&app.app_settings);
    let before = serde_json::to_value(&app.app_settings).unwrap();
    let persisted = std::fs::read(app_settings_path()).unwrap();
    let path = storage.path().join("portable.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    review_menu(&mut app, &ctx, "Import layout", &path);
    assert_eq!(
        *physical.lock().unwrap(),
        2,
        "test never reached accepted SET"
    );
    assert!(
        app.import_report_open,
        "missing localized failure report: {}",
        app.status_msg
    );
    assert!(
        app.import_report_body.contains("failed"),
        "{}",
        app.import_report_body
    );
    assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
    assert_eq!(std::fs::read(app_settings_path()).unwrap(), persisted);
    assert_eq!(
        app.touchpad_settings.sniper_sens, 1,
        "draft/cache rollback changed"
    );
    app.import_report_open = false;
    let export = storage.path().join("next.entlayout");
    std::fs::write(&export, b"preserve destination").unwrap();
    review_menu(&mut app, &ctx, "Export layout", &export);
    assert_eq!(
        std::fs::read(&export).unwrap(),
        b"preserve destination",
        "failed portable SET/readback left stale value exportable"
    );
    if staged && qsid == 230 {
        assert!(
            matches!(
                app.deferred_device_load
                    .section_status(DeferredLoadSection::Modules),
                DeferredLoadStatus::Failed(_)
            ),
            "ordinary export never scheduled the unconfirmed module GET: {:?}",
            app.deferred_device_load
                .section_status(DeferredLoadSection::Modules)
        );
    }
    assert!(
        staged || app.status_msg.contains("Reconnect") || app.status_msg.contains("reconnect"),
        "eager recovery must be actionable: {}",
        app.status_msg
    );
    assert!(!matches!(app.entlayout_export_json(), Some(Ok(_))));
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|r| r[..4] == [0xfe, 0x0b, qsid as u8, (qsid >> 8) as u8])
            .count(),
        1,
        "hidden SET retry"
    );
    if staged {
        get_failure.store(false, std::sync::atomic::Ordering::SeqCst);
        review_menu(&mut app, &ctx, "Export layout", &export);
        let recovered: EntLayoutFile =
            serde_json::from_slice(&std::fs::read(&export).unwrap()).unwrap();
        assert_eq!(
            recovered
                .data
                .portable_settings
                .iter()
                .find(|s| s.qsid == qsid)
                .unwrap()
                .value,
            2
        );
        assert_eq!(
            if qsid == 230 {
                app.module_settings.values[&qsid]
            } else {
                u16::from(app.touchpad_settings.sniper_sens)
            },
            2,
            "read-only recovery did not confirm physical setting"
        );
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|r| r[..4] == [0xfe, 0x0b, qsid as u8, (qsid >> 8) as u8])
                .count(),
            1,
            "read-only export retried SET"
        );
        // Once recovered, changing back to the formerly confirmed value must
        // write, not take the stale equality shortcut.
        let mut correction = recovered;
        correction
            .data
            .portable_settings
            .iter_mut()
            .find(|s| s.qsid == qsid)
            .unwrap()
            .value = 1;
        write_entlayout_file(&path, &correction, app.app_settings.language).unwrap();
        review_menu(&mut app, &ctx, "Import layout", &path);
        assert_eq!(*physical.lock().unwrap(), 1, "corrective SET suppressed");
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|r| r[..4] == [0xfe, 0x0b, qsid as u8, (qsid >> 8) as u8])
                .count(),
            2
        );
    }
}

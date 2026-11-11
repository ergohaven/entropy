// Real menu and serialized worker; only OS HID and native portal are scripted.
// Source-first regression; execution deliberately deferred to parent checkpoint.
#[cfg(target_os = "linux")]
#[test]
fn gui_pending_import_waits_for_durable_resident_backup_and_refuses_stale_or_read_error() {
    for failure in ["", "stale", "read"] {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let (mut app, session, _) = scripted_export_device_app(|_| None, true);
        // One writable OS transport from connect through backup and apply; the
        // read-only exporter is used only to construct a valid source document.
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.connect_hid = Some(hid);
        resident_export_peer(&recorder, "");
        let json = export_entlayout(&mut app, 0, &session, deadline(), &AtomicBool::default()).unwrap();
        // Settle ordinary catalog initialization before measuring import-only
        // preservation; built-in local profiles must also survive the backup.
        app.tick_layout_import_lifecycle_for_test(&egui::Context::default(), 0.0);
        let author_ids: Vec<_> = app.application_layout_settings().into_iter()
            .flat_map(|settings| settings.layouts.keys().cloned()).collect();
        let request_cursor = recorder.requests().len();
        // Legacy device input exercises native SET without importing host profiles.
        let mut source: serde_json::Value = serde_json::from_str(&json).unwrap();
        source["version"] = 7.into();
        source.as_object_mut().unwrap().remove("application_layouts");
        source.as_object_mut().unwrap().remove("application_categories");
        // This legacy input isolates native keys; it must not carry a custom
        // icon package whose independent capability admission would fail first.
        source.as_object_mut().unwrap().remove("pictograms");
        source["data"]["keymap"][0][0] = 5.into();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.entlayout");
        let source_bytes = serde_json::to_vec(&source).unwrap();
        std::fs::write(&path, &source_bytes).unwrap();
        let backup_dir = storage.path().join("entropy/backups");
        std::fs::create_dir_all(&backup_dir).unwrap();
        let valuable = backup_dir.join("valuable.entlayout");
        std::fs::write(&valuable, b"previous complete backup").unwrap();
        let before = serde_json::to_value(&app.app_settings).unwrap();
        let settings_path = crate::app::app_storage::app_settings_path();
        let before_disk = std::fs::read(&settings_path).ok();
        app.vial_unlocked = Some(true);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut gate = Some(release_rx);
        let durable_dir = backup_dir.clone();
        let saw_set = std::sync::Arc::new(AtomicBool::default());
        let set_signal = saw_set.clone();
        preimport_resident_peer(&recorder, failure == "read", move |p| {
            if p[..3] == [0xef, 9, 6] {
                if let Some(gate) = gate.take() {
                    entered_tx.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                }
            }
            if is_write(p) {
                // Filesystem readback at the first real mutating packet, not
                // merely a report string or in-memory prepared-document flag.
                let backups: Vec<_> = std::fs::read_dir(&durable_dir).unwrap()
                    .map(|e| e.unwrap().path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("auto-backup-")).collect();
                assert_eq!(backups.len(), 1, "SET preceded durable backup");
                let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&backups[0]).unwrap()).unwrap();
                let profiles = saved["application_layouts"].as_object().unwrap();
                assert!(profiles.contains_key("default"));
                assert!(author_ids.iter().all(|id| profiles.contains_key(id)), "backup omitted configured profiles");
                for name in ["device-editor", "device-terminal"] {
                    assert!(profiles.values().any(|profile| profile["name"] == name), "backup omitted physical profile {name}");
                }
                set_signal.store(true, Ordering::Release);
            }
        });
        let ctx = egui::Context::default();
        app.begin_entlayout_import(path.clone());
        app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
        let end = Instant::now() + Duration::from_secs(1);
        while entered_rx.try_recv().is_err() && Instant::now() < end {
            app.tick_layout_import_lifecycle_for_test(&ctx, 1.1);
            std::thread::yield_now();
        }
        assert!(recorder.requests()[request_cursor..].iter().any(|p| p[..3] == [0xef, 9, 6]),
            "ordinary pending dispatch never reached backup READ: {}", app.status_msg);
        assert!(app.vial_hid_task_active(), "pending dispatch did not start backup worker: {}", app.status_msg);
        assert!(app.pending_entlayout_import_path.is_some(), "pending path removed before preparation");
        assert!(!saw_set.load(Ordering::Acquire));
        assert!(recorder.requests()[request_cursor..].iter().all(|p| !is_write(p)));
        assert!(app.pending_file_dialog.is_none(), "backup preparation opened export picker");
        if failure == "stale" { app.connection_generation = app.connection_generation.wrapping_add(1); }
        release_tx.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while app.vial_hid_task_active() || app.pending_entlayout_import_path.is_some() {
            assert!(Instant::now() < end, "pending import did not terminate: {}", app.status_msg);
            app.tick_layout_import_lifecycle_for_test(&ctx, 1.2);
            std::thread::yield_now();
        }
        assert_eq!(std::fs::read(&path).unwrap(), source_bytes);
        assert_eq!(std::fs::read(&valuable).unwrap(), b"previous complete backup");
        if failure.is_empty() {
            assert!(saw_set.load(Ordering::Acquire), "prepared import never applied: {}", app.status_msg);
            assert_eq!(app.layout.as_ref().unwrap().get_key_binding(0, 0).vial_keycode(), 5);
        } else {
            assert!(!saw_set.load(Ordering::Acquire));
            assert_eq!(serde_json::to_value(&app.app_settings).unwrap(), before);
            assert_eq!(std::fs::read(&settings_path).ok(), before_disk);
            assert!(app.import_report_open, "backup failure must terminate visibly");
        }
    }
}

// Stateful physical keymap plus immutable resident FULLs. All packet encoding,
// backup production, dispatch and filesystem publication remain production code.
#[cfg(target_os = "linux")]
fn preimport_resident_peer(
    recorder: &crate::hid::TestHidRecorder,
    fail_read: bool,
    mut observe: impl FnMut(&[u8; 32]) + Send + 'static,
) {
    let records = [(7u8, cold_resident_full("device-editor", 4)),
        (99u8, cold_resident_full("device-terminal", 5))];
    let mut keymap: Vec<u8> = (0..16*10*7*2).map(|byte|
        if byte % 2 == 0 { 0 } else { 4u8.wrapping_add(byte as u8 / 2) }).collect();
    scripted_keyboard_answering_geometry(recorder, move |p| {
        observe(p);
        let mut r = *p;
        match p[..3] {
            [0x05, layer, row] => {
                let offset = (usize::from(layer)*70 + usize::from(row)*7 + usize::from(p[3]))*2;
                keymap[offset..offset+2].copy_from_slice(&p[4..6]);
                return Some(Ok(r));
            }
            [0x13, high, low] => {
                let offset = usize::from(u16::from_be_bytes([high, low]));
                let n = usize::from(p[3]);
                keymap[offset..offset+n].copy_from_slice(&p[4..4+n]);
                return Some(Ok(r));
            }
            [0x12, high, low] => {
                let offset = usize::from(u16::from_be_bytes([high, low]));
                let n = usize::from(p[3]);
                r[4..4+n].copy_from_slice(&keymap[offset..offset+n]);
                return Some(Ok(r));
            }
            [0x04, layer, row] => {
                let offset = (usize::from(layer)*70 + usize::from(row)*7 + usize::from(p[3]))*2;
                r[4..6].copy_from_slice(&keymap[offset..offset+2]);
                return Some(Ok(r));
            }
            [0xeb, 8, 0xa5] if p[3] == 2 => {
                r[2] = 0x5a;
                r[8..15].copy_from_slice(&[2, 100, 0xd0, 0x11, 16, 62, 22]);
                r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
                r[20..24].copy_from_slice(&15u32.to_le_bytes()); r[24] = 2;
                return Some(Ok(r));
            }
            _ => {}
        }
        if p[..2] != [0xef, 9] { return None; }
        assert!(matches!(p[2], 3 | 6), "backup must be read-only: {p:?}");
        let record = records.iter().find(|(slot, _)| *slot == p[3]);
        r[2] |= 0x80;
        if p[2] == 3 {
            r[8..].fill(0); r[3] = if record.is_some() { 0 } else { 1 };
            if let Some((_, raw)) = record {
                r[8..28].copy_from_slice(&raw[43..63]);
                let crc = !raw.iter().fold(!0u32, |mut crc, b| {
                    crc ^= u32::from(*b);
                    for _ in 0..8 { crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 }; }
                    crc
                });
                r[28..].copy_from_slice(&crc.to_le_bytes());
            }
        } else {
            if fail_read { return Some(Err("injected resident READ failure".into())); }
            let raw = &record.unwrap().1;
            let offset = usize::from(u16::from_le_bytes(p[8..10].try_into().unwrap()));
            let n = (raw.len()-offset).min(usize::from(p[10]));
            r[10] = n as u8; r[11..].fill(0); r[12..12+n].copy_from_slice(&raw[offset..offset+n]);
        }
        Some(Ok(r))
    }, None, true);
}

#[cfg(target_os = "linux")]
fn click_resident_export_without_wait(app: &mut EntropyApp, ctx: &egui::Context) {
    let draw = |app: &mut EntropyApp, events| {
        crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0,900.0))),
            events, ..Default::default()
        }, |ui| app.draw_layout_device_dropdown(ui,ctx,crate::i18n::Language::English,
            Some(egui::Rect::from_min_size(egui::pos2(400.0,0.0),egui::vec2(80.0,30.0))),true,false,false)))
    };
    let _ = draw(app,vec![]);
    let output = draw(app,vec![]);
    let pos = output.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text() == "Export layout" => Some(t.pos+t.galley.size()*0.5), _ => None
    }).expect("ordinary export menu item");
    for pressed in [true,false] {
        let _ = draw(app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {
            pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE
        }]);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn gui_resident_prepicker_read_returns_ui_and_retains_document_through_picker() {
    gui_resident_preparation_barrier(false);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_resident_preparation_stale_connection_refuses_portal_and_destination() {
    gui_resident_preparation_barrier(true);
}

#[cfg(target_os = "linux")]
fn gui_resident_preparation_barrier(stale: bool) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,session,recorder) = scripted_export_device_app(|_|None,true);
    resident_export_peer(&recorder,"");
    export_entlayout(&mut app,0,&session,deadline(),&AtomicBool::default()).unwrap();
    let (entered_tx,entered_rx) = std::sync::mpsc::channel();
    let (release_tx,release_rx) = std::sync::mpsc::channel();
    let mut gate = Some(release_rx);
    resident_export_peer_observing(&recorder,"",move |p| {
        if p[..3] == [0xef,9,6] {
            if let Some(gate) = gate.take() {
                entered_tx.send(()).unwrap();
                // Bounded even on the old synchronous UI: release timeout is RED,
                // never a permanently blocked test process.
                let _ = gate.recv_timeout(Duration::from_secs(2));
            }
        }
    });
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("valuable.entlayout");
    std::fs::write(&path,b"previous complete backup").unwrap();
    crate::app::file_dialog::set_test_dialog_result(Some(path.clone()));
    let ctx = egui::Context::default();
    let started = Instant::now();
    click_resident_export_without_wait(&mut app,&ctx);
    while entered_rx.try_recv().is_err() && started.elapsed() < Duration::from_secs(1) {
        app.maybe_start_deferred_device_load(&ctx,false);
        app.poll_vial_hid_task(&ctx);
        app.finish_deferred_full_layout_action(&ctx);
        std::thread::yield_now();
    }
    let returned = started.elapsed() < Duration::from_secs(1);
    let borrowed = app.vial_hid_task_active();
    let no_portal = app.pending_file_dialog.is_none();
    if stale { app.connection_generation = app.connection_generation.wrapping_add(1); }
    let _ = release_tx.send(());
    assert!(returned,"ordinary menu click blocked on resident READ: status={}, worker={}, preparation={}",
        app.status_msg, borrowed, app.gui_entlayout_preparation.is_some());
    assert!(borrowed,"resident READ must borrow the existing serialized owner");
    assert!(no_portal,"portal opened before resident READ completed");
    let end = Instant::now()+Duration::from_secs(5);
    while app.vial_hid_task_active() {
        app.poll_vial_hid_task(&ctx);
        assert!(Instant::now()<end);
        std::thread::yield_now();
    }
    if stale {
        assert!(app.pending_file_dialog.is_none());
        assert_eq!(std::fs::read(&path).unwrap(),b"previous complete backup");
        crate::app::file_dialog::set_test_dialog_result(None);
    } else {
        assert!(app.pending_file_dialog.is_some(),"{}",app.status_msg);
        let reads = recorder.requests().iter().filter(|p|p[..3]==[0xef,9,6]).count();
        app.poll_file_dialog(&ctx);
        assert_eq!(recorder.requests().iter().filter(|p|p[..3]==[0xef,9,6]).count(),reads,
            "postpicker must consume retained document, never repeat resident READ");
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["application_layouts"].as_object().unwrap().len(),3);
    }
    assert!(recorder.requests().iter().all(|p| !is_write(p)));
}

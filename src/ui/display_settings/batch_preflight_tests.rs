//! Real App::ui and combo worker; only the HID transport/storage boundary is simulated.

#[test]
fn batch_preflight_rynk_keeps_its_existing_dynamic_write_policy() {
    use crate::rynk::session::{
        testing::{board, link, Stored},
        RynkSession,
    };
    let (mut app, ctx, _) = setup(false);
    let mut stored = Stored::new(1, 2, 1);
    stored.combos = vec![rmk_types_rynk::combo::Combo::empty(); 2];
    stored.morses = vec![rmk_types_rynk::morse::Morse::default(); 2];
    stored.capabilities.max_morse = 2;
    let (board, state) = board(stored);
    let session = RynkSession::new(link("batch-preflight-rynk", &board));
    session.read_morses().unwrap();
    app.hid_device = Some(session.into());
    app.firmware = crate::firmware::FirmwareProtocol::Rynk;
    app.keycode_picker.tap_dance_entries = vec![Default::default(); 2];
    app.keycode_picker.tap_dance_synced_entries = vec![Default::default(); 2];
    let rynk_action = KeyBinding::Rynk(rmk_types_rynk::action::KeyAction::Single(
        rmk_types_rynk::action::Action::User(144),
    ));
    for entry in &mut app.keycode_picker.tap_dance_entries {
        entry.on_tap = rynk_action;
    }
    app.keycode_picker.tap_dance_dirty = true;
    ui(&mut app, &ctx);
    assert_eq!(
        app.keycode_picker.tap_dance_entries, app.keycode_picker.tap_dance_synced_entries,
        "{}",
        app.status_msg
    );
    app.combo_synced_entries = vec![ComboEntry::default(); 2];
    app.combo_entries = vec![combo(rynk_action); 2];
    app.mark_combo_dirty();
    for _ in 0..2 {
        ui(&mut app, &ctx);
        drain_combo(&mut app, &ctx);
    }
    assert_eq!(
        app.combo_entries, app.combo_synced_entries,
        "{}",
        app.status_msg
    );
    assert_eq!(state.lock().unwrap().writes().len(), 4);
}

#[test]
fn batch_preflight_tap_dance_checks_every_field_of_late_entry() {
    for field in 0..4 {
        let (mut app, ctx, recorder) = setup(false);
        app.keycode_picker.tap_dance_synced_entries = vec![Default::default(); 2];
        app.keycode_picker.tap_dance_entries = vec![Default::default(); 2];
        app.keycode_picker.tap_dance_entries[0].on_hold = 5.into();
        let td = &mut app.keycode_picker.tap_dance_entries[1];
        let fields = [
            &mut td.on_tap,
            &mut td.on_hold,
            &mut td.on_double_tap,
            &mut td.on_tap_hold,
        ];
        *fields.into_iter().nth(field).unwrap() = crate::universal_symbols::binding(180);
        app.keycode_picker.tap_dance_dirty = true;
        let before = recorder.requests().len();
        ui(&mut app, &ctx);
        assert!(
            writes(&recorder, before).is_empty(),
            "late field {field} allowed a partial SET"
        );
        assert!(app
            .keycode_picker
            .tap_dance_synced_entries
            .iter()
            .all(|e| e == &Default::default()));
    }
}

use super::*;
use crate::app::ComboEntry;
use crate::keyboard::KeyBinding;

fn ui(app: &mut EntropyApp, ctx: &egui::Context) {
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
}

fn setup(numero: bool) -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
    let (app, ctx, recorder) = tap_dance_app();
    *app.hid_device
        .as_ref()
        .unwrap()
        .as_vial()
        .unwrap()
        .native_capabilities
        .lock()
        .unwrap() = Some(crate::rmk_native::RmkNativeCapabilities {
        version: 1,
        key_actions: true,
        tap_dance_actions: true,
        combo_output: true,
        universal_symbols: true,
        universal_numero: numero,
        ..Default::default()
    });
    let storage = std::sync::Arc::new(std::sync::Mutex::new([[0u8; 10]; 256]));
    let native = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    recorder.respond_by(move |r| {
        if r[..3] == [7, 232, 5] {
            native
                .lock()
                .unwrap()
                .insert((r[4], r[5], r[6]), r[8..8 + r[7] as usize].to_vec());
            let mut out = [0; 32];
            out[..4].copy_from_slice(&[7, 232, 5, r[3]]);
            Some(Ok(out))
        } else if r[..3] == [8, 232, 5] {
            let bytes = native
                .lock()
                .unwrap()
                .get(&(r[4], r[5], r[6]))
                .unwrap()
                .clone();
            let mut out = [0; 32];
            out[..4].copy_from_slice(&[8, 232, 5, r[3]]);
            out[5] = bytes.len() as u8;
            out[6..6 + bytes.len()].copy_from_slice(&bytes);
            Some(Ok(out))
        } else if r[..2] == [254, 13] && [2, 4].contains(&r[2]) {
            storage.lock().unwrap()[r[3] as usize].copy_from_slice(&r[4..14]);
            Some(Ok([0; 32]))
        } else if r[..2] == [254, 13] && [1, 3].contains(&r[2]) {
            let mut out = [0; 32];
            out[1..11].copy_from_slice(&storage.lock().unwrap()[r[3] as usize]);
            Some(Ok(out))
        } else {
            None
        }
    });
    (app, ctx, recorder)
}

fn writes(recorder: &crate::hid::TestHidRecorder, before: usize) -> Vec<[u8; 32]> {
    recorder.requests()[before..]
        .iter()
        .filter(|r| (r[..2] == [254, 13] && [2, 4].contains(&r[2])) || r[0] == 7)
        .cloned()
        .collect()
}

fn drain_combo(app: &mut EntropyApp, ctx: &egui::Context) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.combo_write_task.is_some() {
        assert!(std::time::Instant::now() < until, "combo worker timed out");
        app.poll_combo_write(ctx);
        std::thread::yield_now();
    }
}

#[test]
fn batch_preflight_tap_dance_legacy_broad_both_orders() {
    for bad_index in [1, 0] {
        let (mut app, ctx, recorder) = setup(false);
        let mut stored = vec![crate::keycode_picker::TapDanceEntry::default(); 2];
        stored[bad_index].on_tap = crate::universal_symbols::binding(180);
        app.keycode_picker.tap_dance_synced_entries = stored.clone();
        app.keycode_picker.tap_dance_entries = stored.clone();
        for entry in &mut app.keycode_picker.tap_dance_entries {
            entry.on_hold = 5.into();
        }
        let pending = app.keycode_picker.tap_dance_entries.clone();
        app.keycode_picker.tap_dance_dirty = true;
        app.keycode_picker.open = false;
        let before = recorder.requests().len();
        ui(&mut app, &ctx);
        assert!(
            app.status_msg.contains("Numero is not supported"),
            "{}",
            app.status_msg
        );
        assert!(
            writes(&recorder, before).is_empty(),
            "partial Tap Dance SET: {:?}",
            writes(&recorder, before)
        );
        assert_eq!(app.keycode_picker.tap_dance_synced_entries, stored);
        assert_eq!(
            app.keycode_picker.tap_dance_entries, pending,
            "stored unsupported action lost"
        );
    }
}

fn combo(output: KeyBinding) -> ComboEntry {
    ComboEntry {
        keys: [4, 5, 0, 0],
        output,
        layer: None,
    }
}

#[test]
fn batch_preflight_combo_legacy_broad_both_orders() {
    for bad_index in [1, 0] {
        let (mut app, ctx, recorder) = setup(false);
        let mut stored = vec![combo(6.into()); 2];
        stored[bad_index].output = crate::universal_symbols::binding(180);
        app.combo_synced_entries = stored.clone();
        app.combo_entries = stored.clone();
        for entry in &mut app.combo_entries {
            entry.keys[1] = 7;
        }
        let pending = app.combo_entries.clone();
        app.keycode_picker.open = false;
        app.mark_combo_dirty();
        let before = recorder.requests().len();
        ui(&mut app, &ctx);
        drain_combo(&mut app, &ctx);
        assert!(
            writes(&recorder, before).is_empty(),
            "partial Combo SET: {:?}; {}",
            writes(&recorder, before),
            app.status_msg
        );
        assert!(
            app.status_msg.contains("Numero is not supported"),
            "{}",
            app.status_msg
        );
        assert_eq!(app.combo_synced_entries, stored);
        assert_eq!(app.combo_entries, pending);
    }
}

#[test]
fn batch_preflight_numero_capable_valid_batches_both_orders() {
    for native_index in [0, 1] {
        let (mut app, ctx, recorder) = setup(true);
        app.keycode_picker.tap_dance_synced_entries = vec![Default::default(); 2];
        app.keycode_picker.tap_dance_entries = vec![Default::default(); 2];
        for td in &mut app.keycode_picker.tap_dance_entries {
            td.on_hold = 5.into();
        }
        app.keycode_picker.tap_dance_entries[native_index].on_tap =
            crate::universal_symbols::binding(180);
        app.keycode_picker.tap_dance_dirty = true;
        let before = recorder.requests().len();
        ui(&mut app, &ctx);
        assert_eq!(
            app.keycode_picker.tap_dance_entries, app.keycode_picker.tap_dance_synced_entries,
            "{}",
            app.status_msg
        );
        assert_eq!(writes(&recorder, before).len(), 3);
        app.combo_synced_entries = vec![ComboEntry::default(); 2];
        app.combo_entries = vec![combo(6.into()); 2];
        app.combo_entries[native_index].output = crate::universal_symbols::binding(180);
        app.mark_combo_dirty();
        let before = recorder.requests().len();
        for _ in 0..2 {
            ui(&mut app, &ctx);
            drain_combo(&mut app, &ctx);
        }
        assert_eq!(
            app.combo_entries, app.combo_synced_entries,
            "{}",
            app.status_msg
        );
        assert!(!app.combo_dirty);
        assert_eq!(writes(&recorder, before).len(), 3);
    }
}

#[test]
fn batch_preflight_combo_profile_mismatch_late_entry() {
    use rmk_types::action::{Action, KeyAction};
    let (mut app, ctx, recorder) = setup(true);
    *app.hid_device
        .as_ref()
        .unwrap()
        .as_vial()
        .unwrap()
        .native_capabilities
        .lock()
        .unwrap() = Some(crate::rmk_native::RmkNativeCapabilities {
        version: 2,
        key_actions: true,
        combo_output: true,
        profile_resolution: true,
        ..Default::default()
    });
    let storage = std::sync::Arc::new(std::sync::Mutex::new([0; 10]));
    recorder.respond_by(move |r| {
        if r[..3] == [8, 232, 8] {
            let mut out = [0; 32];
            out[..8].copy_from_slice(&[8, 232, 8, 2, 0, r[4], 1, 0]);
            out[8..16].copy_from_slice(&180u64.to_le_bytes());
            out[16..24].copy_from_slice(&200u64.to_le_bytes());
            out[24] = 1;
            Some(Ok(out))
        } else if r[..3] == [254, 13, 4] {
            storage.lock().unwrap().copy_from_slice(&r[4..14]);
            Some(Ok([0; 32]))
        } else if r[..3] == [254, 13, 3] {
            let mut out = [0; 32];
            out[1..11].copy_from_slice(&*storage.lock().unwrap());
            Some(Ok(out))
        } else {
            None
        }
    });
    app.combo_synced_entries = vec![ComboEntry::default(); 2];
    app.combo_entries = vec![
        combo(6.into()),
        combo(KeyAction::TapHold(Action::User(144), Action::User(145), 999u32.into()).into()),
    ];
    app.mark_combo_dirty();
    let baseline = app.combo_synced_entries.clone();
    let pending = app.combo_entries.clone();
    let before = recorder.requests().len();
    ui(&mut app, &ctx);
    drain_combo(&mut app, &ctx);
    assert!(
        writes(&recorder, before).is_empty(),
        "profile mismatch caused partial writes"
    );
    assert_eq!(app.combo_synced_entries, baseline);
    assert_eq!(app.combo_entries, pending);
    assert!(app.status_msg.contains("profile"), "{}", app.status_msg);
    assert!(recorder.requests()[before..]
        .iter()
        .any(|r| r[..3] == [8, 232, 8]));
}

#[test]
fn batch_preflight_combo_new_revision_while_worker_borrowed() {
    let (mut app, ctx, recorder) = setup(false);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = std::sync::Mutex::new(release_rx);
    let storage = std::sync::Mutex::new([0u8; 10]);
    let first_write = std::sync::atomic::AtomicBool::new(true);
    recorder.respond_by(move |r| {
        if r[..3] == [254, 13, 4] {
            if first_write.swap(false, std::sync::atomic::Ordering::SeqCst) {
                started_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }
            storage.lock().unwrap().copy_from_slice(&r[4..14]);
            Some(Ok([0; 32]))
        } else if r[..3] == [254, 13, 3] {
            let mut out = [0; 32];
            out[1..11].copy_from_slice(&*storage.lock().unwrap());
            Some(Ok(out))
        } else {
            None
        }
    });
    app.combo_synced_entries = vec![ComboEntry::default(); 2];
    app.combo_entries = vec![combo(6.into()), combo(7.into())];
    app.mark_combo_dirty();
    ui(&mut app, &ctx);
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(app.hid_device.is_none());
    app.combo_entries[0].output = 8.into();
    app.combo_entries[1].output = crate::universal_symbols::binding(180);
    app.mark_combo_dirty();
    let revision = app.combo_edit_revision;
    release_tx.send(()).unwrap();
    drain_combo(&mut app, &ctx);
    assert_eq!(app.combo_synced_entries[0].output, 6.into());
    assert_eq!(app.combo_entries[0].output, 8.into());
    let synced = app.combo_synced_entries.clone();
    let before = recorder.requests().len();
    ui(&mut app, &ctx);
    drain_combo(&mut app, &ctx);
    assert!(
        writes(&recorder, before).is_empty(),
        "new revision reused stale batch approval"
    );
    assert_eq!(app.combo_synced_entries, synced);
    assert_eq!(app.combo_attempted_revision, Some(revision));
    assert!(app.combo_dirty);
    assert!(app.status_msg.contains("Numero is not supported"));
    let before = recorder.requests().len();
    ui(&mut app, &ctx);
    assert_eq!(
        recorder.requests().len(),
        before,
        "failed revision must not retry"
    );
}

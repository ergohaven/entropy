use super::*;
use crate::hid::{HidDevice, TestHidRecorder};
fn setup() -> (EntropyApp, TestHidRecorder, tempfile::TempDir) {
    let mut app = EntropyApp::new_inert_for_test();
    let device = crate::device::test_usb_device("fps-synthetic", 217, 0x00b2);
    app.device_manager.replace_devices(vec![device]);
    app.selected_device = Some(0);
    app.firmware = FirmwareProtocol::Vial;
    app.layout=Some(KeyboardLayout::from_vial_json(&serde_json::json!({"name":"SM62","matrix":{"rows":1,"cols":1},"layouts":{"keymap":[["0,0"]]}})).unwrap());
    let (hid, recorder) = HidDevice::test_device();
    app.shared_hid_output = hid.shared_output();
    app.hid_device = Some(hid.into());
    let directory = tempfile::tempdir().unwrap();
    (app, recorder, directory)
}
fn reply(q: &[u8; 32]) -> [u8; 32] {
    let mut r = [0; 32];
    r[..8].copy_from_slice(&[
        0xd7,
        0x62,
        3,
        q[3],
        q[5],
        q[6],
        if q[4] == 0 { 0x11 } else { 0x22 },
        q[3],
    ]);
    if q[3] == 2 {
        let mut w = [0u32; 48];
        w[0] = 10;
        w[1] = 10010;
        w[2] = 10000;
        w[3] = 3;
        w[5] = 3;
        w[7] = 8;
        w[11] = 100;
        w[13] = 99;
        w[14] = 9900;
        w[15] = 100;
        w[20] = 99;
        w[37] = 10011;
        w[38] = 10;
        w[39] = 9910;
        for i in 0..6 {
            r[8 + 4 * i..12 + 4 * i].copy_from_slice(&w[q[6] as usize * 6 + i].to_le_bytes());
        }
    }
    r
}
fn respond(recorder: &TestHidRecorder) {
    recorder.respond_by(|q| {
        if q[0] == 0xd7 {
            Some(Ok(reply(q)))
        } else if q[0] == 0xe6 {
            let mut r = [0; 32];
            r[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            Some(Ok(r))
        } else {
            None
        }
    });
}
fn settle(app: &mut EntropyApp, ctx: &egui::Context) {
    let due = Instant::now() + Duration::from_secs(3);
    while app.vial_hid_task_active() {
        assert!(Instant::now() < due, "worker never finished");
        app.poll_vial_hid_task(ctx);
        std::thread::yield_now();
    }
    // Sender publishes just before its RAII pause is dropped; await by next tick.
    std::thread::sleep(Duration::from_millis(1));
}
fn arm(app: &mut EntropyApp, ctx: &egui::Context) {
    app.sm62_fps.as_mut().unwrap().phase =
        Phase::Preparing(Instant::now() - Duration::from_secs(1));
    app.tick_sm62_fps(ctx);
    settle(app, ctx);
    app.tick_sm62_fps(ctx);
    settle(app, ctx);
    assert!(matches!(
        app.sm62_fps.as_ref().unwrap().phase,
        Phase::Measuring(_)
    ));
}
fn trace_words(seq: u8) -> [u32; 792] {
    let mut w = [0; 792];
    w[..24].copy_from_slice(&[0x19323644,0x19,792,seq as u32,100,200,2100,0,2,2,2,10,40,0,0,7,7,20,21,2100,0,0,0,0]);
    w[24..40].copy_from_slice(&[200,1000,0,239 | (27 << 16),10,400,5,1,
        1200,2100,28 << 16,239 | (55 << 16),10,500,5,2]);
    w
}
fn trace_reply(q: &[u8;32]) -> [u8;32] {
    let mut r = [0;32];
    r[..8].copy_from_slice(&[0xd7,0x62,0x19,q[3],q[5],q[6],0x12,q[3]]);
    if q[3] == 2 {
        let w = trace_words(q[5]);
        for i in 0..6 { r[8+i*4..12+i*4].copy_from_slice(&w[q[6] as usize*6+i].to_le_bytes()); }
    }
    r
}
#[test]
fn sm62_trace_about_button_runs_local_owner_and_all_132_pages_without_fps() {
    let (mut app, recorder, _dir) = setup();
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    app.app_settings.language = crate::i18n::Language::English;
    recorder.respond_by(|q| if q[0] == 0xd7 { Some(Ok(trace_reply(q))) } else if q[0] == 0xe6 {
        let mut r=[0;32]; r[..3].copy_from_slice(&[0xe6,8,0x5a]); Some(Ok(r))
    } else { None });
    let ctx = egui::Context::default();
    let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| app.draw_sm62_fps(ui)));
    fn locate(s: &egui::Shape) -> Option<egui::Pos2> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == "Frame trace 0x19 (SM62, USB half)" => Some(t.pos + t.galley.size()*0.5),
            egui::Shape::Vec(v) => v.iter().find_map(locate),
            _ => None,
        }
    }
    let pos = output.shapes.iter().find_map(|s| locate(&s.shape)).expect("missing executable 0x19 user path");
    for pressed in [true,false] {
        let input=egui::RawInput { events: vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::default()}], ..Default::default() };
        crate::app::discard_texture_deltas(ctx.run_ui(input, |ui| app.draw_sm62_fps(ui)));
    }
    assert!(app.sm62_fps.is_some());
    app.sm62_fps.as_mut().unwrap().phase=Phase::Preparing(Instant::now()-Duration::from_secs(1));
    app.tick_sm62_fps(&ctx); settle(&mut app,&ctx);
    assert!(matches!(app.sm62_fps.as_ref().unwrap().phase,Phase::Measuring(_)));
    assert_eq!(recorder.requests()[0][..8],[0xd7,0x62,0x19,1,0,app.sm62_fps.as_ref().unwrap().sequence,0,0]);
    let bridge=app.shared_hid_output.as_ref().unwrap().for_host_bridge_with_sm62(crate::qmk_hid_host::HostDataMode::default(),false,true);
    for _ in 0..3 {
        app.tick_sm62_fps(&ctx);
        assert!(bridge.poll_application_layout_event(&[0xe6,8,0xa5,0,0]).unwrap().is_some());
        assert!(bridge.write_application_layout_event_state(&[0xe8,8,0,0,0,0,0,0,0]).unwrap());
        bridge.write_output_report(&[0xe7,3,42]).unwrap();
    }
    assert_eq!(recorder.requests().iter().filter(|q|q[0]==0xd7).count(),1);
    app.sm62_fps.as_mut().unwrap().phase=Phase::Measuring(Instant::now()-Duration::from_secs(1));
    for _ in 0..132 { app.tick_sm62_fps(&ctx); settle(&mut app,&ctx); }
    let s=app.sm62_fps.as_ref().unwrap();
    assert_eq!(s.phase,Phase::Finished);
    assert!(s.message.starts_with("SUCCESS"),"{}",s.message);
    let log=std::fs::read_to_string(&s.path).unwrap();
    assert!(log.contains("part=1") && log.contains("gap_us=200") && log.contains("pixdata_us=500"),"{log}");
    assert!(!log.contains("completed frames/s"));
    assert!(log.contains("not FPS"));
    let diag: Vec<_>=recorder.requests().into_iter().filter(|q|q[0]==0xd7).collect();
    assert_eq!(diag.len(),133);
    for (i,q) in diag[1..].iter().enumerate() { assert_eq!(q[6] as usize,i); assert_eq!(q[4],0); }
}
#[test]
#[ignore = "requires generated production firmware fixture; qualification must run explicitly with --ignored"]
fn sm62_keymap_trace_real_owner_decodes_actual_c_firmware_pages() {
    let fixture=std::fs::read(std::path::PathBuf::from(std::env::var("AGENT_BUILD_CACHE").unwrap()).join("dualcore-trace-evidence/keymap-c-pairs.bin")).expect("run qualify_firmware.py first");
    let (mut app,recorder,dir)=setup();
    recorder.respond_by(move |q| {
        let i=if q[3]==1 {0}else{q[6] as usize+1};
        let chunk=&fixture[i*96..(i+1)*96];
        assert_eq!(&q[..],&chunk[..32]);
        Some(Ok(chunk[32..64].try_into().unwrap()))
    });
    app.begin_sm62_capture_to(dir.path().join("actual-c.log"),true);
    app.sm62_fps.as_mut().unwrap().sequence=23;
    app.sm62_fps.as_mut().unwrap().phase=Phase::Preparing(Instant::now()-Duration::from_secs(1));
    let ctx=egui::Context::default();
    app.tick_sm62_fps(&ctx);settle(&mut app,&ctx);
    app.sm62_fps.as_mut().unwrap().phase=Phase::Measuring(Instant::now()-Duration::from_secs(1));
    for _ in 0..132 {app.tick_sm62_fps(&ctx);settle(&mut app,&ctx);}
    let s=app.sm62_fps.as_ref().unwrap();
    assert_eq!(s.phase,Phase::Finished);assert!(s.message.starts_with("SUCCESS"),"{}",s.message);
    let log=std::fs::read_to_string(&s.path).unwrap();
    assert!(log.contains("pixel_bytes=134400")&&log.contains("pixdata_us=4010")&&log.contains("interpart_gap_us=1810")&&log.contains("gap_us=1"),"{log}");
    assert_eq!(recorder.requests().len(),133);
}
#[test]
fn sm62_keymap_trace_decoder_accepts_first_part_after_frame_begin_with_strict_bounds() {
    let mut w = trace_words(23);
    w[24] += 1; // Actual C records frame begin and first part in separate NOW() calls.
    let decode = |w: &[u32; 792]| keymap_trace::keymap_trace_text(w, 23);
    let text = decode(&w).expect("a moving timer may advance before first part start");
    assert!(text.contains("gap_us=1"), "{text}");
    assert!(text.contains("interpart_gap_us=201"), "{text}");
    for (i, value) in [(24,199),(24,1001),(25,200),(25,2101),(32,999),(33,2101),
        (5,2100),(19,2099),(6,10_000_101),(19,10_000_101)] {
        let mut bad = w; bad[i] = value;
        assert!(decode(&bad).is_err(), "accepted invalid timestamp word {i}={value}");
    }
    for i in [4,5,6,19,24,25,32,33] { w[i] = w[i].wrapping_add(u32::MAX - 100); }
    assert!(decode(&w).is_ok(), "moving timer with legitimate hardware wrap");
}
#[test]
fn sm62_keymap_trace_decoder_rejects_bad_identity_flags_modes_times_parts_and_bounds() {
    let w = trace_words(23);
    let decode = |w: &[u32; 792]| keymap_trace::keymap_trace_text(w, 23);
    let text = decode(&w).unwrap();
    assert!(text.contains("KEYMAP single completed frame; not FPS"));
    assert!(text.contains("pixel_bytes=26880"));
    // Each mutation violates a specific production-bound field, not host timing.
    for (i, value) in [(0,0),(1,3),(2,48),(3,24),(7,1),(7,2),(7,4),(7,8),(7,16),
        (8,3),(9,3),(10,0),(10,97),(16,8),(18,22),(20,1),(40,1),
        (5,1000),(6,200),(19,1000),(24,199),(25,2200),(26,240),(27,280<<16),
        (28,1000),(31,3),(32,900),(39,0)] {
        let mut bad = w; bad[i] = value;
        assert!(decode(&bad).is_err(), "accepted bad word {i}={value}");
    }
    let mut wrap = w;
    for i in [4,5,6,19,24,25,32,33] { wrap[i] = wrap[i].wrapping_add(u32::MAX - 100); }
    assert!(decode(&wrap).is_ok(), "legitimate hardware timer wrap");
}
#[test]
fn sm62_keymap_trace_rejection_and_stale_cancel_never_rearm_or_accept() {
    for status in [3,4,6,8,0xfe] {
        let (mut app, recorder, dir) = setup();
        recorder.respond_by(move |q| { let mut r=trace_reply(q); r[7]=status; r[8..].fill(0); Some(Ok(r)) });
        app.begin_sm62_capture_to(dir.path().join("trace.log"), true);
        let ctx=egui::Context::default();
        app.sm62_fps.as_mut().unwrap().phase=Phase::Preparing(Instant::now()-Duration::from_secs(1));
        app.tick_sm62_fps(&ctx); settle(&mut app,&ctx);
        assert_eq!(app.sm62_fps.as_ref().unwrap().phase,Phase::Finished);
        assert!(app.sm62_fps.as_ref().unwrap().message.starts_with("ERROR"));
        app.tick_sm62_fps(&ctx);
        assert_eq!(recorder.requests().len(),1);
    }
    let (mut app, recorder, dir) = setup();
    app.begin_sm62_capture_to(dir.path().join("stale.log"), true);
    let q = { let s=app.sm62_fps.as_mut().unwrap(); let mut q=request(1,0,s.sequence,0); q[2]=0x19; s.pending=Some(q); q };
    app.connection_generation += 1;
    app.finish_sm62_fps(q,Ok(trace_reply(&q)));
    assert_eq!(app.sm62_fps.as_ref().unwrap().phase,Phase::Finished);
    assert!(app.sm62_fps.as_ref().unwrap().message.contains("stale"));
    assert!(recorder.requests().is_empty());
}
#[test]
fn sm62_fps_owner_workers_allow_normal_traffic_during_whole_window_then_read_all_pages() {
    let (mut app, recorder, dir) = setup();
    respond(&recorder);
    let ctx = egui::Context::default();
    let bridge = app
        .shared_hid_output
        .as_ref()
        .unwrap()
        .for_host_bridge_with_sm62(crate::qmk_hid_host::HostDataMode::default(), false, true);
    let writes = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let capture = writes.clone();
    recorder.on_output(move |p| capture.lock().unwrap().push(p.to_vec()));
    app.begin_sm62_fps_to(dir.path().join("measurement.log"));
    assert!(app.sm62_fps.is_some());
    arm(&mut app, &ctx);
    assert!(app.hid_device.is_some());
    assert!(!app.vial_hid_task_active());
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|q| q[0] == 0xd7)
            .map(|q| q[4])
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    // Same production owner/lease APIs as event poll and feedback bridge. No FPS
    // poll or event-pause guard may be held over the idle measurement window.
    for _ in 0..10 {
        app.tick_sm62_fps(&ctx);
        assert!(bridge
            .poll_application_layout_event(&[0xe6, 8, 0xa5, 0, 0])
            .unwrap()
            .is_some());
        assert!(bridge
            .write_application_layout_event_state(&[0xe8, 8, 0, 0, 0, 0, 0, 0, 0])
            .unwrap());
        for packet in [&[0xe7, 1, 0][..], &[0xe7, 2, 1][..], &[0xe7, 3, 42][..]] {
            bridge.write_output_report(packet).unwrap();
        }
    }
    assert_eq!(
        recorder.requests().iter().filter(|q| q[0] == 0xd7).count(),
        2
    );
    assert_eq!(writes.lock().unwrap().len(), 40);
    app.sm62_fps.as_mut().unwrap().phase =
        Phase::Measuring(Instant::now() - Duration::from_secs(1));
    for _ in 0..16 {
        app.tick_sm62_fps(&ctx);
        settle(&mut app, &ctx);
    }
    let s = app.sm62_fps.as_ref().unwrap();
    assert_eq!(s.phase, Phase::Finished);
    assert!(s.message.starts_with("SUCCESS"), "{}", s.message);
    let log = std::fs::read_to_string(&s.path).unwrap();
    assert!(log.contains("Raw words:"));
    assert!(log.contains("PEER/no USB"));
    assert!(log.contains("MASTER/USB"));
    assert!(log.contains("10.000 completed frames/s"));
    assert_eq!(
        recorder
            .requests()
            .iter()
            .filter(|q| q[0] == 0xd7 && q[3] == 2)
            .count(),
        16
    );
}
#[test]
fn sm62_fps_selection_protocol_busy_unlock_and_missing_owner_are_fail_closed() {
    let (mut app, recorder, dir) = setup();
    let path = dir.path().join("refused.log");
    for case in 0..6 {
        let old = app.device_manager.devices()[0].clone();
        match case {
            0 => {
                let mut d = old.clone();
                d.product_id = 0x0004;
                app.device_manager.replace_devices(vec![d]);
            }
            1 => {
                let mut d = old.clone();
                d.bus_type = "Bluetooth".into();
                app.device_manager.replace_devices(vec![d]);
            }
            2 => app.unlock_open = true,
            3 => app.vial_unlock_polling = true,
            4 => app.pending_device_connect = Some(old.stable_identity()),
            _ => {
                app.hid_device = None;
            }
        }
        app.begin_sm62_fps_to(path.clone());
        assert!(app.sm62_fps.is_none(), "case {case} admitted");
        assert!(recorder.requests().is_empty());
        app.device_manager.replace_devices(vec![old]);
        app.firmware = FirmwareProtocol::Vial;
        app.unlock_open = false;
        app.vial_unlock_polling = false;
        app.pending_device_connect = None;
    }
    assert!(!path.exists());
}
#[test]
fn sm62_fps_cancel_disconnect_switch_generation_and_owner_replacement_never_read_snapshots() {
    for case in 0..5 {
        let (mut app, recorder, dir) = setup();
        respond(&recorder);
        let ctx = egui::Context::default();
        app.begin_sm62_fps_to(dir.path().join("cancel.log"));
        arm(&mut app, &ctx);
        match case {
            0 => app.cancel_sm62_fps("CANCELLED by user"),
            1 => {
                app.hid_device = None;
                app.shared_hid_output = None;
                app.connection_generation += 1;
            }
            2 => app.selected_device = None,
            3 => app.connection_generation += 1,
            _ => {
                let (other, _) = HidDevice::test_device();
                app.shared_hid_output = other.shared_output();
                app.hid_device = Some(other.into());
            }
        }
        app.tick_sm62_fps(&ctx);
        let s = app.sm62_fps.as_ref().unwrap();
        assert_eq!(s.phase, Phase::Finished);
        assert!(s.cancelled.load(Ordering::Acquire));
        assert_eq!(recorder.requests().len(), 2);
        let log = std::fs::read_to_string(&s.path).unwrap();
        assert!(log.contains("CANCELLED"));
        assert!(!log.contains("SUCCESS"));
    }
}
#[test]
fn sm62_fps_late_arm_completion_cannot_publish_after_switch_or_cancel_and_ui_does_not_block() {
    for cancel in [false, true] {
        let (mut app, recorder, dir) = setup();
        let ctx = egui::Context::default();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        recorder.respond_by(move |q| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Some(Ok(reply(q)))
        });
        app.begin_sm62_fps_to(dir.path().join("late.log"));
        app.sm62_fps.as_mut().unwrap().phase = Phase::ArmPeer;
        let start = Instant::now();
        app.tick_sm62_fps(&ctx);
        assert!(start.elapsed() < Duration::from_secs(1));
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        if cancel {
            app.cancel_sm62_fps("CANCELLED by user");
        } else {
            app.connection_generation += 1;
            app.selected_device = None;
            app.tick_sm62_fps(&ctx);
        }
        let _ = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| app.draw_sm62_fps(ui)));
        assert!(start.elapsed() < Duration::from_secs(2));
        release_tx.send(()).unwrap();
        settle(&mut app, &ctx);
        app.tick_sm62_fps(&ctx);
        assert_eq!(app.sm62_fps.as_ref().unwrap().phase, Phase::Finished);
        assert_eq!(recorder.requests().len(), 1);
        assert!(!std::fs::read_to_string(dir.path().join("late.log"))
            .unwrap()
            .contains("SUCCESS"));
    }
}
#[test]
fn sm62_fps_rejects_malformed_or_failed_worker_reply_without_second_arm() {
    for case in 0..8 {
        let (mut app, recorder, dir) = setup();
        let ctx = egui::Context::default();
        recorder.respond_by(move |q| {
            if case == 7 {
                return Some(Err("diagnostic RPC failed".into()));
            }
            let mut r = reply(q);
            r[case] ^= 0x70;
            Some(Ok(r))
        });
        app.begin_sm62_fps_to(dir.path().join("invalid.log"));
        app.sm62_fps.as_mut().unwrap().phase = Phase::ArmPeer;
        app.tick_sm62_fps(&ctx);
        settle(&mut app, &ctx);
        app.tick_sm62_fps(&ctx);
        let s = app.sm62_fps.as_ref().unwrap();
        assert_eq!(s.phase, Phase::Finished);
        assert!(s.message.contains("ERROR"));
        assert_eq!(recorder.requests().len(), 1);
        let log = std::fs::read_to_string(&s.path).unwrap();
        assert!(log.contains("ERROR"));
        assert!(!log.contains("SUCCESS"));
    }
}
#[test]
fn sm62_fps_dualcore_unsupported_finishes_localized_without_retry_or_owner_loss() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for (language, expected) in [
        (crate::i18n::Language::Russian, "Текущая прошивка не поддерживает этот замер FPS"),
        (crate::i18n::Language::English, "Current firmware does not support this FPS measurement"),
    ] {
        let (mut app, recorder, dir) = setup();
        app.app_settings.language = language;
        respond(&recorder);
        recorder.respond_by(|q| {
            let mut r = [0; 32];
            r[..8].copy_from_slice(&[0xd7, 0x62, 0x18, q[3], q[5], q[6], 0x12, 0xfe]);
            Some(Ok(r))
        });
        let ctx = egui::Context::default();
        let owner = app.shared_hid_output.clone().unwrap();
        app.begin_sm62_fps_to(dir.path().join("unsupported.log"));
        app.sm62_fps.as_mut().unwrap().phase = Phase::ArmPeer;
        app.tick_sm62_fps(&ctx);
        // Run actual poll→diagnostic→runtime background ordering; the existing
        // wrapper keeps OS discovery not due for this synthetic endpoint.
        let due = Instant::now() + Duration::from_secs(3);
        while app.vial_hid_task_active() {
            assert!(Instant::now() < due);
            app.tick_foreground_runtime_for_test(&ctx, 100.0, false);
            std::thread::yield_now();
        }
        app.tick_foreground_runtime_for_test(&ctx, 100.0, false);
        let s = app.sm62_fps.as_ref().unwrap();
        assert_eq!(s.phase, Phase::Finished);
        assert!(s.message.contains(expected), "{}", s.message);
        assert!(app.status_msg.contains(expected), "{}", app.status_msg);
        assert!(s.pending.is_none());
        assert_eq!(s.sides, [0; 2]);
        assert_eq!(s.words, [[0; 48]; 2]);
        let log = std::fs::read_to_string(&s.path).unwrap();
        assert!(log.contains(expected));
        assert!(log.contains("r=[D7, 62, 18"));
        assert!(!log.contains("completed frames/s"));
        assert!(!log.contains("SUCCESS"));
        assert!(owner.is_available());
        assert!(app.hid_device.as_ref().unwrap().shared_output().unwrap().shares_owner_with(&owner));
        for _ in 0..5 {
            app.poll_vial_hid_task(&ctx);
            app.tick_sm62_fps(&ctx);
            let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
                app.draw_about_device_page(ui, ui.max_rect());
            }));
            fn painted_text(shape: &egui::Shape, text: &mut String) {
                match shape {
                    egui::Shape::Text(t) => text.push_str(t.galley.text()),
                    egui::Shape::Vec(shapes) => for s in shapes { painted_text(s, text); },
                    _ => {}
                }
            }
            let mut text = String::new();
            for s in &output.shapes { painted_text(&s.shape, &mut text); }
            // The accepted SINGLE UI deliberately has no diagnostic FPS panel
            // (also true before the upstream port). Do not invoke a test-only
            // renderer or add experimental controls merely to satisfy this test.
            // Terminal localization is asserted on production status/log above.
            assert!(!text.is_empty(), "About page failed to paint");
            assert!(!text.contains("Measure ORBIT FPS") && !text.contains("Замер FPS ORBIT"));
            assert!(app.status_msg.contains(expected), "{}", app.status_msg);
        }
        assert_eq!(recorder.requests().len(), 1);
        respond(&recorder);
        let bridge = owner.for_host_bridge_with_sm62(crate::qmk_hid_host::HostDataMode::default(), false, true);
        assert!(bridge.poll_application_layout_event(&[0xe6, 8, 0xa5, 0, 0]).unwrap().is_some());
    }
}
#[test]
fn sm62_fps_wrong_nonce_garbage_and_stale_unsupported_never_publish_support_result() {
    for case in 0..5 {
        let (mut app, recorder, dir) = setup();
        let ctx = egui::Context::default();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        recorder.respond_by(move |q| {
            if case >= 3 {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            let mut r = [0; 32];
            r[..8].copy_from_slice(&[0xd7, 0x62, 0x18, q[3], q[5], q[6], 0x12, 0xfe]);
            match case {
                0 => r[4] ^= 1,
                1 => r[8] = 1,
                2 => { r[3..7].fill(0); }, // old uncorrelated RPC guard
                _ => {}
            }
            Some(Ok(r))
        });
        app.begin_sm62_fps_to(dir.path().join("negative.log"));
        app.sm62_fps.as_mut().unwrap().phase = Phase::ArmPeer;
        app.tick_sm62_fps(&ctx);
        if case >= 3 { entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(); }
        if case == 3 { app.connection_generation += 1; }
        if case == 4 { app.cancel_sm62_fps("CANCELLED by user"); }
        if case >= 3 { release_tx.send(()).unwrap(); }
        settle(&mut app, &ctx);
        app.tick_sm62_fps(&ctx);
        let s = app.sm62_fps.as_ref().unwrap();
        assert_eq!(s.phase, Phase::Finished);
        assert!(!s.message.contains("does not support"));
        assert!(!s.message.contains("не поддерживает"));
        let log = std::fs::read_to_string(&s.path).unwrap();
        assert!(!log.contains("SUCCESS"));
        assert!(!log.contains("completed frames/s"));
        assert_eq!(recorder.requests().len(), 1);
    }
}
#[test]
fn sm62_fps_cancellation_before_preparation_has_zero_hid_writes() {
    let (mut app, recorder, dir) = setup();
    let ctx = egui::Context::default();
    app.begin_sm62_fps_to(dir.path().join("cancel-before-arm.log"));
    app.cancel_sm62_fps("CANCELLED by user");
    app.tick_sm62_fps(&ctx);
    assert!(recorder.requests().is_empty());
    assert!(!app.vial_hid_task_active());
}
#[test]
fn sm62_fps_window_deadline_does_not_poll_early_and_overall_wait_is_bounded() {
    let (mut app, recorder, dir) = setup();
    respond(&recorder);
    let ctx = egui::Context::default();
    app.begin_sm62_fps_to(dir.path().join("deadline.log"));
    app.tick_sm62_fps(&ctx);
    assert!(recorder.requests().is_empty());
    arm(&mut app, &ctx);
    let original = app.sm62_fps.as_ref().unwrap().phase;
    for _ in 0..20 {
        app.tick_sm62_fps(&ctx);
    }
    assert_eq!(app.sm62_fps.as_ref().unwrap().phase, original);
    assert_eq!(recorder.requests().len(), 2);
    app.sm62_fps.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
    app.tick_sm62_fps(&ctx);
    assert_eq!(app.sm62_fps.as_ref().unwrap().phase, Phase::Finished);
    assert!(app.sm62_fps.as_ref().unwrap().message.contains("120s"));
}

//! Real worker/UI regressions; HID transport is simulated, not hardware acceptance.
use super::*;
use std::time::{Duration, Instant};

fn frame(
    app: &mut EntropyApp,
    ctx: &egui::Context,
    pointer: Option<egui::Pos2>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
            ..Default::default()
        },
        |ui| app.draw_live_features_settings_page(ui, ui.max_rect()),
    )
}
fn selector(output: &egui::FullOutput) -> (egui::Rect, egui::Color32) {
    let pos = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "ANSI" => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("selector text");
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.rect.contains(pos) && r.rect.height() < 40.0 && r.rect.width() > 100.0 =>
            {
                Some((r.rect, r.fill))
            }
            _ => None,
        })
        .expect("selector rectangle")
}

fn fixture() -> (EntropyApp, crate::hid::TestHidRecorder) {
    let mut app = EntropyApp::new_inert_for_test();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid);
    app.device_manager
        .replace_devices(vec![crate::device::Device {
            name: "Test".into(),
            vendor_id: 0x1209,
            product_id: 0x2327,
            manufacturer: "Entropy".into(),
            serial_number: "test".into(),
            bus_type: "Bluetooth".into(),
            path: "geometry-diagnostic".into(),
            instance_token: String::new(),
            firmware: FirmwareProtocol::Vial,
        }]);
    app.selected_device = Some(0);
    app.layout = Some(KeyboardLayout {
        name: "Geometry diagnostic".into(),
        rows: 0,
        cols: 0,
        keys: vec![],
        encoders: vec![],
        layers: vec![],
        encoder_layers: vec![],
        layer_names: vec![],
        custom_keycodes: vec![],
        layout_options: vec![LayoutOption {
            label: "Universal Symbols geometry".into(),
            choices: vec!["ANSI".into(), "ISO".into()],
        }],
        live_features: Default::default(),
        supports_rgb: false,
        lighting_mode: None,
        firmware: FirmwareProtocol::Vial,
    });
    app.layout_options_value = Some(0);
    (app, recorder)
}

#[test]
fn universal_geometry_hover_background_hid_borrow_keeps_selector_stable() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    let generation = app.connection_generation;
    let initial = selector(&frame(&mut app, &ctx, None));
    let pointer = initial.0.center();
    let _ = frame(&mut app, &ctx, Some(pointer));
    let before_frame = frame(&mut app, &ctx, Some(pointer));
    let before = selector(&before_frame);
    assert_eq!(before.0, initial.0);
    assert_eq!(
        before_frame.platform_output.cursor_icon,
        egui::CursorIcon::PointingHand
    );
    let (release, gate) = std::sync::mpsc::channel();
    assert_eq!(
        app.start_vial_hid_operation_with_runner(
            &ctx,
            VialHidOperation::BatteryRefresh,
            move |_, _, _, _| {
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(VialHidOutcome::Battery(None))
            }
        ),
        VialHidTaskStart::Started
    );
    assert!(app.hid_device.is_none());
    assert!(app.hid_write_task_owner_active());
    let during_frame = frame(&mut app, &ctx, Some(pointer));
    let during = selector(&during_frame);
    let during_cursor = during_frame.platform_output.cursor_icon;
    app.maybe_begin_bluetooth_reconnect();
    assert!(!app.bluetooth_reconnect_active());
    assert_eq!(app.connection_generation, generation);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.vial_hid_task_active() {
        assert!(Instant::now() < deadline);
        app.poll_vial_hid_task(&ctx);
        std::thread::yield_now();
    }
    let _ = frame(&mut app, &ctx, Some(pointer));
    let after = selector(&frame(&mut app, &ctx, Some(pointer)));
    assert_eq!(before.0, during.0);
    assert_eq!(before, after);
    assert_eq!(
        before.1, during.1,
        "healthy HID ownership must not disable selector"
    );
    assert_eq!(during_cursor, egui::CursorIcon::PointingHand);
    assert_eq!(app.connection_generation, generation);
    assert_eq!(app.layout_options_value, Some(0));
    assert!(
        recorder.requests().is_empty(),
        "hover must not issue HID GET/SET"
    );
    println!("STABLE: same selector rect, fill {:?} -> {:?} -> {:?}; no HID requests, no reconnect, generation={generation}", before.1, during.1, after.1);
}

fn drain(app: &mut EntropyApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    // Poll even when idle: queued writes must be admitted on the next frame.
    app.poll_vial_hid_task(ctx);
    while app.vial_hid_task_active() {
        assert!(Instant::now() < deadline, "worker did not finish");
        app.poll_vial_hid_task(ctx);
        std::thread::yield_now();
    }
}

fn battery_gate(app: &mut EntropyApp, ctx: &egui::Context) -> std::sync::mpsc::Sender<()> {
    let (release, gate) = std::sync::mpsc::channel();
    assert_eq!(
        app.start_vial_hid_operation_with_runner(
            ctx,
            VialHidOperation::BatteryRefresh,
            move |_, _, _, _| {
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(VialHidOutcome::Battery(None))
            }
        ),
        VialHidTaskStart::Started
    );
    release
}

fn echo_layout(recorder: &crate::hid::TestHidRecorder) {
    let mut packed = [0u8; 4];
    recorder.respond_by(move |request| {
        if request[1] != 2 {
            return None;
        }
        let mut response = *request;
        match request[0] {
            3 => packed.copy_from_slice(&request[2..6]),
            2 => response[2..6].copy_from_slice(&packed),
            _ => return None,
        }
        Some(Ok(response))
    });
}

#[test]
fn universal_geometry_choice_while_borrowed_serializes_set_get() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    echo_layout(&recorder);
    let release = battery_gate(&mut app, &ctx);
    app.set_layout_option_value(0, 1);
    assert_eq!(app.layout_options_value, Some(1));
    assert!(
        recorder.requests().is_empty(),
        "cannot write through a borrowed handle"
    );
    release.send(()).unwrap();
    drain(&mut app, &ctx);
    let traffic: Vec<_> = recorder
        .requests()
        .iter()
        .map(|r| r[..6].to_vec())
        .collect();
    assert_eq!(
        traffic,
        vec![vec![3, 2, 0, 0, 0, 1], vec![2, 2, 0, 0, 0, 0]],
        "choice made during worker ownership must reach SET then GET"
    );
    assert_eq!(app.layout_options_value, Some(1));
}

#[test]
fn universal_geometry_rapid_choices_coalesce_and_inflight_completion_keeps_latest() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    let release = battery_gate(&mut app, &ctx);
    app.set_layout_option_value(0, 1);
    app.set_layout_option_value(0, 0);
    app.set_layout_option_value(0, 1);
    let (entered, started) = std::sync::mpsc::channel();
    let (finish, gate) = std::sync::mpsc::channel();
    let mut packed = [0u8; 4];
    let mut first = true;
    recorder.respond_by(move |request| {
        if request[1] != 2 {
            return None;
        }
        let mut response = *request;
        if request[0] == 3 {
            packed.copy_from_slice(&request[2..6]);
            if first {
                first = false;
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        } else if request[0] == 2 {
            response[2..6].copy_from_slice(&packed);
        } else {
            return None;
        }
        Some(Ok(response))
    });
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_vial_hid_task(&ctx);
        if started.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    app.set_layout_option_value(0, 0);
    assert_eq!(app.layout_options_value, Some(0));
    // Inspect old completion without queue admission, so a bad old result
    // cannot hide behind the next write.
    finish.send(()).unwrap();
    while app.vial_hid_task_active() {
        app.poll_vial_hid_task_with_settings_save(&ctx, |_| {});
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(
        app.layout_options_value,
        Some(0),
        "old ISO completion replaced newer ANSI"
    );
    drain(&mut app, &ctx);
    let traffic: Vec<_> = recorder
        .requests()
        .iter()
        .map(|r| r[..6].to_vec())
        .collect();
    assert_eq!(
        traffic,
        vec![
            vec![3, 2, 0, 0, 0, 1],
            vec![2, 2, 0, 0, 0, 0],
            vec![3, 2, 0, 0, 0, 0],
            vec![2, 2, 0, 0, 0, 0]
        ]
    );
    assert!(app.pending_universal_geometry.is_none());
}

#[test]
fn universal_geometry_stale_queue_never_reaches_new_session_or_device() {
    for changed in ["generation", "path", "instance"] {
        let ctx = egui::Context::default();
        let (mut app, recorder) = fixture();
        let release = battery_gate(&mut app, &ctx);
        app.set_layout_option_value(0, 1);
        let mut device = app.device_manager.devices()[0].clone();
        match changed {
            "generation" => app.connection_generation += 1,
            "path" => device.path = "other-device".into(),
            _ => device.instance_token = "new-instance".into(),
        }
        app.device_manager.replace_devices(vec![device]);
        assert!(
            !app.universal_geometry_connection_ready(),
            "old owner lent readiness to {changed}"
        );
        release.send(()).unwrap();
        drain(&mut app, &ctx);
        assert!(
            recorder.requests().is_empty(),
            "stale choice wrote to {changed}"
        );
        assert!(app.pending_universal_geometry.is_none());
        assert!(
            app.hid_device.is_none(),
            "old owner's handle returned to {changed}"
        );
        assert!(!app.universal_geometry_connection_ready());
    }
}

#[test]
fn universal_geometry_errors_are_visible_without_blind_retry() {
    for failure in ["set failed", "get failed", "mismatch"] {
        let ctx = egui::Context::default();
        let (mut app, recorder) = fixture();
        recorder.respond_by(move |request| {
            if request[1] != 2 {
                return None;
            }
            if request[0] == 3 && failure == "set failed" {
                return Some(Err("test set failed".into()));
            }
            if request[0] == 2 && failure == "get failed" {
                return Some(Err("test get failed".into()));
            }
            let mut response = *request;
            if request[0] == 2 {
                response[2..6].fill(0);
            }
            Some(Ok(response))
        });
        app.set_layout_option_value(0, 1);
        drain(&mut app, &ctx);
        assert!(
            app.status_msg.contains(failure),
            "{}: {}",
            failure,
            app.status_msg
        );
        assert!(app.pending_universal_geometry.is_none());
        let count = recorder.requests().len();
        assert_eq!(count, if failure == "set failed" { 1 } else { 2 });
        for _ in 0..5 {
            app.poll_vial_hid_task(&ctx);
            let _ = frame(&mut app, &ctx, None);
        }
        assert_eq!(
            recorder.requests().len(),
            count,
            "must not blindly retry {failure}"
        );
    }
}

#[test]
fn universal_geometry_true_disconnect_disables_selector_and_drops_pending() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    let initial = selector(&frame(&mut app, &ctx, None));
    app.set_layout_option_value(0, 1);
    app.hid_device = None;
    app.layout_options_value = Some(0);
    let during = frame(&mut app, &ctx, Some(initial.0.center()));
    assert_ne!(
        during.platform_output.cursor_icon,
        egui::CursorIcon::PointingHand
    );
    assert!(!app.universal_geometry_connection_ready());
    drain(&mut app, &ctx);
    assert!(app.pending_universal_geometry.is_none());
    assert!(recorder.requests().is_empty());
}

#[test]
fn universal_geometry_pending_choice_participates_in_exit_lifecycle_without_self_blocking() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    echo_layout(&recorder);
    app.set_layout_option_value(0, 1);
    assert!(
        app.hid_write_lifecycle_busy(),
        "exit may drop a queued geometry write"
    );
    drain(&mut app, &ctx);
    assert!(!app.hid_write_lifecycle_busy());
    assert_eq!(
        recorder.requests().len(),
        2,
        "pending queue must not self-block admission"
    );
}

#[test]
fn universal_geometry_old_inflight_result_cannot_replace_new_handle_or_status() {
    for changed in ["generation", "path", "instance"] {
        let ctx = egui::Context::default();
        let (mut app, old_recorder) = fixture();
        app.set_layout_option_value(0, 1);
        let (entered, started) = std::sync::mpsc::channel();
        let (finish, gate) = std::sync::mpsc::channel();
        old_recorder.respond_by(move |request| {
            if request[0] == 3 && request[1] == 2 {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                return Some(Err("old target write failed".into()));
            }
            None
        });
        app.poll_vial_hid_task(&ctx);
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        // A newer queued choice also belongs to the old session.
        app.set_layout_option_value(0, 0);
        let mut device = app.device_manager.devices()[0].clone();
        match changed {
            "generation" => app.connection_generation += 1,
            "path" => device.path = "new-path".into(),
            _ => device.instance_token = "new-instance".into(),
        }
        app.device_manager.replace_devices(vec![device]);
        let (new_hid, new_recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(new_hid);
        app.layout_options_value = Some(0);
        app.status_msg = "new target".into();
        finish.send(()).unwrap();
        drain(&mut app, &ctx);
        assert_eq!(
            app.status_msg, "new target",
            "stale error changed new target status"
        );
        assert!(app.pending_universal_geometry.is_none());
        assert_eq!(old_recorder.requests().len(), 1);
        assert!(new_recorder.requests().is_empty());
        // Prove the handle itself was not replaced by the stale returned one.
        app.hid_device
            .as_ref()
            .unwrap()
            .get_layout_options()
            .unwrap();
        assert_eq!(new_recorder.requests().len(), 1);
        assert_eq!(old_recorder.requests().len(), 1);
    }
}

#[test]
fn universal_geometry_async_set_get_preserves_encoder_bits_with_sync_off_or_on() {
    for sync in [false, true] {
        for bits in 0..4 {
            let ctx = egui::Context::default();
            let (mut app, recorder) = fixture();
            app.app_settings.layout_sync_enabled = sync;
            let options = &mut app.layout.as_mut().unwrap().layout_options;
            for label in ["Hide left encoder", "Hide right encoder"] {
                options.push(LayoutOption {
                    label: label.into(),
                    choices: vec![],
                });
            }
            echo_layout(&recorder);
            app.layout_options_value = Some(bits);
            for (choice, packed) in [(1, bits | 4), (0, bits)] {
                app.set_layout_option_value(0, choice);
                drain(&mut app, &ctx);
                assert_eq!(app.layout_options_value, Some(packed));
            }
            let requests = recorder.requests();
            assert_eq!(requests.len(), 4);
            assert_eq!(&requests[0][..6], &[3, 2, 0, 0, 0, (bits | 4) as u8]);
            assert_eq!(&requests[2][..6], &[3, 2, 0, 0, 0, bits as u8]);
            assert_eq!(&requests[1][..2], &[2, 2]);
            assert_eq!(&requests[3][..2], &[2, 2]);
            assert_eq!(app.app_settings.layout_sync_enabled, sync);
        }
    }
}

fn input_frame(
    app: &mut EntropyApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.draw_live_features_settings_page(ui, ui.max_rect()),
    )
}

fn click(app: &mut EntropyApp, ctx: &egui::Context, pos: egui::Pos2) -> egui::FullOutput {
    input_frame(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    input_frame(
        app,
        ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    )
}

fn text_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, label))
        .expect("popup choice text")
}

#[test]
fn universal_geometry_actual_popup_choice_during_worker_is_saved() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    echo_layout(&recorder);
    let rect = selector(&frame(&mut app, &ctx, None)).0;
    let release = battery_gate(&mut app, &ctx);
    click(&mut app, &ctx, rect.center());
    let popup = frame(&mut app, &ctx, Some(rect.center()));
    let pos = text_center(&popup, "ISO");
    click(&mut app, &ctx, pos);
    assert_eq!(
        app.layout_options_value,
        Some(1),
        "enabled popup must accept choice while HID is borrowed"
    );
    assert!(recorder.requests().is_empty());
    release.send(()).unwrap();
    drain(&mut app, &ctx);
    assert_eq!(recorder.requests().len(), 2);
    assert_eq!(&recorder.requests()[0][..6], &[3, 2, 0, 0, 0, 1]);
}

#[test]
fn universal_geometry_queued_write_keeps_later_unrelated_option_edit() {
    let ctx = egui::Context::default();
    let (mut app, recorder) = fixture();
    app.layout
        .as_mut()
        .unwrap()
        .layout_options
        .push(LayoutOption {
            label: "Hide right encoder".into(),
            choices: vec![],
        });
    echo_layout(&recorder);
    let release = battery_gate(&mut app, &ctx);
    app.set_layout_option_value(0, 1); // ISO=bit 1
    app.set_layout_option_value(1, 1); // encoder=bit 0
    assert_eq!(app.layout_options_value, Some(3));
    release.send(()).unwrap();
    drain(&mut app, &ctx);
    assert_eq!(
        &recorder.requests()[0][..6],
        &[3, 2, 0, 0, 0, 3],
        "queued geometry snapshot clobbered a later unrelated packed bit"
    );
    assert_eq!(recorder.requests().len(), 2);
}

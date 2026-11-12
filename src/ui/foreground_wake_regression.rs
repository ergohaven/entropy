#[test]
fn foreground_event_wakes_settled_native_runtime_without_heartbeat_wait() {
    use crate::app_discovery::{ApplicationDiscoverySnapshot, ForegroundState, ForegroundStatus};
    use crate::application_layouts::DetectedApplication;
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let mut app = EntropyApp::new_inert_for_test();
    let mut device = m4cr0pad_v3_device();
    device.name = "SM62".into();
    device.product_id = 0x00b2;
    device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    app.device_manager.replace_devices(vec![device.clone()]);
    app.selected_device = Some(0);
    app.current_keyboard_id = Some(0x37434f4c32364d53);
    app.current_device_name = "SM62".into();
    app.layout = Some(sm62_test_layout());
    mark_application_layout_protocol_supported(&mut app);
    let (hid, recorder, _) = runtime_fixture_hid(Vec::new());
    let bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(
        device.clone(),
        Default::default(),
        hid.shared_output(),
        crate::qmk_hid_host::HostProtocol::Selected(true),
    );
    bridge.set_selected_application_layout_support(true);
    app.qmk_hid_hosts.insert(device.path.clone(), bridge);
    runtime_fixture_connect(&mut app, &device, hid);
    let a = DetectedApplication {
        executable: "wake-app-a".into(),
        display_name: "A".into(),
        ..Default::default()
    };
    let b = DetectedApplication {
        executable: "wake-app-b".into(),
        display_name: "B".into(),
        ..Default::default()
    };
    let settings = app.application_layout_settings_mut().unwrap();
    let aid = settings.create_for_application(&a);
    let bid = settings.create_for_application(&b);
    settings.layouts.get_mut(&aid).unwrap().layers = vec![vec![0x22; 62]; 16];
    settings.layouts.get_mut(&bid).unwrap().layers = vec![vec![0x23; 62]; 16];
    assert!(app.activate_application_layout(&bid));
    assert!(app.activate_application_layout(&aid));
    let snapshot = |application: DetectedApplication| ApplicationDiscoverySnapshot {
        foreground: Some(application.clone()),
        foreground_status: ForegroundStatus {
            backend: "synthetic OS event producer".into(),
            state: ForegroundState::Focused(application),
        },
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let frame = |app: &mut EntropyApp, s: ApplicationDiscoverySnapshot, time: f64| {
        crate::app_discovery::with_test_application_discovery(s, || {
            crate::app::discard_texture_deltas(ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    app.tick_foreground_runtime_for_test(ui.ctx(), time, true);
                },
            ));
        });
    };
    // Settle actual egui and ordinary hidden-window background ordering.
    for pass in 1..=4 {
        frame(&mut app, snapshot(a.clone()), pass as f64 / 10.0);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    ctx.set_request_repaint_callback(move |info| {
        // Real wake callbacks can re-enter discovery and subscription cleanup.
        // Both publication and listener locks must already be released.
        let _snapshot = crate::app_discovery::test_foreground_status_snapshot();
        let temporary =
            crate::app_discovery::subscribe_foreground_repaint(&egui::Context::default());
        drop(temporary);
        let _ = tx.send(info.delay);
    });
    crate::app_discovery::test_publish_foreground_status(snapshot(b.clone()).foreground_status);
    assert_eq!(
        rx.recv_timeout(std::time::Duration::from_millis(50))
            .expect(
            "foreground publication did not wake native UI; it still waits for the 200ms heartbeat"
        ),
        std::time::Duration::ZERO
    );
    frame(&mut app, snapshot(b), 0.5);
    assert_eq!(
        app.qmk_hid_hosts[&device.path]
            .test_application_layout_snapshot()
            .profile_id
            .as_ref(),
        Some(&bid)
    );
    app.qmk_hid_hosts.clear();
    app.selected_device = None;
    app.app_settings.application_layouts.clear();
    frame(&mut app, ApplicationDiscoverySnapshot::default(), 0.6);
    assert!(
        app.foreground_repaint_subscription.is_none(),
        "inactive runtime retained its foreground wake subscription"
    );
}

fn picker_intent_app() -> EntropyApp {
    let mut app = EntropyApp::new_inert_for_test();
    let device = crate::device::Device {
        name: "SM62".into(), vendor_id: 0xE126, product_id: 0x00b2,
        manufacturer: "Ergohaven".into(), serial_number: "picker-intent".into(),
        bus_type: "Usb".into(), path: format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")),
        instance_token: "picker-intent".into(), firmware: FirmwareProtocol::Vial,
    };
    app.device_about_info = Some(crate::app::DeviceAboutInfo {
        supports_application_layouts: true, vendor_id: device.vendor_id,
        product_id: device.product_id, path: device.path.clone(), ..Default::default()
    });
    app.device_manager.replace_devices(vec![device]);
    app.selected_device = Some(0);
    app.current_keyboard_id = Some(42);
    app.current_device_name = "SM62".into();
    let keys: Vec<String> = crate::application_layouts::SM62_APPLICATION_POSITIONS.iter()
        .map(|(r,c)| format!("{r},{c}")).collect();
    let mut layout = KeyboardLayout::from_vial_json(&serde_json::json!({
        "name":"SM62", "matrix":{"rows":10,"cols":7}, "layouts":{"keymap":[keys]}
    })).unwrap();
    layout.layers = vec![vec![42.into();62];16];
    app.layout = Some(layout);
    app.layer_count = 16;
    app.deferred_device_load = DeferredDeviceLoadState::complete(16);
    app.ensure_application_layout_settings();
    app
}

fn picker_intent_attach(app: &mut EntropyApp) -> String {
    let key = app.application_layout_device_key().unwrap();
    let device = app.device_manager.devices()[0].clone();
    let mut bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(
        device.clone(), Default::default(), None, crate::qmk_hid_host::HostProtocol::Selected(false));
    bridge.bind_selected_application_layout_device(key);
    bridge.set_selected_application_layout_support(true);
    app.qmk_hid_hosts.insert(device.path.clone(), bridge);
    device.path
}

#[test]
fn explicit_picker_first_owner_add_publishes_without_accessor_prewarm() {
    picker_first_owner_action(false);
}

#[test]
fn explicit_picker_first_owner_edit_publishes_without_accessor_prewarm() {
    picker_first_owner_action(true);
}

fn picker_first_owner_action(edit: bool) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    {
        let mut app = picker_intent_app();
        if edit {
            app.open_application_picker(false);
            app.select_application_picker_choice(detected_application("old-picker-app", "Old picker app"));
            assert!(app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        }
        // The new owner has not had a runtime tick or mutable accessor.
        let path = picker_intent_attach(&mut app);
        app.open_application_picker(edit);
        app.select_application_picker_choice(detected_application("explicit-picker-app", "Explicit picker app"));
        assert!(app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
        let id = app.application_layout_settings().unwrap().editor_layout_id.clone();
        for _ in 0..2 { app.update_application_layout_runtime(); }
        let library = app.qmk_hid_hosts[&path].test_published_library();
        let profile = library.iter().find(|p| p.profile_id.as_ref() == Some(&id))
            .expect("first-owner picker intent was swallowed as a baseline");
        assert_eq!(profile.name, "Explicit picker app");
        assert_eq!(library.len(), 1, "unrelated saved profiles were authorized");
    }
}

#[test]
fn explicit_picker_pending_action_does_not_authorize_replacement_owner() {
    picker_pending_action(false);
}

#[test]
fn explicit_picker_pending_action_does_not_authorize_new_generation() {
    picker_pending_action(true);
}

fn picker_pending_action(generation_change: bool) {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let mut app = picker_intent_app();
    let path = picker_intent_attach(&mut app);
    app.open_application_picker(false);
    app.select_application_picker_choice(detected_application("pending-picker-app", "Pending picker app"));
    assert!(app.apply_picker_selection(app.application_picker_selected.clone().unwrap()));
    let id = app.application_layout_settings().unwrap().editor_layout_id.clone();
    if generation_change {
        let key = app.application_layout_device_key().unwrap();
        let bridge = app.qmk_hid_hosts.get_mut(&path).unwrap();
        let previous = bridge.application_layout_publication_generation().unwrap();
        bridge.clear_application_layout_binding();
        bridge.bind_selected_application_layout_device(key);
        assert_ne!(bridge.application_layout_publication_generation(), Some(previous));
    } else {
        app.qmk_hid_hosts.clear();
        picker_intent_attach(&mut app);
    }
    app.update_application_layout_runtime();
    assert!(app.qmk_hid_hosts[&path].test_published_library().is_empty());
    assert!(app.application_layout_settings().unwrap().layouts.contains_key(&id));
}

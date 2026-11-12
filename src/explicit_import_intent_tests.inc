use super::*;
use super::macropad_tests::LegacyResidentFixture;
use crate::application_layouts::{DeviceApplicationLayouts, DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};

// Adapt the existing complete-import stateful HID fixture, without accessor prewarming after owner attachment.
fn import_intent_fixture_for(sm62: bool) -> (EntropyApp, crate::hid::TestHidRecorder, String) {
    let mut app = EntropyApp::new_inert_for_test();
    // Most cases model firmware with explicitly unsupported custom icon storage.
    // Cold-connection regressions override this to None and script production reads.
    app.display_settings.pictograms.supported = Some(false);
    let device = Device {
        name: if sm62 { "SM62" } else { "M4CR0Pad v3" }.into(),
        vendor_id: 0xE126,
        product_id: if sm62 { 0x00b2 } else { 0x0042 },
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
    if sm62 {
        let keys: Vec<String> = crate::application_layouts::SM62_APPLICATION_POSITIONS.iter()
            .map(|(r,c)| format!("{r},{c}")).collect();
        layout = KeyboardLayout::from_vial_json(&serde_json::json!({
            "name":"SM62", "matrix":{"rows":10,"cols":7}, "layouts":{"keymap":[keys]}
        })).unwrap();
    }
    let layers = if sm62 {16} else {8};
    layout.layers = vec![vec![4.into(); layout.keys.len()]; layers];
    layout.encoder_layers = vec![vec![5; layout.encoders.len()]; layers];
    // This manually connected fixture declares a confirmed legacy QMK schema.
    layout.live_features.qmk_settings_confirmed = true;
    app.layout = Some(layout);
    app.layer_count = layers;
    app.layer_names = (0..layers).map(|i| format!("Firmware {i}")).collect();
    app.keycode_picker.macro_texts.clear();
    app.vial_unlocked = Some(true);
    app.default_layout_device_reconciled = true;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    // Synthetic firmware memory, not a bypass of production writeback validation.
    let mut keys = std::collections::BTreeMap::new();
    recorder.respond_as_legacy(move |request| match request[0] {
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
    app.hid_device = Some(hid.into());
    let key = app.application_layout_device_key().unwrap();
    let mut settings = DeviceApplicationLayouts::default();
    if sm62 { settings.layouts.get_mut(DEFAULT_APPLICATION_LAYOUT_ID).unwrap().layers = vec![vec![4;62];16]; }
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
        if sm62 { continue; }
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

fn import_intent_attach(app: &mut EntropyApp) -> String {
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
fn explicit_import_first_owner_zero_file_counter_publishes() { import_first_owner_action(false, 0); }
#[test]
fn explicit_import_first_owner_equal_file_counter_publishes() { import_first_owner_action(true, 0); }
#[test]
fn explicit_import_pending_intent_does_not_authorize_replacement() { import_first_owner_action(false, 1); }
#[test]
fn explicit_import_pending_intent_does_not_authorize_new_generation() { import_first_owner_action(false, 2); }

fn import_first_owner_action(equal: bool, successor: u8) {
    let storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app, _, application) = import_intent_fixture_for(true);
    app.current_device_name = "SM62".into();
    app.deferred_device_load = DeferredDeviceLoadState::complete(16);
    let key = app.application_layout_device_key().unwrap();
    app.app_settings.application_layouts.insert("unrelated-import-device".into(), DeviceApplicationLayouts::default());
    let unrelated = app.app_settings.application_layouts["unrelated-import-device"].clone();
    let previous = app.application_layout_settings().unwrap().layouts[&application].user_edit_revision;
    let mut bundle = app.entlayout_snapshot_with_profiles(None).unwrap();
    let source = bundle.application_layouts.as_mut().unwrap().get_mut(&application).unwrap();
    source.layers[0][0] = 37;
    source.user_edit_revision = if equal { previous } else { 0 };
    let path = storage.path().join("explicit-complete.entlayout");
    write_entlayout_file(&path, &bundle, app.app_settings.language).unwrap();
    let owner = import_intent_attach(&mut app);
    // Import through the production file reader, validation, sidecars, HID and persistence gates.
    let report = app.import_entlayout_from_path(&path).unwrap();
    assert!(report.contains("Application layouts:"), "{report}");
    assert_eq!(app.app_settings.application_layouts[&key].layouts[&application].layers[0][0], 37);
    assert_eq!(load_app_settings().application_layouts[&key].layouts[&application].layers[0][0], 37);
    assert_eq!(load_app_settings().application_layouts["unrelated-import-device"], unrelated);
    if successor == 1 {
        app.qmk_hid_hosts.clear();
        import_intent_attach(&mut app);
    } else if successor == 2 {
        let bridge = app.qmk_hid_hosts.get_mut(&owner).unwrap();
        let previous = bridge.application_layout_publication_generation().unwrap();
        bridge.clear_application_layout_binding();
        bridge.bind_selected_application_layout_device(key.clone());
        assert_ne!(bridge.application_layout_publication_generation(), Some(previous));
    }
    for _ in 0..2 { app.update_application_layout_runtime(); }
    let library = app.qmk_hid_hosts[&owner].test_published_library();
    if successor != 0 {
        assert!(library.is_empty(), "old import intent authorized a successor owner");
    } else {
        let profile = library.iter().find(|p| p.profile_id.as_ref() == Some(&application))
            .expect("first-owner explicit complete import was swallowed as provenance");
        assert_eq!(profile.layers[0][0], 37);
        assert_eq!(library.len(), 1, "import authorized unrelated saved profiles");
        assert_ne!(app.app_settings.application_layouts[&key].layouts[&application].user_edit_revision, previous,
            "explicit import must establish local authorship independently of its file counter");
    }
}

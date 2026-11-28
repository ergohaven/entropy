// Real shared geometry/snapshot/file/publisher owners; HID is modeled, not hardware.
#[test]
fn matrix_v9_all_official_descriptors_roundtrip_without_presets_or_truncation() {
    use crate::application_layouts::ApplicationLayoutGeometry as Geometry;
    for (pid, count, definition) in [
        (
            0x0052,
            60,
            include_str!("../tests/fixtures/phenom-geometry-v9.json"),
        ),
        (
            0x0053,
            48,
            include_str!("../tests/fixtures/phenom_mini-geometry-v9.json"),
        ),
        (
            0x0054,
            42,
            include_str!("../tests/fixtures/phenom_micro-geometry-v9.json"),
        ),
    ] {
        let (mut app, recorder, _) = fixture_for(false);
        let mut device = app.device_manager.devices()[0].clone();
        device.product_id = pid;
        device.name = format!("Phenom {pid:04x}");
        let mut layout =
            KeyboardLayout::from_vial_json(&serde_json::from_str(definition).unwrap()).unwrap();
        assert_eq!(layout.keys.len(), count);
        assert_eq!(layout.encoders.len(), 4);
        layout.layers = (0..16)
            .map(|layer| {
                (0..count)
                    .map(|key| crate::keyboard::KeyBinding::Vial(4 + ((layer + key) % 26) as u16))
                    .collect()
            })
            .collect();
        layout.encoder_layers = (0..16)
            .map(|layer| (0..4).map(|i| 4 + ((layer + i) % 26) as u16).collect())
            .collect();
        layout.live_features.qmk_settings_confirmed = true;
        let geometry = Geometry::for_layout(&layout).unwrap();
        assert!(geometry.valid());
        assert_eq!(device.application_layout_protocol_version(), Some(9));
        app.device_manager.replace_devices(vec![device.clone()]);
        app.device_about_info.as_mut().unwrap().product_id = pid;
        app.device_about_info
            .as_mut()
            .unwrap()
            .application_layout_geometry = Some(geometry);
        app.layer_count = 16;
        app.layer_names = (0..16).map(|i| format!("Native {i}")).collect();
        app.deferred_device_load = crate::app::DeferredDeviceLoadState::complete(16);
        app.layout = Some(layout.clone());
        app.app_settings.application_layouts.clear();
        let key = app.application_layout_device_key().unwrap();
        assert!(key.ends_with(&format!("-phenom-{pid:04x}")));
        let seed = EntropyApp::base_application_layers(&layout);
        assert!(geometry.complete(&seed));
        for (i, k) in layout.keys.iter().enumerate() {
            let control = usize::from(k.row) * 8 + usize::from(k.col);
            assert_eq!(
                EntropyApp::application_control_for_key(&layout, i),
                Some(control)
            );
            for l in 0..16 {
                assert_eq!(seed[l][control], layout.get_keycode(l, i));
            }
        }
        for (i, e) in layout.encoders.iter().enumerate() {
            let control = 80 + 2 * usize::from(e.encoder_idx) + usize::from(e.direction);
            assert_eq!(
                EntropyApp::application_control_for_encoder(&layout, i),
                Some(control)
            );
            for l in 0..16 {
                assert_eq!(seed[l][control], layout.get_encoder_keycode(l, i));
            }
        }
        let settings = app.application_layout_settings_mut().unwrap();
        assert!(!settings.provision_builtin_presets(None));
        assert_eq!(settings.layouts.len(), 1);
        let id = settings.create_for_application(&DetectedApplication {
            executable: "authored-app".into(),
            display_name: "Custom".into(),
            ..Default::default()
        });
        settings
            .layouts
            .get_mut(&id)
            .unwrap()
            .set_keycode(15, 83, 0x1d);
        settings.normalize();
        assert_eq!(settings.layouts[&id].layers[15][83], 0x1d);
        let mut restored: DeviceApplicationLayouts =
            serde_json::from_slice(&serde_json::to_vec(settings).unwrap()).unwrap();
        restored.normalize();
        assert_eq!(restored, *settings);
        let snapshot = restored.snapshot_for_layout(&restored.layouts[&id]);
        let packets = crate::qmk_hid_host::test_application_layout_transmission_packets(&snapshot);
        assert!(packets.iter().all(|p| p[1] == 9));
        assert_eq!(packets.iter().filter(|p| p[0] == 0xe2).count(), 16 * 7);
        assert_eq!(packets.iter().filter(|p| p[0] == 0xe7).count(), 16 * 4);
        assert_eq!(packets.iter().filter(|p| p[0] == 0xe9).count(), 16 * 4);
        assert!(packets
            .iter()
            .filter(|p| p[0] == 0xe9)
            .all(|p| p[4..].iter().all(|b| *b == 0)));
        assert_eq!(&packets.last().unwrap()[9..11], &[0x57, 2]);
        let mut bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(
            device.clone(),
            Default::default(),
            None,
            crate::qmk_hid_host::HostProtocol::Selected(false),
        );
        bridge.set_selected_application_layout_support(true);
        assert!(!bridge.supports_application_layouts());
        bridge.set_selected_application_layout_geometry(Some(geometry));
        bridge.set_selected_application_layout_support(true);
        bridge.bind_selected_application_layout_device(key);
        let generation = bridge.application_layout_publication_generation().unwrap();
        assert!(
            bridge.publish_application_layout_snapshot_for_generation(snapshot.clone(), generation)
        );
        let mut corrupt = snapshot;
        corrupt.layers[0].truncate(15);
        assert!(!bridge.publish_application_layout_snapshot_for_generation(corrupt, generation));
        let document = app.entlayout_snapshot().unwrap();
        assert_eq!(document.version, MATRIX_APPLICATION_ENTLAYOUT_VERSION);
        app.validate_entlayout_file(&document).unwrap();
        let reloaded: EntLayoutFile =
            serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
        app.validate_entlayout_file(&reloaded).unwrap();
        assert_eq!(
            reloaded.application_layouts.unwrap()[&id].layers[15][83],
            0x1d
        );
        assert!(
            recorder.requests().is_empty(),
            "snapshot/validation must not send HID writes"
        );
        app.device_about_info
            .as_mut()
            .unwrap()
            .application_layout_geometry = None;
        assert!(
            !app.application_layouts_supported(),
            "old/unknown Phenom must hide application actions"
        );
    }
    let mut macropad = DeviceApplicationLayouts::default();
    macropad.geometry = Geometry::legacy(7);
    assert!(macropad.provision_builtin_presets(None));
    assert!(macropad.layouts.len() > 1);
    let mut sm62 = DeviceApplicationLayouts::default();
    sm62.geometry = Geometry::legacy(8);
    sm62.layouts.get_mut("default").unwrap().layers = vec![vec![4; 62]; 16];
    let mut expected_sm62 = sm62.clone();
    expected_sm62.builtin_presets_provisioned = true;
    assert!(sm62.provision_builtin_presets(None));
    assert_eq!(
        sm62, expected_sm62,
        "SM62 cleanup must only persist its completion marker"
    );
    assert!(!sm62.provision_builtin_presets(None));
    assert_eq!(sm62, expected_sm62, "SM62 cleanup must be idempotent");
    assert_eq!(sm62.layouts.len(), 1);
}

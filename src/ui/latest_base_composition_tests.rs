// Composition tests: upstream empty-name authority + accepted SM62 defaults.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn sm62_latest_base_confirmed_empty_names_replace_stale_default_without_native_writes() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let mut app = name_review_device_app(true);
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    app.deferred_device_load = DeferredDeviceLoadState::complete(16);
    app.layer_names = vec![String::new(); 16];
    app.layout.as_mut().unwrap().layer_names = app.layer_names.clone();
    app.confirmed_firmware_layer_names = 0xffff;
    app.default_layout_device_reconciled = false;
    app.application_layout_settings_mut().unwrap().layouts.get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
        .unwrap().layer_names = vec!["Stale host-only label".into(); 16];
    app.maybe_sync_default_application_layout_to_device();
    let settings = app.application_layout_settings().unwrap();
    assert!(settings.layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layer_names.iter().all(String::is_empty));
    assert!(recorder.requests().is_empty(), "reconciliation must never rewrite the native half");
    let saved = crate::app::app_storage::load_app_settings();
    let key = app.application_layout_device_key().unwrap();
    assert!(saved.application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layer_names.iter().all(String::is_empty));
    let backups = app_settings_path().parent().unwrap().join("backups");
    assert!(std::fs::read_dir(backups).unwrap().flatten().any(|entry|
        std::fs::read_to_string(entry.path()).is_ok_and(|data| data.contains("Stale host-only label"))));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn sm62_latest_base_runtime_gate_includes_selected_and_background_v8() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let mut app = name_review_device_app(true);
    assert!(app.application_layout_runtime_needed());
    let sm62 = app.device_manager.devices()[0].clone();
    let mut k04 = crate::device::test_usb_device("/nonexistent/k04-gate", 9, 4);
    k04.name = "K:04".into();
    app.device_manager.replace_devices(vec![sm62.clone(), k04]);
    app.selected_device = Some(1);
    assert!(!app.application_layout_runtime_needed());
    let bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(sm62.clone(), Default::default(), None,
        crate::qmk_hid_host::HostProtocol::Discover);
    app.qmk_hid_hosts.insert(sm62.path.clone(), bridge);
    assert!(app.application_layout_runtime_needed());
}

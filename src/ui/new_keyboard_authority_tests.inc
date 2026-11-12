#[test]
fn sm62_new_keyboard_must_not_receive_previous_keyboard_saved_profile() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=EntropyApp::new_inert_for_test();
    let foreground=crate::application_layouts::DetectedApplication { executable:"unit-isolation-browser".into(),..Default::default() };
    let mut prior_profile=None;
    for unit in ["unit-a","unit-b"] {
        app.qmk_hid_hosts.clear();app.selected_device=None;app.clear_connected_keyboard_state("");
        let mut device=m4cr0pad_v3_device();device.name="SM62".into();device.product_id=0x00b2;
        device.serial_number=unit.into();device.instance_token=unit.into();
        device.path=format!("{}/src/{}.rs",env!("CARGO_MANIFEST_DIR"),if unit=="unit-a"{"main"}else{"app"});
        app.device_manager.replace_devices(vec![device.clone()]);app.selected_device=Some(0);
        app.current_keyboard_id=Some(0x37434f4c32364d53);app.current_device_name="SM62".into();
        app.layout=Some(sm62_test_layout());
        let (hid,rec,_)=runtime_fixture_hid(Vec::new());
        let supported=hid.probe_application_layout_protocol_version(8).unwrap();assert!(supported);
        app.device_about_info=Some(DeviceAboutInfo {supports_application_layouts:supported,vendor_id:device.vendor_id,product_id:device.product_id,path:device.path.clone(),..Default::default()});
        let bridge=crate::qmk_hid_host::test_start_profile_handoff_bridge(device.clone(),hid.shared_output());
        bridge.set_selected_application_layout_support(supported);app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        runtime_fixture_connect(&mut app,&device,hid);
        let key=app.application_layout_device_key().unwrap();
        if unit=="unit-a" {
            let s=app.application_layout_settings_mut().unwrap();let id=s.create_for_application(&foreground);
            let p=s.layouts.get_mut(&id).unwrap();p.layers=vec![vec![4;62];16];p.set_keycode(0,0,0x41);p.layer_names[0]="Keyboard A only".into();
            prior_profile=Some(crate::application_layouts::ApplicationLayoutSnapshot::from_layout(p));
            assert!(app.activate_application_layout(&id));
        }
        app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot {
            foreground:Some(foreground.clone()),foreground_status:crate::app_discovery::ForegroundStatus {backend:"synthetic OS".into(),state:crate::app_discovery::ForegroundState::Focused(foreground.clone())},..Default::default()
        });
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
        while !rec.requests().iter().any(|r|r[0]==0xe3) && std::time::Instant::now()<deadline {std::thread::sleep(std::time::Duration::from_millis(10));}
        let requests=rec.requests();app.qmk_hid_hosts.clear();
        if unit=="unit-a" {assert!(requests.iter().any(|r|r[0]==0xe3),"control A must publish its selected profile");}
        else {
            // No read/restore/import of B's resident catalog was performed and
            // no user authorized copying A's host library onto B.
            assert!(!requests.iter().any(|r|r[0]==0xe1||r[0]==0xe2||r[0]==0xe3||r[0]==0xec),
                "new physical SM62 received another keyboard's cached active profile before device-authoritative readback (key={key}, previous={:?})",prior_profile.as_ref().unwrap().profile_id);
        }
    }
}

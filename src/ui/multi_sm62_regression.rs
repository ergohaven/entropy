fn twin_sm62_browser_snapshot()->crate::app_discovery::ApplicationDiscoverySnapshot {
    // Controlled OS aliases let the ordinary per-profile checkbox change the
    // resolved browser profile without a focus change after USB unplug.
    let foreground=crate::application_layouts::DetectedApplication {
        executable:"firefox".into(),identities:vec!["chrome".into()],
        display_name:"Two SM62 Browser".into(),..Default::default()
    };
    crate::app_discovery::ApplicationDiscoverySnapshot {
        foreground:Some(foreground.clone()),
        foreground_status:crate::app_discovery::ForegroundStatus {backend:"controlled OS foreground".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},..Default::default()
    }
}
struct TwinSm62ReadFirst {
    app:EntropyApp, a:crate::device::Device, b:crate::device::Device,
    key_a:String, key_b:String, id:String, alternate_id:String,
    raw_a:Vec<u8>, raw_b:Vec<u8>, raw_after:Vec<u8>,
    packets_a:crate::hid::TestHidRecorder, packets_b:crate::hid::TestHidRecorder,
}
fn twin_sm62_fixture()->TwinSm62ReadFirst {
    let mut app=EntropyApp::new_inert_for_test();
    let mut a=m4cr0pad_v3_device();a.name="SM62".into();a.product_id=0x00b2;
    a.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
    a.serial_number="C281FE0DD04A01230000000000000000".into();a.instance_token="SM62-unit-A-enumeration".into();
    let mut b=a.clone();b.path=format!("{}/src/app.rs",env!("CARGO_MANIFEST_DIR"));
    b.serial_number="C281FE0DD04A04560000000000000000".into();b.instance_token="SM62-unit-B-enumeration".into();
    assert!(!a.may_share_physical_device(&b));
    app.device_manager.replace_devices(vec![a.clone(),b.clone()]);
    let (id,raw_a)=runtime_fixture_builtin_record("firefox","Unit A browser",91);
    let (id_b,raw_b)=runtime_fixture_builtin_record("firefox","Unit B browser",92);
    let (alternate_id,raw_after)=runtime_fixture_builtin_record_at_slot("google_chrome","Unit B alternate",93,8);
    assert_eq!(id,id_b); // Same menu identity is NOT shared device content.
    let (hid_b,packets_b,_)=runtime_fixture_hid(vec![(7,raw_b.clone()),(8,raw_after.clone())]);
    runtime_fixture_connect(&mut app,&b,hid_b);
    let key_b=app.application_layout_device_key().unwrap();
    let id = runtime_fixture_adopt(&mut app,&b.path,&key_b,&id,&raw_b,&packets_b);
    let alternate_id = runtime_fixture_adopt(&mut app,&b.path,&key_b,&alternate_id,&raw_after,&packets_b);
    // Explicit host matching policy, independent of installed-app discovery.
    // Leave byte-exact confirmed device payloads and their revisions untouched.
    for profile in [&id,&alternate_id] {
        let settings=app.app_settings.application_layouts.get_mut(&key_b).unwrap();
        settings.pending_builtin_auto_bind.remove(profile);
        settings.layouts.get_mut(profile).unwrap().automatic_switching=false;
        settings.layouts.get_mut(profile).unwrap().executable =
            if profile == &id { "firefox" } else { "chrome" }.into();
    }
    assert!(app.handoff_selected_qmk_hid_host_bridge(&b,&a));
    let (hid_a,packets_a,_)=runtime_fixture_hid(vec![(7,raw_a.clone())]);
    runtime_fixture_connect(&mut app,&a,hid_a);
    let key_a=app.application_layout_device_key().unwrap();
    let adopted_a = runtime_fixture_adopt(&mut app,&a.path,&key_a,&id,&raw_a,&packets_a);
    assert_eq!(adopted_a, id, "same device record identity must retain its menu identity");
    let settings=app.app_settings.application_layouts.get_mut(&key_a).unwrap();
    settings.pending_builtin_auto_bind.remove(&id);
    settings.layouts.get_mut(&id).unwrap().automatic_switching=false;
    settings.layouts.get_mut(&id).unwrap().executable="firefox".into();
    assert_ne!(key_a,key_b);
    for (key,raw,code) in [(&key_a,&raw_a,91),(&key_b,&raw_b,92)] {
        let settings=&app.app_settings.application_layouts[key];
        assert_eq!(&settings.device_raw_profiles[&id],raw);
        assert_eq!(settings.layouts[&id].layers[15][61],code);
        assert!(!settings.layouts[&id].automatic_switching);
    }
    assert!(!app.app_settings.application_layouts[&key_a].device_raw_profiles.contains_key(&alternate_id));
    assert!(!packets_a.requests().iter().chain(packets_b.requests().iter()).any(|q|q[..3]==[0xef,9,2]));
    // Explicit ordinary automatic-switching checkboxes only after each unit's
    // independently confirmed catalog exists; no model-wide library seeding.
    for key in [&key_a,&key_b] {
        app.app_settings.application_layouts.get_mut(key).unwrap().layouts.get_mut(&id).unwrap().automatic_switching=true;
    }
    TwinSm62ReadFirst {app,a,b,key_a,key_b,id,alternate_id,raw_a,raw_b,raw_after,packets_a,packets_b}
}
#[test]
fn two_sm62_verified_distinct_owners_both_follow_browser(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let TwinSm62ReadFirst {mut app,a,b,key_a,key_b,id,raw_a,raw_b,packets_a,packets_b,..}=twin_sm62_fixture();
    let cursor_a=packets_a.requests().len();let cursor_b=packets_b.requests().len();
    app.update_application_layout_runtime_with(twin_sm62_browser_snapshot());
    assert_eq!(app.qmk_hid_hosts[&a.path].application_layout_device_key(),Some(key_a.as_str()));
    assert_eq!(app.qmk_hid_hosts[&b.path].application_layout_device_key(),Some(key_b.as_str()));
    assert_eq!(app.app_settings.application_layouts[&key_a].active_layout_id,id);
    assert_eq!(app.app_settings.application_layouts[&key_b].active_layout_id,id);
    runtime_fixture_wait_select(&packets_a,cursor_a,&raw_a);
    runtime_fixture_wait_select(&packets_b,cursor_b,&raw_b);
    assert!(!packets_a.requests()[cursor_a..].iter().any(|q|q[..3]==[0xef,9,2]&&q[28..32]==runtime_fixture_crc(&raw_b).to_le_bytes()));
    assert!(!packets_b.requests()[cursor_b..].iter().any(|q|q[..3]==[0xef,9,2]&&q[28..32]==runtime_fixture_crc(&raw_a).to_le_bytes()));
}
#[test]
fn remaining_sm62_rebinds_after_first_unplug_without_editor_selection(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let TwinSm62ReadFirst {mut app,b,key_a,key_b,id,alternate_id,raw_b,raw_after,packets_b,..}=twin_sm62_fixture();
    let cursor=packets_b.requests().len();
    app.update_application_layout_runtime_with(twin_sm62_browser_snapshot());
    runtime_fixture_wait_select(&packets_b,cursor,&raw_b);
    let archived_a=serde_json::to_value(&app.app_settings.application_layouts[&key_a].layouts).unwrap();
    let (tx,rx)=std::sync::mpsc::channel();
    let (connect_tx,connect_rx)=std::sync::mpsc::channel();app.test_connect_requests=Some(connect_tx);
    app.device_scan_state=DeviceScanState::Scanning {rx,started_at:std::time::Instant::now(),generation:app.connection_generation,timeout_logged:false,usb_only:true};
    tx.send(Ok(vec![b.clone()])).unwrap();
    app.poll_device_scan(&egui::Context::default());
    assert!(connect_rx.try_recv().is_err(),"USB metadata reconciliation must not open another HID owner");
    assert!(app.selected_device.is_none());
    assert!(app.qmk_hid_hosts.contains_key(&b.path),"unplug cleanup retired the unrelated remaining owner");
    let packet_cursor=packets_b.requests().len();
    // Same foreground, different complete device-owned content through ordinary
    // matching rules; the immutable fixture cannot pretend a host STORE won.
    let settings=app.app_settings.application_layouts.get_mut(&key_b).unwrap();
    settings.layouts.get_mut(&id).unwrap().automatic_switching=false;
    // A disabled strongest match intentionally keeps the active profile. Make
    // the old rule inapplicable instead of assuming weaker aliases win over it.
    settings.layouts.get_mut(&id).unwrap().title_contains="Other window".into();
    settings.layouts.get_mut(&alternate_id).unwrap().automatic_switching=true;
    app.update_application_layout_runtime_with(twin_sm62_browser_snapshot());
    assert_eq!(app.qmk_hid_hosts[&b.path].application_layout_device_key(),Some(key_b.as_str()));
    assert_eq!(app.app_settings.application_layouts[&key_b].active_layout_id,alternate_id);
    assert_eq!(app.app_settings.application_layouts[&key_b].layouts[&alternate_id].layers[15][61],93);
    runtime_fixture_wait_select(&packets_b,packet_cursor,&raw_after);
    assert_eq!(serde_json::to_value(&app.app_settings.application_layouts[&key_a].layouts).unwrap(),archived_a,"remaining unit modified unplugged unit's bindings");
}
#[test]
fn two_sm62_still_refuses_aliases_ambiguous_units_and_replaced_enumerations(){
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for mode in 0..3 {
        let TwinSm62ReadFirst {mut app,mut a,b,packets_b,..}=twin_sm62_fixture();
        let cursor=packets_b.requests().len();
        let mut live_b=b.clone();let mut devices=Vec::new();
        match mode {
            0=>{let mut alias=b.clone();alias.path=format!("{}/src/device.rs",env!("CARGO_MANIFEST_DIR"));devices.push(alias);},
            1=>{a.serial_number.clear();live_b.serial_number.clear();},
            2=>{live_b.instance_token="replacement-enumeration-not-queried".into();},
            _=>unreachable!(),
        }
        devices.extend([a,live_b]);app.device_manager.replace_devices(devices);
        app.update_application_layout_runtime_with(twin_sm62_browser_snapshot());
        assert!(app.qmk_hid_hosts[&b.path].application_layout_device_key().is_none(),"unsafe owner was granted in case {mode}");
        std::thread::sleep(std::time::Duration::from_millis(30));
        // Read-only reconciliation and native EA cleanup are not publication.
        assert!(!packets_b.requests()[cursor..].iter().any(|q|matches!(q[0],0xe1..=0xe5|0xe7|0xe9)||q[..2]==[0xef,9]&&matches!(q[2],1|2|4)),"profile packets reached unsafe owner in case {mode}");
    }
}

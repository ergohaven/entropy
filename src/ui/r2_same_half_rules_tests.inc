#[test]
fn sm62_review_same_half_replug_restores_only_confirmed_profile_rules() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let serial_a="C281FE0DD04A01230000000000000000";
    let serial_b="C281FE0DD04A04560000000000000000";
    let mut first=name_review_device_app(true);
    let mut device=first.device_manager.devices()[0].clone();
    device.serial_number=serial_a.into();device.instance_token="attachment-before-replug".into();
    first.device_manager.replace_devices(vec![device.clone()]);
    first.update_application_layout_runtime_with(Default::default());
    let old_key=first.application_layout_device_key().unwrap();
    let settings=first.application_layout_settings_mut().unwrap();
    let id=settings.create_for_application_named(&crate::application_layouts::DetectedApplication{
        executable:"rule-repair-helper".into(),identities:vec!["org.example.RuleRepair".into()],display_name:"User rule".into(),..Default::default()
    },Some("My custom rule"),"work-session");
    settings.category_names.insert("custom:77".into(),"My work".into());
    settings.layouts.get_mut(&id).unwrap().custom_category_id=Some("custom:77".into());
    let (hid,rec)=crate::hid::HidDevice::test_device();
    rec.respond_by(|p|{if p[0]==0xeb{return Some(Ok(*p));}if p[0]==0xe6{let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(r));}None});
    let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
    bridge.bind_selected_application_layout_device(old_key.clone());bridge.set_selected_application_layout_support(true);
    bridge.test_set_device_inventory_for_host_id(&id);
    first.shared_hid_output=hid.shared_output();first.hid_device=Some(hid.into());first.qmk_hid_hosts.insert(device.path.clone(),bridge);
    first.update_application_layout_runtime_with(Default::default());
    assert_eq!(first.application_layout_settings().unwrap().layouts[&id].layers[0][0],9);
    let original_identities=first.application_layout_settings().unwrap().layouts[&id].application_identities.clone();
    // These old drafts are deliberately NOT the bytes returned by the new owner.
    first.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,12);
    first.application_layout_settings_mut().unwrap().layouts.get_mut("default").unwrap().layers[0][0]=42;
    first.update_application_layout_runtime_with(Default::default());
    let saved=serde_json::to_vec(&first.app_settings).unwrap();
    first.qmk_hid_hosts.clear();
    for (serial,restore) in [(serial_a,true),(serial_b,false)] {
        let mut next=name_review_device_app(true);
        next.app_settings=serde_json::from_slice(&saved).unwrap();
        let mut device=next.device_manager.devices()[0].clone();
        device.serial_number=serial.into();device.instance_token=format!("attachment-after-{serial}");
        next.device_manager.replace_devices(vec![device.clone()]);
        let native_default=next.application_layer_seed(next.layout.as_ref().unwrap())[0][0];
        next.update_application_layout_runtime_with(Default::default());
        let key=next.application_layout_device_key().unwrap();
        assert_ne!(key,old_key,"durable rules must not turn the old payload namespace into a new owner's library");
        let (hid,rec)=crate::hid::HidDevice::test_device();
        rec.respond_by(|p|{if p[0]==0xeb{return Some(Ok(*p));}if p[0]==0xe6{let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(r));}None});
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
        bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
        bridge.test_set_device_inventory_for_host_id(&id);
        next.shared_hid_output=hid.shared_output();next.hid_device=Some(hid.into());next.qmk_hid_hosts.insert(device.path.clone(),bridge);
        next.update_application_layout_runtime_with(Default::default());
        let settings=next.application_layout_settings().unwrap();
        if restore {
            let profile=settings.layouts.get(&id).expect("same-half reconnect lost a non-builtin host rule after validated device readback");
            assert_eq!(profile.executable,"rule-repair-helper");
            assert_eq!(profile.application_identities,original_identities,"replug changed the original normalized matching identities");
            assert_eq!(profile.title_contains,"work-session");
            assert!(profile.automatic_switching);
            assert_eq!(profile.custom_category_id.as_deref(),Some("custom:77"));
            assert_eq!(settings.category_names.get("custom:77").map(String::as_str),Some("My work"));
            assert_eq!(profile.layers[0][0],9,"restoring rules replayed an obsolete host payload instead of the device's readback");
        } else {
            assert!(!settings.layouts.contains_key(&id),"another physical half inherited a saved host rule without explicit linking");
            let profile=settings.layouts.values().find(|p|p.id.starts_with("device-resident-7-")).unwrap();
            assert!(profile.executable.is_empty());assert!(!profile.automatic_switching);
            assert_eq!(profile.layers[0][0],9);
        }
        assert_eq!(settings.layouts["default"].layers[0][0],native_default);
        assert!(settings.resident_deleted.is_empty());
        assert_eq!(next.app_settings.application_layouts[&old_key].layouts["default"].layers[0][0],42,"old host draft backup changed");
        assert!(!rec.requests().iter().any(|r|r[0]==0x05||r[..3]==[0xef,9,1]||r[..3]==[0xef,9,4]),"rule recovery issued an unwanted device mutation");
        let foreground=crate::application_layouts::DetectedApplication {
            executable:"rule-repair-helper".into(),identities:vec!["org.example.RuleRepair".into()],
            window_title:"work-session - project".into(),..Default::default()
        };
        next.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot {
            foreground:Some(foreground.clone()),
            foreground_status:crate::app_discovery::ForegroundStatus {backend:"same-half rule recovery fixture".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},
            ..Default::default()
        });
        assert_eq!(next.application_layout_settings().unwrap().active_layout_id,if restore {id.as_str()} else {"default"},
            "recovered rules did not control ordinary foreground routing on the correct half");
        next.qmk_hid_hosts.clear();
    }
}

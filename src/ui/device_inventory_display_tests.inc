#[test]
fn sm62_verified_device_profile_appears_with_its_own_keys_and_names() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);
    let key=app.application_layout_device_key().unwrap();
    app.application_layout_settings_mut().unwrap();
    let device=app.device_manager.devices()[0].clone();
    let (hid,rec)=crate::hid::HidDevice::test_device();
    rec.respond_by(|r|{if r[0]==0xeb{return Some(Ok(*r));}if r[0]==0xe6{let mut p=[0;32];p[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(p));}None});
    let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
    bridge.bind_selected_application_layout_device(key.clone());bridge.set_selected_application_layout_support(true);
    bridge.test_set_valid_device_inventory();
    app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    app.update_application_layout_runtime_with(Default::default());
    let raw=include_bytes!("../qmk_hid_host/resident_stack_max_full.bin");
    let name=String::from_utf8_lossy(&raw[10..10+usize::from(raw[9])]);
    let settings=app.application_layout_settings().unwrap();
    let recovered=settings.layouts.values().find(|p|p.id!="default"&&p.name==name);
    assert!(recovered.is_some(),"validated device-owned resident profile was preserved as a raw backup but never displayed in the editor");
    let recovered=recovered.unwrap();assert_eq!(settings.resident_slots.get(&recovered.id),Some(&7));
    assert_eq!(recovered.layers[0][0],u16::from_le_bytes(raw[64..66].try_into().unwrap()));
    assert!(!recovered.automatic_switching,"do not invent application matching from an unknown device identity");
    let recovered_id=recovered.id.clone();
    assert!(settings.snapshot_for_layout(recovered).device_raw.is_some(),"runtime seeding changed the device's own inherited bindings");
    let published_initial=app.qmk_hid_hosts[&device.path].test_published_library();
    assert!(published_initial.iter().any(|p|p.profile_id.as_ref()==Some(&recovered_id)),"verified firmware UNSET inheritance was mistaken for an incomplete host draft; publication={:?}, library={:?}, shape={:?}, raw={:?}, slots={:?}",app.qmk_hid_hosts[&device.path].application_layout_publication_generation(),published_initial.iter().map(|s|(&s.profile_id,s.device_identity,s.resident_slot)).collect::<Vec<_>>(),recovered.layers.iter().map(Vec::len).collect::<Vec<_>>(),settings.device_raw_profiles.keys().collect::<Vec<_>>(),settings.resident_slots);
    assert!(!rec.requests().iter().any(|r|r[0]==0x05||r[..2]==[0xfe,0x0b]),"readback rewrote native keys or names");
    assert!(app.activate_application_layout(&recovered_id));
    app.update_application_layout_runtime_with(Default::default());
    let active=app.qmk_hid_hosts[&device.path].test_published_snapshot();
    assert!(active.active && active.device_raw.as_deref()==Some(raw.as_slice()),"manual activation replaced a verified device profile with firmware Default");
    assert!(app.application_layout_settings_mut().unwrap().remove(&recovered_id));
    app.update_application_layout_runtime_with(Default::default());
    let published=app.qmk_hid_hosts[&device.path].test_published_library();
    let tombstone=published.iter().find(|p|p.profile_id.as_ref()==Some(&recovered_id)).unwrap();
    assert!(!tombstone.active);
    assert_eq!(tombstone.device_identity.as_ref().map(|i|i.as_slice()),Some(&raw[43..63]),"delete lost the device-owned semantic identity");
}

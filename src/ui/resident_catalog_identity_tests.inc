#[test]
fn sm62_manual_catalog_choice_uses_its_existing_device_record_not_a_duplicate_template() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();app.update_application_layout_runtime_with(Default::default());
    let key=app.application_layout_device_key().unwrap();
    let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_ascii_lowercase().contains("firefox")).unwrap().id.clone();
    let device=app.device_manager.devices()[0].clone();let (hid,rec)=crate::hid::HidDevice::test_device();
    rec.respond_by(|p|{if p[0]==0xeb{return Some(Ok(*p));}if p[0]==0xe6{let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(r));}None});
    let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,crate::qmk_hid_host::HostProtocol::Selected(false),||None);
    bridge.bind_selected_application_layout_device(key);bridge.set_selected_application_layout_support(true);
    bridge.test_set_device_inventory_for_host_id(&id);let record=bridge.resident_recoveries().remove(0);
    // Upgrade from the previous delivered reader, which created a second host ID.
    let orphan=crate::qmk_hid_host::decode_device_profile(&record).unwrap();
    let settings=app.application_layout_settings_mut().unwrap();
    settings.resident_slots.insert(orphan.id.clone(),7);
    settings.device_raw_profiles.insert(orphan.id.clone(),record.raw.clone());
    settings.layouts.insert(orphan.id.clone(),orphan);
    app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    app.update_application_layout_runtime_with(Default::default());
    let s=app.application_layout_settings().unwrap();
    assert_eq!(s.layouts[&id].layers[0][0],9,"standard menu profile ignored the confirmed device-owned bindings and retained its Default-derived template");
    assert_eq!(s.resident_slots.get(&id),Some(&7));
    assert!(s.device_raw_profiles.contains_key(&id));
    assert!(!s.layouts.keys().any(|id|id.starts_with("device-resident-7-")),"one device record appeared twice with different host identities");
    app.activate_application_layout("default");assert!(app.activate_application_layout(&id));app.update_application_layout_runtime_with(Default::default());
    let snapshot=app.qmk_hid_hosts[&device.path].test_published_snapshot();
    assert!(snapshot.active);assert_eq!(snapshot.resident_slot,Some(7));
    assert_eq!(snapshot.device_raw.as_ref(),Some(&record.raw));
    assert_eq!(snapshot.device_identity.as_ref().map(|x|x.as_slice()),Some(&record.identity_crc[..20]));
}

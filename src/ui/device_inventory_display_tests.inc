#[test]
fn sm62_verified_device_profile_appears_with_its_own_keys_and_names() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);
    let device=app.device_manager.devices()[0].clone();
    let raw=include_bytes!("../qmk_hid_host/resident_stack_max_full.bin");
    let (hid,rec,_)=runtime_fixture_hid(vec![(7,raw.to_vec())]);
    runtime_fixture_connect(&mut app,&device,hid);
    let key=app.application_layout_device_key().unwrap();
    let recovered_id=runtime_fixture_adopt(&mut app,&device.path,&key,"production-C archive",raw,&rec);
    let name=String::from_utf8_lossy(&raw[10..10+usize::from(raw[9])]);
    let settings=app.application_layout_settings().unwrap();
    let recovered=&settings.layouts[&recovered_id];
    assert_eq!(recovered.name.as_str(),name.as_ref());
    assert_eq!(settings.resident_slots.get(&recovered_id),Some(&7));
    assert_eq!(recovered.layers[0][0],u16::from_le_bytes(raw[64..66].try_into().unwrap()));
    assert!(!recovered.automatic_switching,"do not invent application matching from an unknown device identity");
    assert_eq!(settings.snapshot_for_layout(recovered).device_raw.as_deref(),Some(raw.as_slice()),"runtime seeding changed the device's own inherited bindings");
    runtime_fixture_assert_archive_backup(&app,&key,raw);
    assert!(!app.qmk_hid_hosts[&device.path].test_published_snapshot().active,"readback alone activated the archive");
    assert!(app.qmk_hid_hosts[&device.path].test_published_library().is_empty());
    let cursor=rec.requests().len();
    assert!(app.activate_application_layout(&recovered_id));
    app.update_application_layout_runtime_with(Default::default());
    let active=r1_assert_active_ram(&app,&device.path,&recovered_id);
    assert_eq!(active.device_raw.as_deref(),Some(raw.as_slice()));
    runtime_fixture_wait_ram(&rec,cursor,raw);
    assert!(app.application_layout_settings_mut().unwrap().remove(&recovered_id));
    for _ in 0..3 { app.update_application_layout_runtime_with(Default::default()); }
    assert!(!app.qmk_hid_hosts[&device.path].test_published_snapshot().active);
    let settings=app.application_layout_settings().unwrap();
    assert!(!settings.layouts.contains_key(&recovered_id),"unchanged readback resurrected a deleted archive profile");
    let tombstone=&settings.resident_deleted[&recovered_id].1;
    assert_eq!(settings.snapshot_for_layout(tombstone).device_identity.as_ref().map(|i|i.as_slice()),Some(&raw[43..63]),"host tombstone lost the archive semantic identity");
    runtime_fixture_assert_archive_backup(&app,&key,raw);
    assert!(!rec.requests().iter().any(|p|p[0]==5||p[..2]==[0xfe,0x0b]||p[..2]==[0xef,9]&&matches!(p[2],1|2|4)),"readback/activation/deletion rewrote native state or the read-only archive");
}

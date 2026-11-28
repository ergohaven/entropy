// Previous readers could leave an archival ID beside a host profile with the
// same semantic digest. A completed migration must preserve both distinct data
// sets, not replace newer host authoring with the archive's keys/names.
fn runtime_fixture_known_host_conflict() -> (EntropyApp,String,String,String,String,Vec<u8>) {
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let key=app.application_layout_device_key().unwrap();
    let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_ascii_lowercase().contains("firefox")).unwrap().id.clone();
    assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));
    let host=app.application_layout_settings().unwrap().layouts[&id].clone();
    let path=r1_attach_inert_owner(&mut app);
    app.qmk_hid_hosts[&path].test_set_device_inventory_for_host_id(&id);
    let record=app.qmk_hid_hosts[&path].completed_legacy_archive().unwrap().remove(0);
    let orphan=crate::qmk_hid_host::decode_device_profile(&record).unwrap();
    let orphan_id=orphan.id.clone();
    let settings=app.application_layout_settings_mut().unwrap();
    settings.resident_slots.insert(orphan_id.clone(),7);
    settings.device_raw_profiles.insert(orphan_id.clone(),record.raw.clone());
    settings.layouts.insert(orphan_id.clone(),orphan);
    let previous=serde_json::to_value(&app.app_settings).unwrap();
    let disk_before=std::fs::read(crate::app::app_storage::app_settings_path()).ok();
    app.update_application_layout_runtime_with(Default::default());
    let settings=app.application_layout_settings().unwrap();
    assert_eq!(settings.layouts[&id],host,"migration overwrote host authorship");
    assert_eq!(settings.layouts[&orphan_id].layers[0][0],9);
    assert_eq!(settings.device_raw_profiles[&orphan_id],record.raw);
    assert_eq!(settings.device_raw_profiles.values().filter(|raw|raw.as_slice()==record.raw.as_slice()).count(),1,"one archive record duplicated during migration");
    let backup=runtime_fixture_assert_archive_backup(&app,&key,&record.raw);
    assert_eq!(backup["records"],serde_json::to_value(vec![record.clone()]).unwrap(),"backup lost or invented archive inventory");
    assert_eq!(backup["previous_host_settings"],previous,"backup did not preserve the complete pre-migration host library");
    assert_eq!(backup["previous_settings_file_bytes"],serde_json::to_value(disk_before).unwrap(),"backup changed the pre-migration settings file bytes");
    (app,path,key,id,orphan_id,record.raw)
}

#[test]
fn sm62_manual_catalog_choice_preserves_host_draft_and_separate_complete_archive() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,path,key,id,orphan_id,raw)=runtime_fixture_known_host_conflict();
    assert!(app.activate_application_layout(&id));
    app.update_application_layout_runtime_with(Default::default());
    let host=r1_assert_active_ram(&app,&path,&id);
    assert_eq!(host.layers[0][0],37,"manual host choice selected archival keys");
    assert!(host.device_raw.is_none(),"new host draft was confused with older raw FULL");
    assert!(app.activate_application_layout(&orphan_id));
    app.update_application_layout_runtime_with(Default::default());
    let archive=r1_assert_active_ram(&app,&path,&orphan_id);
    assert_eq!(archive.resident_slot,Some(7));
    assert_eq!(archive.device_raw.as_ref(),Some(&raw));
    assert_eq!(archive.device_identity.as_ref().map(|x|x.as_slice()),Some(&raw[43..63]));
    runtime_fixture_assert_archive_backup(&app,&key,&raw);
}

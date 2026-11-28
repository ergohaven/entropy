#[test]
fn r1_readback_preserves_authorship_before_next_explicit_edit() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let key=app.application_layout_device_key().unwrap();
    let id=app.application_layout_settings().unwrap().layouts.values()
        .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().user_edit_revision=5;
    app.application_layout_observed_edits.insert((key.clone(),id.clone()),5);
    let before=app.application_layout_settings().unwrap().layouts[&id].clone();
    let path=r1_attach_inert_owner(&mut app);
    app.qmk_hid_hosts[&path].test_set_device_inventory_for_host_id(&id);
    let raw=app.qmk_hid_hosts[&path].resident_recoveries()[0].raw.clone();
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(app.application_layout_settings().unwrap().layouts[&id],before,
        "device readback replaced a host draft or reset authorship");
    runtime_fixture_assert_archive_backup(&app,&key,&raw);
    app.update_application_layout_runtime_with(Default::default());
    assert!(app.application_layout_owner_intents.values().all(|ids|!ids.contains(&id)),
        "readback alone fabricated explicit user intent");
    assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(app.application_layout_settings().unwrap().layouts[&id].user_edit_revision,6);
    assert!(app.application_layout_owner_intents.values().any(|ids|ids.contains(&id)),"explicit post-readback edit intent was lost");
    assert!(app.activate_application_layout(&id));
    app.update_application_layout_runtime_with(Default::default());
    let profile=r1_assert_active_ram(&app,&path,&id);
    assert_eq!(profile.layers[0][0],37);
    assert!(app.application_layout_settings().unwrap().device_raw_profiles.values().any(|bytes|bytes==&raw),"editing discarded the conflicting archive backup");
}

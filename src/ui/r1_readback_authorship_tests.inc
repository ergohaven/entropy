#[test]
fn r1_readback_preserves_authorship_before_next_explicit_edit() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let key=app.application_layout_device_key().unwrap();
    let id=app.application_layout_settings().unwrap().layouts.values()
        .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().user_edit_revision=5;
    app.application_layout_observed_edits.insert((key,id.clone()),5);
    let path=r1_attach_inert_owner(&mut app);
    app.qmk_hid_hosts[&path].test_set_device_inventory_for_host_id(&id);
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(app.application_layout_settings().unwrap().layouts[&id].user_edit_revision,5,
        "device readback reset host authorship and can hide the next explicit user edit");
    app.update_application_layout_runtime_with(Default::default());
    assert!(app.application_layout_owner_intents.values().all(|ids|!ids.contains(&id)),
        "readback alone fabricated explicit user intent");
    assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));
    app.update_application_layout_runtime_with(Default::default());
    let library=app.qmk_hid_hosts[&path].test_published_library();
    let profile=library.iter().find(|p|p.profile_id.as_ref()==Some(&id)).expect("explicit edit after readback was never published");
    assert_eq!(profile.layers[0][0],37);
}

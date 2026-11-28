// Host libraries are durable authority; each physical owner still needs its own
// completed archive migration before any active RAM snapshot is admitted.
fn r1_attach_inert_owner(app: &mut EntropyApp) -> String {
    let key=app.application_layout_device_key().unwrap();
    let device=app.device_manager.devices()[0].clone();
    let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device.clone(),Default::default(),None,crate::qmk_hid_host::HostProtocol::Selected(false));
    bridge.bind_selected_application_layout_device(key);
    bridge.set_selected_application_layout_support(true);
    app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    device.path
}
fn r1_confirm_empty_archive(app: &mut EntropyApp, path: &str) {
    app.qmk_hid_hosts[path].test_set_device_inventory(Vec::new());
    assert_eq!(app.qmk_hid_hosts[path].completed_legacy_archive().unwrap().len(),0);
}
fn r1_assert_active_ram(app: &EntropyApp, path: &str, id: &str) -> crate::application_layouts::ApplicationLayoutSnapshot {
    let snapshot=app.qmk_hid_hosts[path].test_published_snapshot();
    assert!(snapshot.active,"confirmed owner did not receive active RAM");
    assert_eq!(snapshot.profile_id.as_deref(),Some(id));
    assert!(app.qmk_hid_hosts[path].test_published_library().is_empty(),"host library was uploaded to the read-only archive");
    snapshot
}

#[test]
fn r1_linked_host_rule_routes_only_after_owner_archive_confirmation() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let s=app.application_layout_settings_mut().unwrap();
    let p=s.layouts.values_mut().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap();
    p.automatic_switching=true;
    let foreground=crate::application_layouts::DetectedApplication {executable:p.executable.clone(),..Default::default()};
    let id=p.id.clone();
    let discovery=crate::app_discovery::ApplicationDiscoverySnapshot {
        foreground:Some(foreground.clone()),
        foreground_status:crate::app_discovery::ForegroundStatus {backend:"controlled host rule".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},
        ..Default::default()
    };
    app.update_application_layout_runtime_with(discovery.clone());
    // A saved host rule may resolve, but discovery is not an archive receipt.
    assert_eq!(app.application_layout_settings().unwrap().active_layout_id,id);
    assert!(!app.qmk_hid_hosts[&path].test_published_snapshot().active);
    assert!(app.application_layout_owner_intents.values().all(|ids|!ids.contains(&id)),"discovery fabricated edit intent");
    r1_confirm_empty_archive(&mut app,&path);
    app.update_application_layout_runtime_with(discovery);
    r1_assert_active_ram(&app,&path,&id);
    assert_eq!(app.application_layout_settings().unwrap().layouts[&id].user_edit_revision,0);
}

#[test]
fn r1_explicit_edit_and_manual_selection_authorize_only_intended_profile_on_repaint() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for edit in [false,true] {
        let mut app=name_review_cached_sm62_app();
        app.update_application_layout_runtime_with(Default::default());
        let key=app.application_layout_device_key().unwrap();
        let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
        if edit { app.app_settings.application_layouts.get_mut(&key).unwrap().active_layout_id=id.clone(); }
        let path=r1_attach_inert_owner(&mut app);
        r1_confirm_empty_archive(&mut app,&path);
        if edit {assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));}
        else {assert!(app.activate_application_layout(&id));}
        app.update_application_layout_runtime_with(Default::default());
        let snapshot=r1_assert_active_ram(&app,&path,&id);
        if edit {assert_eq!(snapshot.layers[0][0],37);}
        assert!(app.application_layout_owner_intents.values().any(|ids|ids.contains(&id)));
        for _ in 0..3 {app.update_application_layout_runtime_with(Default::default());}
        assert_eq!(app.qmk_hid_hosts[&path].test_published_snapshot(),snapshot);
        let saved=app.application_layout_settings().unwrap().clone();
        let mut restarted=EntropyApp::new_headless();
        assert_eq!(restarted.app_settings.application_layouts[&key],saved,"restart lost the authored host library");
        assert!(restarted.application_layout_owner_intents.is_empty());
        assert!(restarted.application_layout_observed_edits.values().any(|r|*r>0));
        restarted.qmk_hid_hosts.clear();
    }
}

#[test]
fn r1_replaced_owner_requires_fresh_archive_proof_without_losing_host_library() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_cached_sm62_app();
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    r1_confirm_empty_archive(&mut app,&path);
    let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    assert!(app.activate_application_layout(&id));
    app.update_application_layout_runtime_with(Default::default());
    let snapshot=r1_assert_active_ram(&app,&path,&id);
    let old_incarnation=app.qmk_hid_hosts[&path].application_layout_owner_incarnation();
    let generation=app.qmk_hid_hosts[&path].application_layout_publication_generation().unwrap();
    app.qmk_hid_hosts.clear();r1_attach_inert_owner(&mut app);
    assert_eq!(app.qmk_hid_hosts[&path].application_layout_publication_generation(),Some(generation));
    assert_ne!(app.qmk_hid_hosts[&path].application_layout_owner_incarnation(),old_incarnation);
    app.update_application_layout_runtime_with(Default::default());
    assert!(!app.qmk_hid_hosts[&path].test_published_snapshot().active,"old archive proof leaked into a reopen");
    assert!(app.application_layout_owner_intents.is_empty(),"old owner intent leaked into a reopen");
    r1_confirm_empty_archive(&mut app,&path);
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(r1_assert_active_ram(&app,&path,&id),snapshot,"fresh owner proof must not discard the durable host selection");
}

// R1: keep host catalog content separate from owner-scoped publication intent.
fn r1_attach_inert_owner(app: &mut EntropyApp) -> String {
    let key=app.application_layout_device_key().unwrap();
    let device=app.device_manager.devices()[0].clone();
    let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device.clone(),Default::default(),None,crate::qmk_hid_host::HostProtocol::Selected(false));
    bridge.bind_selected_application_layout_device(key);
    bridge.set_selected_application_layout_support(true);
    app.qmk_hid_hosts.insert(device.path.clone(),bridge);
    device.path
}

#[test]
fn r1_unconfirmed_linked_template_never_becomes_active_on_foreground() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let s=app.application_layout_settings_mut().unwrap();
    let p=s.layouts.values_mut().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap();
    p.automatic_switching=true; // installed/discovered app, no explicit edit
    let foreground=crate::application_layouts::DetectedApplication {executable:p.executable.clone(),..Default::default()};
    let id=p.id.clone();
    app.update_application_layout_runtime_with(crate::app_discovery::ApplicationDiscoverySnapshot {
        foreground:Some(foreground.clone()),
        foreground_status:crate::app_discovery::ForegroundStatus {backend:"R1 controlled foreground".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},
        ..Default::default()
    });
    assert_eq!(app.application_layout_settings().unwrap().active_layout_id,"default","discovery invented explicit activation intent");
    assert!(!app.qmk_hid_hosts[&path].test_published_snapshot().active);
    assert!(app.qmk_hid_hosts[&path].test_published_library().is_empty());
    assert!(app.application_layout_settings().unwrap().layouts.contains_key(&id));
}

#[test]
fn r1_explicit_edit_and_manual_selection_authorize_only_intended_profile_on_repaint() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for edit in [false,true] {
        let mut app=name_review_device_app(true);
        app.update_application_layout_runtime_with(Default::default());
        let path=r1_attach_inert_owner(&mut app);
        let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
        if edit {assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));}
        else {app.activate_application_layout(&id);}
        app.update_application_layout_runtime_with(Default::default());
        let library=app.qmk_hid_hosts[&path].test_published_library();
        assert_eq!(library.len(),1);assert_eq!(library[0].profile_id.as_deref(),Some(id.as_str()));
        if edit {assert_eq!(library[0].layers[0][0],37);}
        for _ in 0..3 {app.update_application_layout_runtime_with(Default::default());}
        assert_eq!(app.qmk_hid_hosts[&path].test_published_library(),library);
        let mut restarted=EntropyApp::new_headless();
        // Ordinary startup reads authored counters as a baseline, never a grant.
        assert!(restarted.application_layout_owner_intents.is_empty());
        assert!(restarted.application_layout_observed_edits.values().any(|r|*r>0));
        restarted.qmk_hid_hosts.clear();
    }
}

#[test]
fn r1_replaced_owner_must_not_inherit_previous_owner_intent() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    let mut app=name_review_device_app(true);
    app.update_application_layout_runtime_with(Default::default());
    let path=r1_attach_inert_owner(&mut app);
    let id=app.application_layout_settings().unwrap().layouts.values().find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
    app.activate_application_layout(&id);
    app.update_application_layout_runtime_with(Default::default());
    assert_eq!(app.qmk_hid_hosts[&path].test_published_library().len(),1);
    let generation=app.qmk_hid_hosts[&path].application_layout_publication_generation().unwrap();
    app.qmk_hid_hosts.clear();r1_attach_inert_owner(&mut app);
    // Replacement owners legitimately reuse generation 0; this is not a unit ID.
    assert_eq!(app.qmk_hid_hosts[&path].application_layout_publication_generation(),Some(generation));
    app.update_application_layout_runtime_with(Default::default());
    assert!(app.qmk_hid_hosts[&path].test_published_library().is_empty(),"old owner intent leaked into a reopen");
    assert!(!app.qmk_hid_hosts[&path].test_published_snapshot().active);
}

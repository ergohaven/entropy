#[test]
fn r1_pending_edit_before_owner_replacement_does_not_authorize_successor() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for edit in [false,true] {
        let mut app=name_review_cached_sm62_app();
        app.update_application_layout_runtime_with(Default::default());
        let path=r1_attach_inert_owner(&mut app);
        app.update_application_layout_runtime_with(Default::default());
        let id=app.application_layout_settings().unwrap().layouts.values()
            .find(|p|p.executable.to_lowercase().contains("firefox")).unwrap().id.clone();
        if edit {assert!(app.application_layout_settings_mut().unwrap().layouts.get_mut(&id).unwrap().set_keycode(0,0,37));}
        else {app.activate_application_layout(&id);}
        // Replace before runtime can observe the old owner's pending action.
        app.qmk_hid_hosts.clear();r1_attach_inert_owner(&mut app);
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&path].test_published_library().is_empty(),
            "pending action for the old owner authorized its successor");
        assert!(!app.qmk_hid_hosts[&path].test_published_snapshot().active);
        // A genuinely new action on the successor remains usable.
        app.activate_application_layout(&id);
        app.update_application_layout_runtime_with(Default::default());
        assert_eq!(app.qmk_hid_hosts[&path].test_published_library().len(),1);
    }
}
